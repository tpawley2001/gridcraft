//! Writing a workbook as an XLSX package.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use gridcraft_core::RangeRef;
use gridcraft_formula::quote_sheet;
use gridcraft_model::{CalcMode, CalcSettings, PhoneticString, ShowObjects, Style, UpdateLinks, Visibility, Workbook, WorkbookView};
use sha2::Digest as _;

use crate::package::ZipOut;
use crate::xml::{esc, esc_attr};
use crate::{FunctionInfo, IoError, XlsxKind};

pub const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

pub const CT_WORKSHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
pub const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
pub const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
pub const CT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
pub const CT_COMMENTS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
pub const CT_VBA: &str = "application/vnd.ms-office.vbaProject";
pub const CT_THREADED_COMMENTS: &str = "application/vnd.ms-excel.threadedcomments+xml";
pub const CT_PERSONS: &str = "application/vnd.ms-excel.person+xml";
pub const REL_THREADED_COMMENT: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
pub const REL_PERSON: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
/// Namespace of the threaded comments and persons parts ([MS-XLSX]).
pub const NS_THREADED: &str = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";

/// A part's relationships.
#[derive(Default)]
pub struct Rels {
    items: Vec<(String, String, String, bool)>,
}

impl Rels {
    /// Adds a relationship of type `kind` (last segment, e.g. `worksheet`); returns its id.
    pub fn add(&mut self, kind: &str, target: &str) -> String {
        let id = self.fresh_id();
        let ty = match kind {
            "core-properties" => "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".to_string(),
            // The VBA project relationship lives in the Microsoft-documented namespace, not the
            // OpenXML one; Excel rejects the file if this type is wrong.
            "vbaProject" => "http://schemas.microsoft.com/office/2006/relationships/vbaProject".to_string(),
            k => format!("{NS_REL}/{k}"),
        };
        self.items.push((id.clone(), ty, target.to_string(), false));
        id
    }
    pub fn add_external(&mut self, kind: &str, target: &str) -> String {
        let id = self.fresh_id();
        self.items.push((id.clone(), format!("{NS_REL}/{kind}"), target.to_string(), true));
        id
    }
    /// Adds a Microsoft extension relationship with its complete namespace URI.
    pub fn add_uri(&mut self, ty: &str, target: &str) -> String {
        let id = self.fresh_id();
        self.items.push((id.clone(), ty.to_string(), target.to_string(), false));
        id
    }
    /// Adds a relationship with its complete type URI, internal or external (kept content).
    pub fn add_raw(&mut self, ty: &str, target: &str, external: bool) -> String {
        let id = self.fresh_id();
        self.items.push((id.clone(), crate::xml::esc_attr(ty), target.to_string(), external));
        id
    }
    /// Adds a kept relationship under its id from the source file, or a new id when that one is
    /// taken; returns the id used. Kept parts written byte for byte can name a relationship of
    /// another part by id (a SmartArt data part names the diagram drawing relationship of the
    /// sheet's drawing in `dsp:dataModelExt relId`), so renumbering would break them.
    pub fn add_kept(&mut self, id: &str, ty: &str, target: &str, external: bool) -> String {
        let plain = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        let id = if !plain || self.items.iter().any(|(i, ..)| i == id) { self.fresh_id() } else { id.to_string() };
        self.items.push((id.clone(), crate::xml::esc_attr(ty), target.to_string(), external));
        id
    }
    /// The first `rIdN` from the number of relationships on that isn't in use.
    fn fresh_id(&self) -> String {
        let mut n = self.items.len().saturating_add(1);
        loop {
            let id = format!("rId{n}");
            if !self.items.iter().any(|(i, ..)| *i == id) {
                return id;
            }
            n = n.saturating_add(1);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn xml(&self) -> String {
        let mut s = format!("{XML_DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, ty, target, ext) in &self.items {
            let _ = write!(
                s,
                "<Relationship Id=\"{id}\" Type=\"{ty}\" Target=\"{}\"{}/>",
                esc_attr(target),
                if *ext { " TargetMode=\"External\"" } else { "" }
            );
        }
        s.push_str("</Relationships>");
        s
    }
}

/// Shared string table being built.
#[derive(Default)]
pub struct Sst {
    list: Vec<String>,
    map: HashMap<String, u32>,
    pub count: u64,
}

impl Sst {
    pub fn index(&mut self, s: &str) -> u32 {
        self.count += 1;
        if let Some(&i) = self.map.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.to_string());
        self.map.insert(s.to_string(), i);
        i
    }
}

/// Package being assembled.
pub struct Out {
    parts: Vec<(String, Vec<u8>)>,
    overrides: Vec<(String, String)>,
    defaults: BTreeMap<String, String>,
    pub dxfs: Vec<Style>,
    pub sst: Sst,
    pub images: u32,
    /// Media parts written so far, by extension and SHA-256 of their bytes: a picture used again
    /// is found here instead of being compared byte by byte with every earlier one.
    media: HashMap<(String, [u8; 32]), String>,
    pub charts: u32,
    pub theme_overrides: u32,
    /// The workbook's file name, which a PivotChart's link to its PivotTable names in brackets.
    pub book_name: String,
    /// References of chartex parts, defined as hidden `_xlchart.v1.N` names.
    pub chart_names: Vec<String>,
    pub drawings: u32,
    pub tables: u32,
    pub comments: u32,
    /// VML drawings written, the last VML shape-id block used, and control properties parts.
    pub vmls: u32,
    pub vml_block: u32,
    pub ctrl_props: u32,
    /// Authors of threaded comments, in first-use order; written to `xl/persons/person.xml`.
    pub persons: Vec<String>,
    /// The table ids of the workbook, by sheet and table index, decided before any sheet is
    /// written (see [`table_ids`]).
    pub table_ids: Vec<Vec<u32>>,
    pub dynamic_arrays: bool,
    pub needs_calc: bool,
    pub cell_pictures: crate::richdata::PicturesOut,
    /// The workbook's cube-formula (MDX) value metadata, written in `xl/metadata.xml`; cells
    /// refer to it by `vm` (see [`crate::mdx_metadata::metadata`]).
    pub mdx: Option<std::sync::Arc<gridcraft_model::MdxMetadata>>,
    /// Kept (pass-through) parts written so far.
    pub kept: crate::passthrough::KeptOut,
    /// Decides which formulas need dynamic-array metadata.
    pub functions: FunctionInfo,
    /// Defined names analysed for implicit intersection, shared by every formula written.
    pub(crate) names: crate::array_formula::NameCache,
    /// The code of the Python cells written so far, in order: script `i` of the Python part.
    pub python_scripts: Vec<String>,
}

impl Out {
    /// Whether a part of this name (case-insensitive) was already written.
    pub fn has_part(&self, name: &str) -> bool {
        let name = crate::package::normalize(name);
        self.parts.iter().any(|(n, _)| crate::package::normalize(n) == name)
    }
    pub fn part(&mut self, name: &str, content_type: Option<&str>, data: Vec<u8>) {
        if let Some(ct) = content_type {
            self.overrides.push((format!("/{name}"), ct.to_string()));
        }
        // Every `xl/media/image*.ext` part is indexed as it is added, preserved ones included, so a
        // later identical picture finds it instead of writing a second copy.
        if let Some((ext, file)) = name.strip_prefix("xl/media/").filter(|f| f.starts_with("image")).and_then(|f| Some((f.rsplit_once('.')?.1, f))) {
            self.media.entry((ext.to_string(), sha2::Sha256::digest(&data).into())).or_insert_with(|| file.to_string());
        }
        self.parts.push((name.to_string(), data));
    }
    /// The `xl/media` file name (`imageN.ext`) holding `data`. A picture used on several sheets,
    /// or twice on one, is written once and every drawing points to that part, as in Excel.
    pub fn image_part(&mut self, ext: &str, data: &[u8]) -> String {
        let key = (ext.to_string(), sha2::Sha256::digest(data).into());
        if let Some(file) = self.media.get(&key) {
            return file.clone();
        }
        self.images += 1;
        let file = format!("image{}.{ext}", self.images);
        // Not through `part`: that would hash the same bytes again to index them.
        self.parts.push((format!("xl/media/{file}"), data.to_vec()));
        self.media.insert(key, file.clone());
        file
    }
    /// The `xl/media` file name (`{stem}N.ext`) of a part a picture extension refers to (an HD
    /// Photo layer), written once per distinct content with its content type.
    pub fn media_part(&mut self, stem: &str, ext: &str, content_type: &str, data: &[u8]) -> String {
        let prefix = format!("xl/media/{stem}");
        let suffix = format!(".{ext}");
        let numbered = |name: &str| {
            name.strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(&suffix))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        if let Some((name, _)) = self.parts.iter().find(|(name, bytes)| numbered(name) && bytes.as_slice() == data) {
            return name.trim_start_matches("xl/media/").to_string();
        }
        let n = self.parts.iter().filter(|(name, _)| numbered(name)).count().saturating_add(1);
        let mut n = u32::try_from(n).unwrap_or(u32::MAX);
        loop {
            let file = format!("{stem}{n}{suffix}");
            let name = format!("xl/media/{file}");
            if !self.parts.iter().any(|(p, _)| p.eq_ignore_ascii_case(&name)) || n == u32::MAX {
                self.part(&name, Some(content_type), data.to_vec());
                return file;
            }
            n = n.saturating_add(1);
        }
    }
    /// Replaces the bytes of a part already added.
    pub fn replace(&mut self, name: &str, data: Vec<u8>) {
        if let Some(p) = self.parts.iter_mut().find(|(n, _)| n == name) {
            p.1 = data;
        }
    }
    pub fn default_type(&mut self, ext: &str, ct: &str) {
        self.defaults.entry(ext.to_string()).or_insert_with(|| ct.to_string());
    }
}

/// Id of the i-th person of `xl/persons/person.xml`: a GUID-shaped, deterministic id (the
/// file only needs ids that are unique within it).
pub fn person_guid(i: usize) -> String {
    format!("{{00000000-0000-4000-8002-{:012X}}}", i as u64 & 0xFFFF_FFFF_FFFF)
}

/// `$A$1:$D$20` (or `$1:$3` / `$A:$C`).
pub fn abs_range(r: &RangeRef) -> String {
    let c = |col: u32| gridcraft_core::col_to_letters(col);
    if r.is_full_rows() && !r.is_full_cols() {
        return format!("${}:${}", r.start.row + 1, r.end.row + 1);
    }
    if r.is_full_cols() && !r.is_full_rows() {
        return format!("${}:${}", c(r.start.col), c(r.end.col));
    }
    if r.is_single() {
        return format!("${}${}", c(r.start.col), r.start.row + 1);
    }
    format!("${}${}:${}${}", c(r.start.col), r.start.row + 1, c(r.end.col), r.end.row + 1)
}

/// Refuses sheet names XLSX cannot hold. The reader repairs such names (cuts, strips, renames
/// duplicates) but formulas keep the old spelling, so the saved file would turn into `#REF!`.
fn check_sheet_names(wb: &Workbook) -> Result<(), IoError> {
    let mut seen = std::collections::HashSet::new();
    for sheet in &wb.sheets {
        let name = &sheet.name;
        let bad = if name.is_empty() {
            Some("is blank")
        } else if name.chars().count() > 31 {
            Some("is longer than 31 characters")
        } else if name.chars().any(|c| matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']')) {
            Some("contains one of : \\ / ? * [ ]")
        } else if !seen.insert(name.to_ascii_lowercase()) {
            Some("is used by another sheet")
        } else {
            None
        };
        if let Some(why) = bad {
            return Err(IoError::Format(format!("sheet name \"{name}\" {why}; rename the sheet before saving as XLSX")));
        }
    }
    Ok(())
}

/// The `id` of every table of `wb`, by sheet and table index, in write order.
///
/// A table keeps its model id, unless another table already took it; a table without
/// one (made in the app) or with a duplicate gets the next id above every id in use. The ids of
/// single mapped XML cells (`<singleXmlCell id>`, kept verbatim in `tableSingleCells` parts) share
/// the table id space (ECMA-376 Part 1 §18.5.1.2, §18.12.2), so they count as used too: a table
/// that took one would make Excel repair the file.
fn table_ids(wb: &Workbook) -> Result<Vec<Vec<u32>>, IoError> {
    let mut used: std::collections::HashSet<u32> = single_cell_ids(wb);
    let mut next = used.iter().copied().max().unwrap_or(0);
    for sheet in &wb.sheets {
        for t in &sheet.tables {
            next = next.max(t.id);
        }
    }
    let mut ids = Vec::with_capacity(wb.sheets.len());
    for sheet in &wb.sheets {
        let mut sheet_ids = Vec::with_capacity(sheet.tables.len());
        for t in &sheet.tables {
            let id = if t.id != 0 && used.insert(t.id) {
                t.id
            } else {
                next = next.checked_add(1).ok_or_else(|| IoError::Format("no table id available above the existing ids".into()))?;
                used.insert(next);
                next
            };
            sheet_ids.push(id);
        }
        ids.push(sheet_ids);
    }
    Ok(ids)
}

/// The `id`s of single mapped XML cells in all kept `tableSingleCells` parts.
fn single_cell_ids(wb: &Workbook) -> std::collections::HashSet<u32> {
    let mut ids = std::collections::HashSet::new();
    let owners = wb.preserved.iter().map(|p| p.as_ref()).chain(wb.sheets.iter().filter_map(|s| s.preserved.as_deref()));
    let parts = owners.flat_map(|p| {
        // Parts can be reached through another kept part, rather than directly from a sheet.
        let targets: std::collections::HashSet<String> = p
            .rels
            .iter()
            .chain(p.parts.iter().flat_map(|part| &part.rels))
            .filter(|r| !r.external && r.rel_type.rsplit('/').next().is_some_and(|k| k.eq_ignore_ascii_case("tableSingleCells")))
            .map(|r| crate::package::normalize(&r.target))
            .collect();
        p.parts.iter().filter(move |part| {
            part.content_type.eq_ignore_ascii_case("application/vnd.openxmlformats-officedocument.spreadsheetml.tableSingleCells+xml")
                || targets.contains(&crate::package::normalize(&part.name))
        })
    });
    for part in parts {
        let mut add = |e: crate::xml::El| {
            if let Some(id) = e.attr_u32("id") {
                ids.insert(id);
            }
            Ok(())
        };
        // A part that can't be parsed keeps no ids: it is written back as it is either way.
        let _ = crate::xml::parse_streaming(&part.data, &["singleXmlCell"], &mut add);
    }
    ids
}

/// Workbook name written in a PivotChart's link when the file name isn't known.
const DEFAULT_BOOK_NAME: &str = "Workbook.xlsx";

pub fn write_xlsx(wb: &Workbook, kind: XlsxKind, functions: FunctionInfo) -> Result<Vec<u8>, IoError> {
    write_xlsx_named_with(wb, kind, None, functions)
}

/// The `<workbookView>` attributes beyond `activeTab`: the window's display settings
/// that differ from the format's defaults. An ordinary workbook gets none, so its file
/// is unchanged; a workbook Excel saved with the sheet tabs or scroll bars hidden
/// keeps them that way (#3057).
fn workbook_view_attrs(wb: &Workbook) -> String {
    let mut attrs = String::new();
    if !wb.view.show_sheet_tabs {
        attrs.push_str(" showSheetTabs=\"0\"");
    }
    if !wb.view.show_horizontal_scroll {
        attrs.push_str(" showHorizontalScroll=\"0\"");
    }
    if !wb.view.show_vertical_scroll {
        attrs.push_str(" showVerticalScroll=\"0\"");
    }
    let tab_ratio = wb.view.tab_ratio.min(WorkbookView::MAX_TAB_RATIO);
    if tab_ratio != 600 {
        let _ = write!(attrs, " tabRatio=\"{tab_ratio}\"");
    }
    if let Some(r) = wb.view.window {
        let _ = write!(attrs, " xWindow=\"{}\" yWindow=\"{}\" windowWidth=\"{}\" windowHeight=\"{}\"", r.x, r.y, r.width, r.height);
    }
    attrs
}

/// The `<workbookPr>` element (ECMA-376 Part 1 §18.2.28).
fn workbook_pr(wb: &Workbook) -> String {
    let mut w = String::from("<workbookPr");
    match wb.show_objects {
        ShowObjects::All => {}
        ShowObjects::Placeholders => w.push_str(" showObjects=\"placeholders\""),
        ShowObjects::None => w.push_str(" showObjects=\"none\""),
    }
    if wb.date_system == gridcraft_core::DateSystem::D1904 {
        w.push_str(" date1904=\"1\"");
    }
    if wb.remove_personal_info {
        w.push_str(" filterPrivacy=\"1\"");
    }
    if let Some(name) = &wb.code_name {
        let _ = write!(w, " codeName=\"{}\"", crate::xml::esc_attr(name));
    }
    let options = &wb.options;
    for (name, value) in [
        ("autoCompressPictures", options.auto_compress_pictures),
        ("discardImageEditData", options.discard_image_edit_data),
        ("showInkAnnotation", options.show_ink_annotation),
        ("hidePivotFieldList", options.hide_pivot_field_list),
        ("showPivotChartFilter", options.show_pivot_chart_filter),
        ("showBorderUnselectedTables", options.show_border_unselected_tables),
        ("checkCompatibility", options.check_compatibility),
        ("dateCompatibility", options.date_compatibility),
    ] {
        if let Some(value) = value {
            let _ = write!(w, " {name}=\"{}\"", u8::from(value));
        }
    }
    if let Some(policy) = options.update_links {
        let value = match policy {
            UpdateLinks::UserSet => "userSet",
            UpdateLinks::Never => "never",
            UpdateLinks::Always => "always",
        };
        let _ = write!(w, " updateLinks=\"{value}\"");
    }
    for (name, value) in [("defaultImageDpi", options.default_image_dpi), ("defaultThemeVersion", options.default_theme_version)] {
        if let Some(value) = value {
            let _ = write!(w, " {name}=\"{value}\"");
        }
    }
    w.push_str("/>");
    w
}

/// Writes the workbook for the file `book_name` (a bare file name). A PivotChart is bound to its
/// PivotTable in Excel only when its link starts with the workbook in brackets.
pub fn write_xlsx_named_with(wb: &Workbook, kind: XlsxKind, book_name: Option<&str>, functions: FunctionInfo) -> Result<Vec<u8>, IoError> {
    if wb.sheets.is_empty() {
        return Err(IoError::Format("a workbook needs at least one sheet".into()));
    }
    if wb.sheets.iter().any(|sheet| sheet.cell_pictures.keys().any(|c| sheet.cell(*c).is_some_and(|cell| cell.formula.is_some()))) {
        return Err(IoError::Format("a static cell picture cannot also contain a formula".into()));
    }
    check_sheet_names(wb)?;
    let mut out = Out {
        parts: vec![],
        overrides: vec![],
        defaults: BTreeMap::new(),
        dxfs: vec![],
        sst: Sst::default(),
        images: 0,
        media: HashMap::new(),
        charts: 0,
        theme_overrides: 0,
        book_name: book_name.filter(|n| !n.is_empty() && !n.contains(['[', ']'])).unwrap_or(DEFAULT_BOOK_NAME).to_string(),
        chart_names: vec![],
        drawings: 0,
        tables: 0,
        comments: 0,
        vmls: 0,
        vml_block: 0,
        ctrl_props: 0,
        persons: vec![],
        table_ids: table_ids(wb)?,
        dynamic_arrays: false,
        needs_calc: false,
        cell_pictures: Default::default(),
        mdx: wb.mdx_metadata.clone().filter(|m| !m.records.is_empty()),
        kept: Default::default(),
        functions,
        names: Default::default(),
        python_scripts: vec![],
    };
    out.kept.reserve_vml_blocks(wb);
    out.default_type("rels", "application/vnd.openxmlformats-package.relationships+xml");
    out.default_type("xml", "application/xml");

    // Which sheet is active (must be visible), and at least one visible sheet.
    let active = if wb.sheets.get(wb.active_sheet()).is_some_and(|s| s.visibility == Visibility::Visible) {
        wb.active_sheet()
    } else {
        wb.sheets.iter().position(|s| s.visibility == Visibility::Visible).unwrap_or(0)
    };

    let mut wb_rels = Rels::default();
    let (sheets_xml, kept, pivots) = write_sheets(wb, &mut out, active, &mut wb_rels, kind);

    let names = write_defined_names(wb, &kept, &out.chart_names, kind);

    let mut w = format!("{XML_DECL}<workbook xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\">");
    // A workbook read from a file keeps its <fileVersion> as it was: Excel trusts the saved values
    // only when appName is "xl" (#799). One without names GridCraft (#1699), and Excel recalculates it.
    let file_version = crate::passthrough::workbook_elements(wb, "fileVersion");
    w.push_str(if file_version.is_empty() { "<fileVersion appName=\"gridcraft\"/>" } else { &file_version });
    w.push_str(&workbook_pr(wb));
    w.push_str(&workbook_protection(wb));
    let _ = write!(w, "<bookViews><workbookView activeTab=\"{}\"", kept.file_index(active));
    w.push_str(&workbook_view_attrs(wb));
    let _ = write!(w, "/></bookViews><sheets>{sheets_xml}</sheets>");
    w.push_str(&kept.elements);
    if !names.is_empty() {
        let _ = write!(w, "<definedNames>{names}</definedNames>");
    }
    w.push_str("<calcPr calcId=\"191029\"");
    match wb.calc.mode {
        CalcMode::Automatic => {}
        CalcMode::Manual => w.push_str(" calcMode=\"manual\""),
        CalcMode::AutomaticExceptTables => w.push_str(" calcMode=\"autoNoTable\""),
    }
    // CT_CalcPr defaults: iterate off, 100 iterations, 0.001 maximum change. The limits are kept
    // while iteration is off, as Options keeps them.
    let calc = &wb.calc;
    if calc.iterative {
        w.push_str(" iterate=\"1\"");
    }
    if calc.iterative || calc.max_iterations != CalcSettings::DEFAULT_ITERATIONS {
        let _ = write!(w, " iterateCount=\"{}\"", calc.max_iterations);
    }
    if calc.iterative || calc.max_change != CalcSettings::DEFAULT_CHANGE {
        let _ = write!(w, " iterateDelta=\"{}\"", crate::xml::num(calc.max_change));
    }
    if wb.calc.precision_as_displayed {
        w.push_str(" fullPrecision=\"0\"");
    }
    if wb.calc.r1c1 {
        w.push_str(" refMode=\"R1C1\"");
    }
    if out.needs_calc {
        w.push_str(" fullCalcOnLoad=\"1\"");
    }
    w.push_str("/>");
    // CT_Workbook: customWorkbookViews after calcPr (and oleSize), before pivotCaches.
    w.push_str(&kept.custom_views);
    w.push_str(&pivots.workbook_xml(&mut wb_rels));
    // CT_Workbook: after pivotCaches/smartTagTypes, before fileRecoveryPr/webPublishObjects/extLst.
    crate::web_publishing::write(wb, &mut w);
    let exts = crate::passthrough::workbook_elements(wb, "ext");
    if !exts.is_empty() {
        let _ = write!(w, "<extLst>{exts}</extLst>");
    }
    w.push_str("</workbook>");

    // Workbook-level parts.
    let mut extra_fmts = pivots.number_formats(wb);
    for code in out.kept.num_fmt_codes() {
        if !extra_fmts.contains(&code) {
            extra_fmts.push(code);
        }
    }
    let (styles, fmt_ids) = crate::styles::write_styles(wb, &out.dxfs, &extra_fmts);
    let fmt_id = |code: &str| crate::tables::builtin_id(code).or_else(|| fmt_ids.get(code).copied());
    pivots.finish(wb, &mut out, &fmt_id);
    crate::passthrough::finish_style_ids(&mut out, &fmt_id);
    out.part("xl/styles.xml", Some("application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"), styles.into_bytes());
    wb_rels.add("styles", "styles.xml");
    if !out.python_scripts.is_empty() {
        use crate::python_part as py;
        let part = py::write(&out.python_scripts);
        out.part(py::PART, Some(py::CONTENT_TYPE), part.into_bytes());
        wb_rels.add_uri(py::REL, "python.xml");
    }
    if wb.styles.iter().any(|(_, st)| st.num_fmt.as_str() == "checkbox") {
        use crate::feature_bags as fpb;
        out.part(fpb::PART, Some(fpb::CONTENT_TYPE), fpb::checkbox_part().into_bytes());
        wb_rels.add_uri(fpb::REL, "featurePropertyBag/featurePropertyBag.xml");
    }
    out.part(
        "xl/theme/theme1.xml",
        Some("application/vnd.openxmlformats-officedocument.theme+xml"),
        crate::theme::write_theme(&wb.theme).into_bytes(),
    );
    crate::passthrough::write_theme_relationships(&wb.theme, &mut out);
    wb_rels.add("theme", "theme/theme1.xml");
    // Phonetic (furigana) data is kept per string text: the table below holds each text once, so
    // the runs of the entry with that text are written next to it (`Workbook::phonetics`).
    let phonetics: HashMap<&str, &PhoneticString> = wb.phonetics.iter().map(|p| (p.text.as_str(), p)).collect();
    let mut sst = format!("{XML_DECL}<sst xmlns=\"{NS_MAIN}\" count=\"{}\" uniqueCount=\"{}\">", out.sst.count, out.sst.list.len());
    for t in &out.sst.list {
        let _ = write!(sst, "<si>{}</si>", crate::xml::si_body(t, phonetics.get(t.as_str()).copied()));
    }
    sst.push_str("</sst>");
    out.part("xl/sharedStrings.xml", Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"), sst.into_bytes());
    wb_rels.add("sharedStrings", "sharedStrings.xml");
    if !out.cell_pictures.is_empty() {
        crate::richdata::write(&mut out, &mut wb_rels)?;
    } else if let Some(mdx) = out.mdx.clone() {
        crate::mdx_metadata::write(&mut out, &mdx, &mut wb_rels);
    } else if out.dynamic_arrays {
        out.part(
            "xl/metadata.xml",
            Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"),
            METADATA.as_bytes().to_vec(),
        );
        wb_rels.add("sheetMetadata", "metadata.xml");
    }
    if !out.persons.is_empty() {
        let mut x = format!("{XML_DECL}<personList xmlns=\"{NS_THREADED}\" xmlns:x=\"{NS_MAIN}\">");
        for (i, name) in out.persons.iter().enumerate() {
            let n = esc_attr(name);
            let _ = write!(x, "<person displayName=\"{n}\" id=\"{}\" userId=\"{n}\" providerId=\"None\"/>", person_guid(i));
        }
        x.push_str("</personList>");
        out.part("xl/persons/person.xml", Some(CT_PERSONS), x.into_bytes());
        wb_rels.add_uri(REL_PERSON, "persons/person.xml");
    }
    out.part("xl/workbook.xml", Some(kind.main_content_type()), w.into_bytes());
    // A macro-free package can't hold a VBA project (Excel refuses such a file).
    if let Some(data) = wb.vba_project.as_ref().filter(|_| kind.is_macro_enabled()) {
        wb_rels.add("vbaProject", "vbaProject.bin");
        out.part("xl/vbaProject.bin", Some(CT_VBA), data.to_vec());
        crate::vba_signature::write(wb, data, &mut out);
    }
    out.part("xl/_rels/workbook.xml.rels", None, wb_rels.xml().into_bytes());

    // Document properties.
    let p = &wb.props;
    // "Remove personal information on save": the author and company are not saved (#3089).
    let private = |v: &String| if wb.remove_personal_info { String::new() } else { v.clone() };
    let (author, company) = (private(&p.author), private(&p.company));
    let mut core = format!(
        "{XML_DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">"
    );
    for (tag, v) in [
        ("dc:title", &p.title),
        ("dc:subject", &p.subject),
        ("dc:creator", &author),
        ("cp:keywords", &p.keywords),
        ("dc:description", &p.description),
        ("cp:lastModifiedBy", &p.last_modified_by),
        ("cp:revision", &p.revision),
        ("cp:category", &p.category),
        ("dc:language", &p.language),
    ] {
        if !v.is_empty() {
            let _ = write!(core, "<{tag}>{}</{tag}>", esc(v));
        }
    }
    for (tag, v) in [("dcterms:created", &p.created), ("dcterms:modified", &p.modified)] {
        if crate::read::is_w3cdtf(v) {
            let _ = write!(core, "<{tag} xsi:type=\"dcterms:W3CDTF\">{}</{tag}>", esc(v));
        }
    }
    core.push_str("</cp:coreProperties>");
    out.part("docProps/core.xml", Some("application/vnd.openxmlformats-package.core-properties+xml"), core.into_bytes());
    let mut app = String::from(XML_DECL);
    app.push_str("<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><Application>");
    app.push_str(gridcraft_core::brand::APP_NAME);
    app.push_str("</Application>");
    for (tag, value) in [("Manager", &p.manager), ("Company", &company), ("HyperlinkBase", &p.hyperlink_base)] {
        if !value.is_empty() {
            let _ = write!(app, "<{tag}>{}</{tag}>", esc(value));
        }
    }
    app.push_str("</Properties>");
    out.part("docProps/app.xml", Some("application/vnd.openxmlformats-officedocument.extended-properties+xml"), app.into_bytes());
    let mut root = Rels::default();
    root.add("officeDocument", "xl/workbook.xml");
    root.add("core-properties", "docProps/core.xml");
    root.add("extended-properties", "docProps/app.xml");
    crate::passthrough::write_package_rels(&mut root, &mut out);
    out.part("_rels/.rels", None, root.xml().into_bytes());

    // Content types first, then the parts in creation order.
    let mut ct = format!("{XML_DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    for (ext, t) in &out.defaults {
        // Kept parts bring their content types (and extensions) from the input file: escape them.
        let _ = write!(ct, "<Default Extension=\"{}\" ContentType=\"{}\"/>", esc_attr(ext), esc_attr(t));
    }
    for (part, t) in &out.overrides {
        let _ = write!(ct, "<Override PartName=\"{}\" ContentType=\"{}\"/>", esc_attr(part), esc_attr(t));
    }
    ct.push_str("</Types>");
    let mut zip = ZipOut::new();
    zip.add("[Content_Types].xml", ct.as_bytes())?;
    // Excel's order: rels, workbook rels, workbook, the rest, document properties last. libmagic (PHP
    // `finfo`, `file`) recognises an OOXML package only when an `xl/` entry name appears within the
    // first few KB, so no user-sized part (document properties, a long description) may precede it.
    let rank = |n: &str| match n {
        "_rels/.rels" => 0,
        "xl/_rels/workbook.xml.rels" => 1,
        "xl/workbook.xml" => 2,
        n if n.starts_with("docProps/") => 4,
        _ => 3,
    };
    let mut parts = std::mem::take(&mut out.parts);
    parts.sort_by_key(|(n, _)| rank(n));
    for (name, data) in &parts {
        zip.add(name, data)?;
    }
    zip.finish()
}

fn write_sheets(
    wb: &Workbook,
    out: &mut Out,
    active: usize,
    wb_rels: &mut Rels,
    kind: XlsxKind,
) -> (String, crate::passthrough::WorkbookKept, crate::pivot::PivotWriter) {
    // Kept content first: the kept sheets among the worksheets need their relationship ids.
    let mut kept = crate::passthrough::write_workbook(wb, wb_rels, out, kind.is_macro_enabled());
    let mut pivots = crate::pivot::PivotWriter::new(kept.pivot_cache_ids(), kept.pivot_cache_entries());
    let mut sheets_xml = String::new();
    let any_visible = wb.sheets.iter().any(|s| s.visibility == Visibility::Visible);
    let mut ids = crate::passthrough::SheetIds::new(wb, &kept);
    for (i, sheet) in wb.sheets.iter().enumerate() {
        sheets_xml.push_str(&kept.sheet_entries(i, false, &mut ids));
        let (xml, mut rels) = crate::sheet_write::write_sheet(wb, i, i == active, out);
        pivots.add_sheet(wb, i, out, &mut rels);
        let name = format!("xl/worksheets/sheet{}.xml", i + 1);
        out.part(&name, Some(CT_WORKSHEET), xml.into_bytes());
        if !rels.is_empty() {
            out.part(&format!("xl/worksheets/_rels/sheet{}.xml.rels", i + 1), None, rels.xml().into_bytes());
        }
        let rid = wb_rels.add("worksheet", &format!("worksheets/sheet{}.xml", i + 1));
        let state = match sheet.visibility {
            Visibility::Visible => "",
            _ if !any_visible && i == active => "",
            Visibility::Hidden => " state=\"hidden\"",
            Visibility::VeryHidden => " state=\"veryHidden\"",
        };
        let name: String = sheet.name.chars().take(31).collect();
        let sheet_id = ids.take(sheet.file_id);
        let _ = write!(sheets_xml, "<sheet name=\"{}\" sheetId=\"{sheet_id}\"{state} r:id=\"{rid}\"/>", esc_attr(&name));
    }
    sheets_xml.push_str(&kept.sheet_entries(wb.sheets.len(), true, &mut ids));
    kept.custom_views = crate::passthrough::custom_workbook_views(wb, &ids);

    (sheets_xml, kept, pivots)
}

fn write_defined_names(wb: &Workbook, kept: &crate::passthrough::WorkbookKept, chart_names: &[String], kind: XlsxKind) -> String {
    // Defined names, then print areas and titles.
    let mut names = String::new();
    let mut seen = std::collections::HashSet::new();
    let omitted = if kind.is_macro_enabled() { Default::default() } else { crate::xlm::omitted_names(wb) };
    for (i, n) in wb.names.iter().enumerate() {
        if n.name.is_empty() || !seen.insert((n.name.to_ascii_lowercase(), n.scope)) {
            continue;
        }
        if let Some(sc) = n.scope
            && sc >= wb.sheets.len()
        {
            continue;
        }
        if omitted.contains(&i) {
            continue;
        }
        let _ = write!(names, "<definedName name=\"{}\"", esc_attr(&n.name));
        if let Some(sc) = n.scope {
            let _ = write!(names, " localSheetId=\"{}\"", kept.file_index(sc));
        }
        if n.hidden {
            names.push_str(" hidden=\"1\"");
        }
        if !n.comment.is_empty() {
            let _ = write!(names, " comment=\"{}\"", esc_attr(&n.comment));
        }
        let _ = write!(names, ">{}</definedName>", esc(&crate::fmla::text_to_file(&n.formula)));
    }
    for (i, f) in chart_names.iter().enumerate() {
        let _ = write!(
            names,
            "<definedName name=\"{}{i}\" hidden=\"1\">{}</definedName>",
            crate::chartex::NAME_PREFIX,
            esc(&crate::fmla::text_to_file(f))
        );
    }
    for (i, sheet) in wb.sheets.iter().enumerate() {
        let q = quote_sheet(&sheet.name);
        let areas = sheet.print.print_areas();
        if !areas.is_empty() {
            let list: Vec<String> = areas.iter().map(|pa| format!("{q}!{}", abs_range(pa))).collect();
            let _ = write!(
                names,
                "<definedName name=\"_xlnm.Print_Area\" localSheetId=\"{}\">{}</definedName>",
                kept.file_index(i),
                esc(&list.join(","))
            );
        }
        let mut titles = vec![];
        if let Some((c0, c1)) = sheet.print.title_cols {
            titles.push(format!("{q}!{}", abs_range(&RangeRef::cols(c0.min(c1), c0.max(c1)))));
        }
        if let Some((r0, r1)) = sheet.print.title_rows {
            titles.push(format!("{q}!{}", abs_range(&RangeRef::rows(r0.min(r1), r0.max(r1)))));
        }
        if !titles.is_empty() {
            let _ = write!(
                names,
                "<definedName name=\"_xlnm.Print_Titles\" localSheetId=\"{}\">{}</definedName>",
                kept.file_index(i),
                esc(&titles.join(","))
            );
        }
    }

    names
}

/// Cell metadata declaring dynamic-array formulas (MS-XLSX `XLDAPR`).
const METADATA: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"><metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/></metadataTypes><futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata><cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata></metadata>";
/// Writes a workbook with a caller-supplied file name and the default function facts.
pub fn write_xlsx_named(wb: &Workbook, kind: XlsxKind, book_name: Option<&str>) -> Result<Vec<u8>, IoError> {
    write_xlsx_named_with(wb, kind, book_name, FunctionInfo::default())
}

/// `<workbookProtection>` (ECMA-376 Part 1 §18.2.29): the structure lock from
/// [`Workbook::protected_structure`], plus the window and revision locks and passwords kept from
/// the file. Empty when there is nothing to write.
fn workbook_protection(wb: &Workbook) -> String {
    let locks = &wb.locks;
    if !wb.protected_structure && locks.is_default() {
        return String::new();
    }
    let workbook_password = if wb.protected_structure { wb.structure_password.as_ref() } else { locks.workbook_password.as_ref() };
    let mut w = String::from("<workbookProtection");
    if let Some(h) = workbook_password {
        w.push_str(&crate::sheet_write::password_attrs(h, "workbook"));
    }
    if let Some(h) = &locks.revisions_password {
        w.push_str(&crate::sheet_write::password_attrs(h, "revisions"));
    }
    let lock_structure = if wb.protected_structure { Some(true) } else { locks.lock_structure.filter(|on| !on) };
    for (name, on) in [("lockStructure", lock_structure), ("lockWindows", locks.lock_windows), ("lockRevision", locks.lock_revision)] {
        if let Some(on) = on {
            let _ = write!(w, " {name}=\"{}\"", u8::from(on));
        }
    }
    w.push_str("/>");
    w
}

#[cfg(test)]
mod table_id_tests {
    use super::*;
    use gridcraft_model::Table;

    fn table(id: u32, range: &str) -> Table {
        Table {
            id,
            name: String::new(),
            range: RangeRef::parse(range).unwrap(),
            header_row: false,
            totals_row: false,
            columns: vec![],
            style: String::new(),
            banded_rows: false,
            banded_cols: false,
            first_col: false,
            last_col: false,
            filter_button: false,
            filter: vec![],
            table_type: None,
            connection_id: None,
            header_row_dxf: None,
            data_dxf: None,
            totals_row_dxf: None,
            header_row_cell_style: None,
            data_cell_style: None,
            totals_row_cell_style: None,
            header_row_border_dxf: None,
            table_border_dxf: None,
            totals_row_border_dxf: None,
            sort: None,
            comment: None,
            insert_row: None,
            insert_row_shift: None,
            published: None,
        }
    }

    #[test]
    fn model_table_ids_are_preserved_and_duplicates_allocated_above_the_maximum() {
        for (source, expected) in [(vec![7, 2], vec![7, 2]), (vec![5, 5], vec![5, 6]), (vec![0, 0], vec![1, 2]), (vec![0, 9], vec![10, 9])] {
            let mut wb = Workbook::new();
            wb.sheet_mut(0).unwrap().tables = source.into_iter().map(|id| table(id, "A1:A2")).collect();
            assert_eq!(table_ids(&wb).unwrap(), vec![expected]);
        }
    }

    #[test]
    fn exhausted_table_ids_return_an_error() {
        let mut wb = Workbook::new();
        wb.sheet_mut(0).unwrap().tables = vec![table(u32::MAX, "A1:A2"), table(0, "C1:C2")];
        assert!(matches!(write_xlsx(&wb, XlsxKind::Workbook, FunctionInfo::default()), Err(IoError::Format(_))));
    }

    #[test]
    fn skipped_tables_do_not_shift_saved_ids() {
        let mut wb = Workbook::new();
        let mut skipped = table(3, "A1");
        skipped.totals_row = true;
        wb.sheet_mut(0).unwrap().tables = vec![skipped, table(7, "C1:C2")];
        let bytes = write_xlsx(&wb, XlsxKind::Workbook, FunctionInfo::default()).unwrap();
        let mut saved = crate::package::Package::open(&bytes).unwrap();
        let xml = saved.read_xml("xl/tables/table1.xml").unwrap().unwrap();
        assert_eq!(xml.attr_u32("id"), Some(7));
        assert!(saved.read("xl/tables/table2.xml").unwrap().is_none());
    }
}
