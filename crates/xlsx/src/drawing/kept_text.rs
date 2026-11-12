//! A shape's text body kept as the file gave it: the `a:lstStyle` and every `a:p` after
//! `a:bodyPr` (ECMA-376 Part 1 §21.1.2), rebuilt from the parsed tree so several runs, paragraph
//! properties, bullets and list styles are written back while the text is unedited.
//!
//! Only DrawingML text, fill, line, effect and colour elements and their attributes are kept.
//! A body holding anything else (a hyperlink or sound with a relationship id, a picture bullet,
//! extension content, Office Math, markup compatibility) is not kept: a relationship id would
//! dangle in the saved package, and content of other namespaces cannot be rebuilt without them.

use std::fmt::Write as _;

use crate::xml::{El, esc, esc_attr};

/// Longest kept body, in bytes of XML; a longer one is written the usual way.
pub(super) const MAX_KEPT_TEXT_BYTES: usize = 262_144;

/// DrawingML elements a kept text body may hold (all in the `a:` namespace).
const ELEMENTS: &[&str] = &[
    // Text structure (§21.1.2).
    "lstStyle",
    "defPPr",
    "lvl1pPr",
    "lvl2pPr",
    "lvl3pPr",
    "lvl4pPr",
    "lvl5pPr",
    "lvl6pPr",
    "lvl7pPr",
    "lvl8pPr",
    "lvl9pPr",
    "p",
    "pPr",
    "r",
    "rPr",
    "t",
    "br",
    "fld",
    "endParaRPr",
    "defRPr",
    // Paragraph spacing, bullets and tabs.
    "lnSpc",
    "spcBef",
    "spcAft",
    "spcPct",
    "spcPts",
    "buClrTx",
    "buClr",
    "buSzTx",
    "buSzPct",
    "buSzPts",
    "buFontTx",
    "buFont",
    "buNone",
    "buAutoNum",
    "buChar",
    "tabLst",
    "tab",
    // Run properties.
    "highlight",
    "uLnTx",
    "uLn",
    "uFillTx",
    "uFill",
    "latin",
    "ea",
    "cs",
    "sym",
    "rtl",
    // Fills and lines.
    "noFill",
    "solidFill",
    "gradFill",
    "pattFill",
    "grpFill",
    "gsLst",
    "gs",
    "lin",
    "path",
    "fillToRect",
    "tileRect",
    "fgClr",
    "bgClr",
    "ln",
    "prstDash",
    "custDash",
    "ds",
    "round",
    "bevel",
    "miter",
    "headEnd",
    "tailEnd",
    // Effects.
    "effectLst",
    "blur",
    "glow",
    "innerShdw",
    "outerShdw",
    "prstShdw",
    "reflection",
    "softEdge",
    // Colours.
    "scrgbClr",
    "srgbClr",
    "hslClr",
    "sysClr",
    "schemeClr",
    "prstClr",
];

/// Attributes a kept element may carry (all unqualified in DrawingML).
const ATTRIBUTES: &[&str] = &[
    "lang",
    "altLang",
    "sz",
    "b",
    "i",
    "u",
    "strike",
    "kern",
    "cap",
    "spc",
    "normalizeH",
    "baseline",
    "noProof",
    "dirty",
    "err",
    "smtClean",
    "smtId",
    "bmk",
    "marL",
    "marR",
    "lvl",
    "indent",
    "algn",
    "defTabSz",
    "rtl",
    "eaLnBrk",
    "fontAlgn",
    "latinLnBrk",
    "hangingPunct",
    "val",
    "typeface",
    "panose",
    "pitchFamily",
    "charset",
    "char",
    "type",
    "startAt",
    "pos",
    "id",
    "w",
    "cmpd",
    "len",
    "lim",
    "d",
    "sp",
    "flip",
    "rotWithShape",
    "ang",
    "scaled",
    "path",
    "l",
    "t",
    "r",
    "g",
    "hue",
    "sat",
    "lum",
    "lastClr",
    "prst",
    "blurRad",
    "dist",
    "dir",
    "sx",
    "sy",
    "kx",
    "ky",
    "rad",
    "grow",
    "stA",
    "stPos",
    "endA",
    "endPos",
    "fadeDir",
];

fn known_element(name: &str) -> bool {
    ELEMENTS.contains(&name) || super::DML_COLOR_TRANSFORMS.iter().any(|(n, _)| *n == name)
}

/// The XML after `a:bodyPr` of the text body `tb` to keep, or `None` when it holds anything that
/// is not kept or is longer than [`MAX_KEPT_TEXT_BYTES`].
pub(super) fn kept_body_xml(tb: &El) -> Option<String> {
    let mut s = String::new();
    for c in tb.children.iter().filter(|c| c.name != "bodyPr") {
        write_el(&mut s, c)?;
    }
    Some(s)
}

/// Appends `e` with the `a:` prefix; `None` for an element or attribute that is not kept or once
/// the XML outgrows the cap. Nesting is bounded by the parser's depth limit.
fn write_el(s: &mut String, e: &El) -> Option<()> {
    if !known_element(&e.name) {
        return None;
    }
    let _ = write!(s, "<a:{}", e.name);
    for (k, v) in &e.attrs {
        if !ATTRIBUTES.contains(&k.as_str()) {
            return None;
        }
        let _ = write!(s, " {k}=\"{}\"", esc_attr(v));
    }
    let text = if e.name == "t" { esc(&e.text) } else { String::new() };
    if e.children.is_empty() && text.is_empty() && e.name != "t" {
        s.push_str("/>");
    } else {
        s.push('>');
        s.push_str(&text);
        for c in &e.children {
            write_el(s, c)?;
        }
        let _ = write!(s, "</a:{}>", e.name);
    }
    (s.len() <= MAX_KEPT_TEXT_BYTES).then_some(())
}

#[cfg(test)]
mod tests {
    use super::{MAX_KEPT_TEXT_BYTES, kept_body_xml};

    fn body(inner: &str) -> crate::xml::El {
        let xml = format!(r#"<xdr:txBody xmlns:xdr="x" xmlns:a="a"><a:bodyPr/>{inner}</xdr:txBody>"#);
        crate::xml::parse(xml.as_bytes()).unwrap_or_default()
    }

    #[test]
    fn a_body_over_the_cap_is_not_kept() {
        let run = r#"<a:r><a:rPr lang="en-US" b="1"/><a:t>0123456789</a:t></a:r>"#;
        let small = format!("<a:lstStyle/><a:p>{run}</a:p>");
        assert!(kept_body_xml(&body(&small)).is_some());
        let big = format!("<a:lstStyle/><a:p>{}</a:p>", run.repeat(MAX_KEPT_TEXT_BYTES / run.len() + 1));
        assert_eq!(kept_body_xml(&body(&big)), None);
    }

    #[test]
    fn unknown_elements_and_attributes_are_not_kept() {
        assert_eq!(kept_body_xml(&body(r#"<a:p><a:r><a:rPr><a:hlinkClick r:id="rId1"/></a:rPr><a:t>x</a:t></a:r></a:p>"#)), None);
        assert_eq!(kept_body_xml(&body(r#"<a:p><a:r><a:rPr lang="en-US" mystery="1"/><a:t>x</a:t></a:r></a:p>"#)), None);
    }
}
