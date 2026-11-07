//! Pass-through of package content GridCraft does not model, so that open → edit → save keeps it.
//!
//! On read, the raw bytes of unmodelled parts are kept with their content type and their
//! relationship graph, per owner: the package root (custom document properties, ribbon
//! customisation, add-in task panes, the sensitivity label), the workbook (custom XML, the volatile
//! dependencies part with the RTD and CUBE topics, the shared workbook revision history and user
//! names, links to external workbooks with
//! their `<externalReferences>`, data connections, XML maps, add-in task panes with the web extension
//! parts they show, and parts other applications link with relationship types outside the OOXML and
//! Microsoft namespaces, such as Google Sheets' workbook metadata, under their source name when it
//! is free) and each sheet (query tables, cells
//! mapped to XML elements, form controls other than the buttons, scroll bars and spin buttons
//! `controls.rs` models, ActiveX controls, OLE objects, the sheet background,
//! printer settings and worksheet custom properties (add-in tags such as `ORB_SHEETNAME`), with the
//! sheet's `<customProperties>`, `<picture>`, `<oleObjects>` and `<controls>` XML, the `r:id` of
//! `<pageSetup>`, the legacy VML shapes those controls need, the DrawingML anchors of the sheet's drawing
//! wrapped in `mc:AlternateContent` that the drawing reader doesn't model (control shapes and Office
//! add-ins, with their web extension part, its own rels and the add-in's snapshot picture) and its SmartArt
//! graphics with their diagram parts, and the
//! pictures of the headers and footers, `&G`, with their `<legacyDrawingHF>`, and the sheet's part of
//! the Custom Views, `<customSheetViews>`). The workbook also keeps its `<fileVersion>` and the
//! calcFeatures extension, which Excel needs to trust the saved values, and its Custom Views
//! (`<customWorkbookViews>`): a sheet keeps only the views the workbook still lists.
//!
//! On write, kept parts get fresh part names where needed and new relationship ids; the `r:id`s in
//! kept XML are rewritten to match (the kept relationships of a sheet's drawing keep their ids: a
//! SmartArt data part, written byte for byte, names one of them). Kept content is conservative:
//! - Anything whose relationship graph reaches a part GridCraft reads and rewrites (worksheets,
//!   drawings, charts, tables, PivotTables, comments, slicers…) is not kept: it would point at
//!   content that no longer exists in the same form.
//! - External relationship targets (the path of a linked workbook) are kept as text and never
//!   opened, followed or refreshed. A relative path to a linked workbook is rewritten when the
//!   workbook is saved into another folder ([`rebase_links`]), so it still names the same file.
//! - Sheet content lives on the sheet: deleting the sheet drops it, Copy Sheet doesn't copy it.
//!   Cell references inside kept parts (a control's linked cell, a query table's range, a cell
//!   mapped to an XML element) are kept as they were, not adjusted to inserted or deleted rows.
//! - Strict Open XML files are passed through as Transitional (what is saved): relationship types
//!   and the namespace declarations of kept XML are translated on read, binary parts are kept byte
//!   for byte, and a part holding values only Strict allows is dropped with a warning.
//! - Sizes are bounded: per part and per workbook.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Component, Path};
use std::sync::Arc;

use gridcraft_model::{Preserved, PreservedAnchor, PreservedPart, PreservedRel, PreservedSheet, PreservedXml, Sheet, Visibility, Workbook};
use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::{Reader, Writer};

use crate::IoError;
use crate::package::{self, normalize};
use crate::read::Ctx;
use crate::write::{NS_REL, Out, Rels, XML_DECL};
use crate::xml::{self, esc_attr};

pub(crate) mod connections;
mod read;
mod rebase;
mod revisions;
mod strict;
mod write;

pub use read::KeptReader;
pub(crate) use read::{map_rel_ids, raw_rels, with_decls};
pub use rebase::{SheetKept, rebase_links};
pub use strict::strict_rel_type_to_transitional;
use strict::{Untranslatable, is_xml_part, to_transitional};
#[cfg(test)]
pub(crate) use write::sheet_names_in;
pub use write::{
    KeptOut, SheetIds, WorkbookKept, custom_view_guids, custom_workbook_views, workbook_elements, write_package_rels, write_theme_relationships,
    write_workbook,
};

/// Most bytes kept for one workbook.
pub const MAX_KEPT_TOTAL: usize = 256 * 1024 * 1024;
/// Largest single kept part.
pub const MAX_KEPT_PART: u64 = 64 * 1024 * 1024;
/// Most parts kept for one workbook.
const MAX_KEPT_PARTS: usize = 4096;
/// Most relationships read from one rels part.
const MAX_RELS: usize = 10_000;
/// Largest relationships, content-types or VML part read here.
const MAX_META: u64 = 16 * 1024 * 1024;
/// Longest chain of relationships followed from an owner.
const MAX_CHAIN: usize = 16;
/// Largest kept XML fragment of a sheet.
const MAX_FRAGMENT: usize = 4 * 1024 * 1024;
/// Deepest element nesting accepted in a kept fragment.
const MAX_FRAGMENT_DEPTH: usize = 256;

#[cfg(test)]
thread_local! {
    /// A smaller kept-bytes allowance for tests on this thread.
    pub(crate) static KEPT_TOTAL_FOR_TEST: std::cell::Cell<usize> = const { std::cell::Cell::new(MAX_KEPT_TOTAL) };
}

fn kept_total() -> usize {
    #[cfg(test)]
    {
        KEPT_TOTAL_FOR_TEST.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        MAX_KEPT_TOTAL
    }
}

const NS_REL_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";

/// Relationship kinds (last segment of the type) kept from the package root.
const PACKAGE_ROOTS: &[&str] = &["custom-properties", "extensibility", "webextensiontaskpanes", "classificationlabels"];
/// Relationship kinds kept from the workbook without a workbook element.
/// The shared workbook revision history (`revisionHeaders`, with the logs it names) is checked and
/// written back only while the sheets are unchanged (see [`revisions`]).
const WORKBOOK_ROOTS: &[&str] = &["customXml", "connections", "xmlMaps", "volatileDependencies", "revisionHeaders", "usernames"];
/// Workbook relationship kinds of Office add-ins: the task panes part, which names the web
/// extension parts it shows ([MS-OWEXML] 2.3).
const WORKBOOK_WEB_EXTENSIONS: &[&str] = &["webextensiontaskpanes"];
/// Relationship kinds kept from a sheet without a sheet element.
const SHEET_ROOTS: &[&str] = &["queryTable", "tableSingleCells"];
/// Sheet relationship kinds that mean the sheet XML may hold elements worth keeping.
const SHEET_ELEMENT_KINDS: &[&str] = &["oleObject", "ctrlProp", "control", "image", "package", "printerSettings"];
/// Attributes of `<fileVersion>` (CT_FileVersion).
const FILE_VERSION_ATTRS: &[&str] = &["appName", "lastEdited", "lowestEdited", "rupBuild", "codeName"];
/// The workbook extension listing the calculation features the saved values were computed with.
const CALC_FEATURES_URI: &str = "{B58B0392-4F1F-4190-BB64-5DF3571DCE5F}";
const NS_CALC_FEATURES: &str = "http://schemas.microsoft.com/office/spreadsheetml/2018/calcfeatures";
/// Kept sheet elements, in schema order (all after `<legacyDrawing>`, before `<tableParts>`).
const SHEET_SLOTS: &[&str] = &["legacyDrawingHF", "picture", "oleObjects", "controls"];
/// Kept sheet elements that come before `<drawing>` in CT_Worksheet (after `<pageSetup>`).
const SHEET_SLOTS_EARLY: &[&str] = &["customProperties"];
/// Kept sheet elements that come between `<autoFilter>`/`<sortState>` and `<mergeCells>` in
/// CT_Worksheet: the sheet's part of the Custom Views (ECMA-376 Part 1 §18.3.1.28).
const SHEET_SLOTS_MID: &[&str] = &["customSheetViews"];
/// Attributes of `<customWorkbookView>` (CT_CustomWorkbookView, ECMA-376 Part 1 §18.2.3).
const CUSTOM_WORKBOOK_VIEW_ATTRS: &[&str] = &[
    "name",
    "guid",
    "autoUpdate",
    "mergeInterval",
    "changesSavedWin",
    "onlySync",
    "personalView",
    "includePrintSettings",
    "includeHiddenRowCol",
    "maximized",
    "minimized",
    "showHorizontalScroll",
    "showVerticalScroll",
    "showSheetTabs",
    "xWindow",
    "yWindow",
    "windowWidth",
    "windowHeight",
    "tabRatio",
    "activeSheetId",
    "showFormulaBar",
    "showStatusbar",
    "showComments",
    "showObjects",
];
/// The content type of a worksheet custom property part (ECMA-376 Part 1 §18.3.1.23).
const CT_CUSTOM_PROPERTY: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.customProperty";

/// Relationship kinds GridCraft reads and rewrites, or that depend on content it rewrites:
/// kept content whose graph reaches one of them is dropped.
const REWRITTEN: &[&str] = &[
    "officedocument",
    "core-properties",
    "extended-properties",
    "worksheet",
    "chartsheet",
    "dialogsheet",
    "xlmacrosheet",
    "xlintlmacrosheet",
    "drawing",
    "chart",
    "chartex",
    "chartstyle",
    "chartcolorstyle",
    "chartusershapes",
    "themeoverride",
    "table",
    "pivottable",
    "pivotcachedefinition",
    "pivotcacherecords",
    "comments",
    "vmldrawing",
    "threadedcomment",
    "person",
    "sharedstrings",
    "styles",
    "theme",
    "sheetmetadata",
    "calcchain",
    "vbaproject",
    "rdrichvalue",
    "rdrichvaluestructure",
    "rdrichvaluetypes",
    "rdrichvaluewebimage",
    "rdarray",
    "rdsupportingpropertybag",
    "rdsupportingpropertybagstructure",
    "richvaluerel",
    "slicer",
    "slicercache",
    "timeline",
    "timelinecache",
    "powerpivotdata",
];

/// Sheet kinds (workbook relationship kinds) kept whole and written back in their place among the
/// sheets: chart sheets, Excel 5 dialog sheets and Excel 4.0 macro sheets. None is modelled.
pub const KEPT_SHEET_KINDS: &[&str] = &["chartsheet", "dialogsheet", "xlMacrosheet", "xlIntlMacrosheet"];
/// The kept sheet kinds that are macros: written only into macro-enabled packages.
const MACRO_SHEET_KINDS: &[&str] = &["xlMacrosheet", "xlIntlMacrosheet"];

/// Relationship kinds a kept sheet reaches that are otherwise rewritten: its drawing, chart and
/// their style, colour and shape parts, and the VML of its controls and notes, are kept byte for
/// byte (the sheet is not modelled).
const KEPT_SHEET_GRAPH: &[&str] = &[
    "chartsheet",
    "dialogsheet",
    "xlmacrosheet",
    "xlintlmacrosheet",
    "drawing",
    "chart",
    "chartex",
    "chartstyle",
    "chartcolorstyle",
    "chartusershapes",
    "themeoverride",
    "vmldrawing",
    "comments",
];

/// Part-name prefixes (lower case) GridCraft's writer never produces: kept parts there keep their
/// original names. Anything else is renamed so it can't collide with written parts.
const OWN_DIRS: &[&str] = &[
    "customxml/",
    "docprops/custom.xml",
    "customui/",
    "xl/externallinks/",
    "xl/connections.xml",
    "xl/xmlmaps.xml",
    "xl/volatiledependencies.xml",
    "xl/querytables/",
    "xl/ctrlprops/",
    "xl/embeddings/",
    "xl/activex/",
    "xl/webextensions/",
    "xl/printersettings/",
];

/// The package-root relationship type of the custom document properties part.
pub(crate) const REL_CUSTOM_PROPERTIES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
/// The content type of the custom document properties part (ECMA-376 Part 1 §22.3).
pub(crate) const CT_CUSTOM_PROPERTIES: &str = "application/vnd.openxmlformats-officedocument.custom-properties+xml";

/// Keeps a package-root part with no relationships of its own (custom document properties read
/// from a binary workbook) the way the .xlsx reader keeps it, added to what the workbook already
/// keeps. A part of the same name already kept wins.
pub(crate) fn keep_package_root_part(wb: &mut Workbook, rel: PreservedRel, content_type: &str, data: Vec<u8>) {
    let mut p = wb.preserved.as_deref().cloned().unwrap_or_default();
    let key = normalize(&rel.target);
    if p.parts.iter().any(|q| normalize(&q.name) == key) {
        return;
    }
    p.parts.push(PreservedPart { name: rel.target.clone(), content_type: content_type.to_string(), data: Arc::from(data), rels: vec![] });
    p.package_rels.push(rel);
    wb.preserved = Some(Arc::new(p));
}

/// Adds content kept by one binary-workbook reader to what an owner (the workbook or a sheet)
/// already keeps, so readers that fill the same store never overwrite each other. Parts whose name
/// is already kept, and relationships whose id the owner already uses, are left out.
pub(crate) fn merge_preserved(into: &mut Option<Arc<Preserved>>, add: Option<Arc<Preserved>>) {
    let Some(add) = add else { return };
    let Some(existing) = into.as_deref() else {
        *into = Some(add);
        return;
    };
    let mut p = existing.clone();
    let add = Arc::unwrap_or_clone(add);
    for part in add.parts {
        let key = normalize(&part.name);
        if !p.parts.iter().any(|q| normalize(&q.name) == key) {
            p.parts.push(part);
        }
    }
    for rel in add.package_rels {
        if !p.package_rels.iter().any(|r| r.id == rel.id) {
            p.package_rels.push(rel);
        }
    }
    for rel in add.rels {
        if !p.rels.iter().any(|r| r.id == rel.id) {
            p.rels.push(rel);
        }
    }
    for rel in add.drawing_rels {
        if !p.drawing_rels.iter().any(|r| r.id == rel.id) {
            p.drawing_rels.push(rel);
        }
    }
    p.elements.extend(add.elements);
    p.sheets.extend(add.sheets);
    p.anchors.extend(add.anchors);
    if p.vml.is_none() {
        p.vml = add.vml;
    }
    *into = Some(Arc::new(p));
}

/// Relationship type namespaces OOXML (Transitional and Strict) and Microsoft define.
const KNOWN_REL_NAMESPACES: &[&str] = &["http://schemas.openxmlformats.org/", "http://purl.oclc.org/ooxml/", "http://schemas.microsoft.com/"];

/// Folders (lower case) where GridCraft's writer puts parts: a third-party part there is renamed.
const WRITTEN_DIRS: &[&str] = &[
    "_rels/",
    "docprops/",
    "xl/_rels/",
    "xl/worksheets/",
    "xl/chartsheets/",
    "xl/dialogsheets/",
    "xl/macrosheets/",
    "xl/charts/",
    "xl/drawings/",
    "xl/media/",
    "xl/tables/",
    "xl/pivottables/",
    "xl/pivotcache/",
    "xl/richdata/",
    "xl/theme/",
    "xl/persons/",
    "xl/threadedcomments/",
    "xl/ctrlprops/",
    "xl/slicers/",
    "xl/slicercaches/",
    "xl/timelines/",
    "xl/timelinecaches/",
    "xl/featurepropertybag/",
];

/// Whether a relationship type is outside the namespaces OOXML and Microsoft define: a part another
/// application links (Google Sheets' `http://customschemas.google.com/relationships/workbookmetadata`).
fn is_third_party(rel_type: &str) -> bool {
    let t = rel_type.trim().to_ascii_lowercase();
    !t.is_empty() && !KNOWN_REL_NAMESPACES.iter().any(|ns| t.starts_with(ns))
}

/// Whether a third-party part can keep its source name on write: not where GridCraft writes its own
/// parts (its folders, the XML and binary parts directly in `xl/` or at the package root, any rels
/// part or the content types part).
fn free_for_third_party(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let (dir, file) = package::split_dir(&lower);
    if file.is_empty() || lower.contains("_rels/") || file.ends_with(".rels") || WRITTEN_DIRS.iter().any(|d| lower.starts_with(d)) {
        return false;
    }
    let ours = |f: &str| f.ends_with(".xml") || f.ends_with(".bin") || f.ends_with(".vml");
    !((dir.is_empty() || dir == "xl") && ours(file))
}

fn kind(rel_type: &str) -> &str {
    rel_type.rsplit('/').next().unwrap_or("")
}

fn is_kind(r: &PreservedRel, kinds: &[&str]) -> bool {
    let k = kind(&r.rel_type);
    kinds.iter().any(|x| x.eq_ignore_ascii_case(k))
}

fn rewritten(r: &PreservedRel) -> bool {
    !r.external && REWRITTEN.contains(&kind(&r.rel_type).to_ascii_lowercase().as_str())
}

fn rels_part_name(part: &str) -> String {
    let (dir, file) = package::split_dir(part);
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

#[cfg(test)]
mod third_party_tests {
    use super::*;

    #[test]
    fn third_party_types_are_those_outside_ooxml_and_microsoft() {
        assert!(is_third_party("http://customschemas.google.com/relationships/workbookmetadata"));
        assert!(is_third_party("urn:example:addin/relationships/state"));
        assert!(!is_third_party("http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet"));
        assert!(!is_third_party("http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet"));
        assert!(!is_third_party("HTTP://schemas.microsoft.com/office/2099/relationships/notAThing"));
        assert!(!is_third_party(""));
    }

    #[test]
    fn third_party_part_keeps_its_name_only_where_gridcraft_does_not_write() {
        let mut k = write::KeptOut::default();
        assert_eq!(k.claim_third_party("xl/metadata"), "xl/metadata");
        assert_eq!(k.claim_third_party("xl/addin/state.xml"), "xl/addin/state.xml");
        // Names GridCraft writes itself get a fresh name in the same folder.
        for taken in ["xl/metadata.xml", "xl/styles.xml", "xl/worksheets/sheet1.xml", "[Content_Types].xml", "xl/_rels/workbook.xml.rels"] {
            let name = k.claim_third_party(taken);
            assert_ne!(name, taken);
            assert_eq!(package::split_dir(&name).0, package::split_dir(taken).0, "{name}");
        }
        // A second part of the same name doesn't take the first one's.
        assert_ne!(k.claim_third_party("xl/metadata"), "xl/metadata");
    }
}
