//! File content GridCraft does not model, carried through unchanged so that saving a workbook
//! never silently drops it (XLSX pass-through: custom XML, external links, connections, query
//! tables, form controls, OLE objects…).
//!
//! The data is opaque to everything except the file-format crate: the model only holds it, so a
//! sheet's kept content disappears with the sheet. It is not part of the JSON format.

use std::sync::Arc;

use crate::sheet::Visibility;

/// A relationship from an owning part (the package root, the workbook or a sheet, or a kept part)
/// to a kept part or to an external target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedRel {
    /// The id in the source file (`rId7`). Ids of an owner GridCraft writes are reassigned on save.
    pub id: String,
    /// The full relationship type URI.
    pub rel_type: String,
    /// The original part name of the target (no leading `/`), or the raw target of an external
    /// relationship. External targets are never opened.
    pub target: String,
    pub external: bool,
}

/// A kept package part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedPart {
    /// Part name in the source file (no leading `/`).
    pub name: String,
    pub content_type: String,
    pub data: Arc<[u8]>,
    /// The part's own relationships, ids unchanged (the part's bytes refer to them).
    pub rels: Vec<PreservedRel>,
}

/// A raw XML element of an owning part that refers to kept parts, re-emitted on save with its
/// relationship ids rewritten, or that Excel needs back unchanged (the workbook's `fileVersion`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedXml {
    /// Which slot of the owner it belongs in (`externalReferences`, `fileVersion`, `ext`,
    /// `oleObjects`, `controls`).
    pub slot: String,
    /// The element, namespace declarations it needs included.
    pub xml: String,
}

/// A drawing anchor of a sheet's drawing part that GridCraft does not model (a form or ActiveX
/// control shape wrapped in `mc:AlternateContent`, a SmartArt graphic), written back verbatim.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedAnchor {
    /// Its position among the drawing part's anchors in the source file.
    pub index: usize,
    /// The element, namespace declarations it needs included.
    pub xml: String,
}

/// A chart sheet (a sheet that holds one chart and no cells), dialog sheet or Excel 4.0 macro sheet
/// kept as it was in the source file. Its part, drawing, chart, style, colour and VML parts are in
/// [`Preserved::parts`]; the workbook relationship to it is in [`Preserved::rels`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedSheet {
    pub name: String,
    pub visibility: Visibility,
    /// The id of the workbook relationship to the sheet part, in the source file.
    pub rel_id: String,
    /// How many worksheets come before it: its position among the model's sheets.
    pub before: usize,
    /// Its `sheetId` in the source file.
    pub sheet_id: u32,
}

/// A PivotTable read from a file, with the parts it was read from: when its cache can't be rebuilt
/// on save (the source sheet, table or name is gone), these are written back unchanged, as Excel
/// keeps such a PivotTable with its last results (sheet owner only).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedPivot {
    /// The model PivotTable's id and name when it was read (the PivotTable it belongs to).
    pub id: u32,
    pub name: String,
    /// The `pivotTableDefinition` part.
    pub table: PreservedPart,
    /// Its `cacheId` in the source file.
    pub cache_id: u32,
    /// The `pivotCacheDefinition` part; its `rels` hold the relationship to `records`.
    pub cache: PreservedPart,
    /// The `pivotCacheRecords` part, when the file had one.
    pub records: Option<PreservedPart>,
}

/// Unmodelled content kept for one owner (the workbook, which also holds package-root content,
/// or one sheet).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preserved {
    /// Relationships of the package root (`_rels/.rels`) to kept parts (workbook owner only).
    pub package_rels: Vec<PreservedRel>,
    /// Relationships of the owning workbook or sheet part to kept parts.
    pub rels: Vec<PreservedRel>,
    /// Every kept part reachable from `package_rels` and `rels`.
    pub parts: Vec<PreservedPart>,
    /// Owner XML elements that refer to kept parts.
    pub elements: Vec<PreservedXml>,
    /// The sheet's legacy VML drawing when kept elements need its shapes (form controls, OLE
    /// objects). Its own relationships point into `parts`.
    pub vml: Option<PreservedPart>,
    /// Chart, dialog and macro sheets, in file order (workbook owner only).
    pub sheets: Vec<PreservedSheet>,
    /// Unmodelled anchors of the sheet's drawing part, in file order (sheet owner only).
    pub anchors: Vec<PreservedAnchor>,
    /// Relationships of the sheet's drawing part that `anchors` use (ids as in the source file);
    /// their targets are in `parts`.
    pub drawing_rels: Vec<PreservedRel>,
    /// The digital signature of the workbook's VBA project (workbook owner only).
    pub vba_signature: Option<PreservedVbaSignature>,
    /// PivotTables of the sheet with the parts they were read from (sheet owner only).
    pub pivots: Vec<PreservedPivot>,
}

/// The signature parts of a VBA project (`xl/vbaProject.bin` relationships), kept with the project
/// bytes they were read with: a signature is only valid over exactly those bytes, so it is written
/// back only while the workbook's VBA project is unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreservedVbaSignature {
    /// The VBA project bytes the signature was read with.
    pub project: Arc<Vec<u8>>,
    /// The VBA project part's relationships to the signature parts.
    pub rels: Vec<PreservedRel>,
    /// The signature parts, bytes unchanged.
    pub parts: Vec<PreservedPart>,
}

impl Preserved {
    pub fn is_empty(&self) -> bool {
        self.package_rels.is_empty()
            && self.rels.is_empty()
            && self.parts.is_empty()
            && self.elements.is_empty()
            && self.vml.is_none()
            && self.sheets.is_empty()
            && self.anchors.is_empty()
            && self.vba_signature.is_none()
            && self.pivots.is_empty()
    }
    /// Total bytes held.
    pub fn byte_len(&self) -> usize {
        let signature = self.vba_signature.iter().flat_map(|s| &s.parts);
        let parts = self.parts.iter().chain(self.vml.iter()).chain(signature).fold(0usize, |n, p| n.saturating_add(p.data.len()));
        let pivots = self.pivots.iter().flat_map(|p| [Some(&p.table), Some(&p.cache), p.records.as_ref()]).flatten();
        let parts = pivots.fold(parts, |n, p| n.saturating_add(p.data.len()));
        self.anchors.iter().fold(parts, |n, a| n.saturating_add(a.xml.len()))
    }
}
