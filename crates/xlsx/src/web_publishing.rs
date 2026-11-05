//! Bounded workbook web preferences (ECMA-376 Part 1 §18.2.24, CT_WebPublishing).
//! These settings describe a future web export; importing them performs no external access.

use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_model::{PreservedXml, Workbook};

use crate::xml::{El, esc_attr, parse};

const SLOT: &str = "webPublishing";
const MAX_TEXT: usize = 1024;
const MAX_XML: usize = 8192;
const SCREEN_SIZES: &[&str] =
    &["544x376", "640x480", "720x512", "800x600", "1024x768", "1152x882", "1152x900", "1280x1024", "1600x1200", "1800x1440", "1920x1200"];

/// Rebuild only the supported attributes, never copying arbitrary markup or namespaces.
fn sanitized(el: &El) -> String {
    let mut xml = String::from("<webPublishing");
    for name in ["css", "thicket", "longFileNames", "vml", "allowPng", "usePt"] {
        if let Some(value) = el.attr(name) {
            let flag = match value.trim() {
                "true" | "1" => "1",
                "false" | "0" => "0",
                _ => continue,
            };
            let _ = write!(xml, " {name}=\"{flag}\"");
        }
    }
    for name in ["codePage", "dpi"] {
        // Both are unsignedInt in CT_WebPublishing, with independent meanings.
        if let Some(value) = el.attr(name).filter(|v| v.len() <= 32).and_then(|v| v.trim().parse::<u32>().ok()) {
            let _ = write!(xml, " {name}=\"{value}\"");
        }
    }
    if let Some(value) = el.attr("targetScreenSize").filter(|v| SCREEN_SIZES.contains(v)) {
        let _ = write!(xml, " targetScreenSize=\"{value}\"");
    }
    // An xsd:string, retained as text rather than treated as a resource to resolve.
    if let Some(value) = el.attr("characterSet").filter(|v| v.len() <= MAX_TEXT) {
        let _ = write!(xml, " characterSet=\"{}\"", esc_attr(value));
    }
    xml.push_str("/>");
    xml
}

pub(crate) fn read(workbook: &El, wb: &mut Workbook) {
    let Some(el) = workbook.child(SLOT) else { return };
    let preserved = Arc::make_mut(wb.preserved.get_or_insert_with(Default::default));
    preserved.elements.retain(|e| e.slot != SLOT);
    preserved.elements.push(PreservedXml { slot: SLOT.into(), xml: sanitized(el) });
}

pub(crate) fn write(wb: &Workbook, out: &mut String) {
    let Some(kept) = wb.preserved.as_ref().and_then(|p| p.elements.iter().find(|e| e.slot == SLOT)) else { return };
    // Also validate the public preservation storage on write, without trusting raw XML.
    if kept.xml.len() <= MAX_XML
        && let Ok(el) = parse(kept.xml.as_bytes())
        && el.name == SLOT
    {
        out.push_str(&sanitized(&el));
    }
}
