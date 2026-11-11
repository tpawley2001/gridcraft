//! XLSB import of cell values, formulas and print settings, implemented from Microsoft's public
//! MS-XLSB specification:
//! <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/acc8aa92-1f02-4167-99f5-84f9f676b95a>.
//! Cell, shared and array formulas are decoded with the shared BIFF token decoder; those it can't
//! represent keep their saved result as a constant. Array formulas whose anchor carries cell
//! metadata of the dynamic-array type (`BrtCellMeta` and the metadata part) import as
//! dynamic-array formulas whose other cells are left for the spill to fill. Cell formatting (number formats, fonts, fills,
//! borders, alignment and protection), column widths, row heights, hidden and outlined
//! rows/columns, frozen panes, sheet view options, workbook theme colours and fonts, tab colours
//! (including theme colours and tints), hyperlinks, notes, tables (with their calculated column
//! and custom totals formulas, which like cell formulas may use structured references), AutoFilters, PivotTables,
//! Allow Edit Ranges (titles, references and legacy or iterated passwords),
//! sparklines, drawings (pictures, shapes, text boxes and charts, read with the .xlsx drawing reader) and
//! each sheet's page setup, print options, header/footer and manual page breaks are imported; data
//! connections and query tables are kept for saving as .xlsx ([`connections`]).
//! Chart sheets open as chart sheets: the chart of the drawing their BrtDrawing record names, with
//! their margins, page setup and header/footer, saved to .xlsx as chart sheets. Dialog
//! and macro sheets are skipped; gradient fills, cell style inheritance, differential formats, macros and other workbook features
//! are not run or imported.

mod sparklines;
mod styles;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_model::{
    CalcMode, Cell, Color, Comment, EditRange, Formula, Hyperlink, LineInfo, Orientation, PasswordHash, PrintSettings, Sheet, SheetProtection,
    SheetViewMode, StyleId, Visibility, Workbook,
};
use quick_xml::NsReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::package::{Package, Rel, normalize, resolve, split_dir};
use crate::tables::{col_width_to_px, pt_to_px};
use crate::xls::formula::{Links, Mode, Site, SupBook, Xti, decode_xlsb, decode_xlsb_at, external_sheet_text};
use crate::xls::macro_sheet::{MacroSheet, UNREAD_FORMULAS, add_macro_sheets, kept_warning};
use crate::xls::{
    BUILTIN_NAMES, ChartSheet, FormulaGroups, Lbl, MAX_SHARED_ATTACHES, Parsed, Pending, add_chart_sheets, add_defined_names, blank_array_followers,
    resolve_pending,
};
use crate::{IoError, ReadReport};
use gridcraft_formula::Expr;

mod connections;
mod external_links;
mod pivot;
mod tables;

const MAIN: &str = "application/vnd.ms-excel.main";
const BINARY_MAIN: &str = "application/vnd.ms-excel.sheet.binary.macroEnabled.main";
const WORKSHEET: &str = "application/vnd.ms-excel.worksheet";
const CHARTSHEET: &str = "application/vnd.ms-excel.chartsheet";
const MACRO_SHEET: &str = "application/vnd.ms-excel.macrosheet";
const INTL_MACRO_SHEET: &str = "application/vnd.ms-excel.intlmacrosheet";
const STYLES: &str = "application/vnd.ms-excel.styles";
const MAX_STYLE_RECORDS: usize = 65_536;
const STRINGS: &str = "application/vnd.ms-excel.sharedStrings";
const COMMENTS: &str = "application/vnd.ms-excel.comments";
const CT_NS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/content-types";
const REL_NS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";
const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_REL_TYPE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const MS_REL_TYPE: &str = "http://schemas.microsoft.com/office/2006/relationships/";
const MAX_META: u64 = 1024 * 1024;
const MAX_TEXT: usize = 64 * 1024 * 1024;
const MAX_CELLS: usize = 1_000_000;
const MAX_STRINGS: usize = 1_000_000;
const MAX_SHEETS: usize = 1024;
const MAX_RECORDS: usize = 5_000_000;
/// Most stale cached results emptied inside the spill ranges of one sheet's dynamic arrays.
const MAX_SPILL_CELLS: usize = 1_000_000;
/// Most column-information cells one worksheet expands: overlapping BrtColInfo records repeat
/// work even when the column map stays small.
const MAX_COL_ASSIGNMENTS: u64 = 1_000_000;
/// Defined names, supporting links and XTIs read at most.
const MAX_NAMES: usize = 65_536;
const MAX_LINKS: usize = 65_536;
/// Per-sheet caps, matching the .xlsx reader's hyperlink cell cap.
const MAX_LINK_CELLS: usize = 10_000;
const MAX_COMMENTS: usize = 65_536;
/// BrtLegacyDrawing ([MS-XLSB]): the relationship id of the sheet's VML drawing part.
const LEGACY_DRAWING: u16 = 551;
/// Longest relationship id read from a record.
const MAX_REL_ID: u32 = 255;
/// Relationship types of the threaded comments and persons parts ([MS-XLSX]).
const THREADED_REL: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
const PERSON_REL: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";

mod cell_meta;
mod dxf;
mod rules;

/// Icon set names by KPISets number, shared with the `.xls` AutoFilter12 reader.
pub(crate) use rules::icon_set;

fn invalid(msg: impl Into<String>) -> IoError {
    IoError::Format(format!("XLSB: {}", msg.into()))
}

#[derive(Default)]
struct Budget {
    cells: usize,
    text: usize,
    /// Formula cells kept as their saved result because their formula couldn't be decoded.
    formulas_kept: usize,
    /// Hyperlink cells and comments on the sheet being read; reset per sheet.
    links: usize,
    comments: usize,
}
impl Budget {
    fn text(&mut self, len: usize) -> Result<(), IoError> {
        self.text = self.text.saturating_add(len);
        if self.text > MAX_TEXT {
            return Err(IoError::TooLarge("XLSB decoded text exceeds 64 MiB".into()));
        }
        Ok(())
    }
    fn cell(&mut self) -> Result<(), IoError> {
        self.cells += 1;
        if self.cells > MAX_CELLS {
            return Err(IoError::TooLarge("XLSB exceeds 1 million imported cells".into()));
        }
        Ok(())
    }
}

/// Imports worksheet scalar values, cell formulas (with their saved results) and cell formatting.
/// Dates retain the workbook's original date system. Inspect warnings for unsupported features.
pub fn read_xlsb(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    let mut package = Package::open_repairing(bytes)?;
    let types = content_types(&mut package)?;
    let root_rels = relationships(&mut package, "")?;
    let roots: Vec<_> = root_rels.iter().filter(|r| r.kind == "officeDocument").collect();
    if roots.len() != 1 {
        return Err(invalid("expected one workbook relationship"));
    }
    let root = roots.first().ok_or_else(|| invalid("missing workbook relationship"))?;
    if root.external {
        return Err(invalid("external workbook relationship"));
    }
    check_type(&types, &root.target, MAIN)?;
    let book = required(&mut package, &root.target)?;
    let mut wb = Workbook::new();
    crate::read::read_doc_props(&mut package, &root_rels, &mut wb.props);
    wb.sheets.clear();
    let mut report = ReadReport::default();
    if let Some(n) = package.recovered_parts() {
        report.warn(crate::package::damaged_warning(n));
    }
    report.warn("XLSB import: some workbook features are not imported. Save a new .xlsx file; XLSB saving is not supported.");
    keep_custom_properties(&mut package, &types, &root_rels, &mut wb, &mut report);
    let mut itab_cur: Option<u32> = None;
    let (mut sheets, names, mut links) = workbook(&book, &mut wb, &mut report, &mut itab_cur)?;
    crate::read::repair_sheet_names(sheets.iter_mut().map(|s| &mut s.name), &mut report);
    links.sheets = sheets.iter().map(|s| s.name.clone()).collect();
    let cache_ids = pivot::cache_ids(&book);
    let rels = relationships(&mut package, &root.target)?;
    package = external_links::read(package, &types, &rels, &mut links, &mut wb, &mut report);
    links.lists = tables::scan_lists(&mut package, &types, &rels, &sheets);
    // Cells using a name the import drops would calculate to #NAME?: they keep their saved result.
    links.skipped =
        names.iter().map(|l| !l.rgce.is_empty() && decode_xlsb(&l.rgce, &l.rgcb, CellRef::new(0, 0), Mode::Name, &links).is_err()).collect();
    if let Some(rel) = rels.iter().find(|r| r.kind == "theme") {
        let theme = (|| -> Result<_, IoError> {
            if rel.external {
                return Err(invalid("external theme relationship"));
            }
            let data = package.read_limited(&rel.target, MAX_META)?.ok_or_else(|| invalid("missing theme part"))?;
            let xml = crate::xml::parse(&data)?;
            Ok(crate::theme::read_theme(&xml))
        })();
        match theme {
            Ok(theme) => wb.theme = theme,
            Err(_) => report.warn("The XLSB workbook theme could not be read; theme colours and fonts use the default Office theme."),
        }
    }
    // The VBA project is the same Compound File in .xlsb and .xlsm, so it is carried through
    // unchanged (never run or parsed). Its signature part is not kept: it would not match the
    // macro-enabled XML package the workbook is saved as.
    if let Some(rel) = rels.iter().find(|r| r.kind == "vbaProject" && !r.external) {
        match package.read(&rel.target) {
            Ok(Some(data)) => {
                wb.vba_project = Some(data.into());
                report.warn("macros (VBA project) were kept but not executed");
            }
            Ok(None) => report.warn("macros (VBA project) reference a missing part and were dropped"),
            Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
            Err(e) => report.warn(format!("macros (VBA project) could not be read and were dropped: {e}")),
        }
    }
    let mut budget = Budget::default();
    let (styles, formats) = import_styles(&mut package, &types, &rels, &mut wb, &mut budget, &mut report);
    let dxfs = dxf::import(&mut package, &types, &rels, &mut budget, &mut report);
    let mut caches = pivot::Caches::new(&cache_ids, &rels, formats, wb.date_system);
    let mut kept_connections = connections::read_connections(&mut package, &types, &rels, &links, &mut report);
    crate::passthrough::merge_preserved(&mut wb.preserved, kept_connections.preserved.take());
    let sst_rels: Vec<_> = rels.iter().filter(|r| r.kind == "sharedStrings").collect();
    if sst_rels.len() > 1 {
        return Err(invalid("multiple shared-string relationships"));
    }
    let strings = if let Some(rel) = sst_rels.first() {
        if rel.external {
            return Err(invalid("external shared-string relationship"));
        }
        check_type(&types, &rel.target, STRINGS)?;
        shared_strings(&required(&mut package, &rel.target)?, &mut budget, &mut report)?
    } else {
        vec![]
    };
    let persons = persons(&mut package, &rels, &mut report);
    let cell_meta = cell_meta::read(&mut package, &rels, &mut report);
    let mut model_index: Vec<Option<usize>> = Vec::with_capacity(sheets.len());
    let mut table_names = HashSet::new();
    let mut table_id = 1u32;
    let sheet_ids: Vec<u32> = sheets.iter().map(|s| s.sheet_id).collect();
    let mut chart_sheets = vec![];
    let mut chart_bounds: Vec<usize> = vec![];
    let mut unread_chart_sheets = false;
    let mut macro_sheets: Vec<MacroSheet> = vec![];
    let mut macro_unread = false;
    // Damaged chart sheet page records warn once for the workbook.
    let mut chart_page = PageState::default();
    // Unmodelled worksheet drawing content, read as for .xlsx (created on the first drawing).
    let mut kept_drawing: Option<crate::passthrough::KeptReader> = None;
    for (bound, info) in sheets.into_iter().enumerate() {
        model_index.push(None);
        let Some(id) = info.rel else {
            if info.visibility != Visibility::VeryHidden {
                return Err(invalid("worksheet relationship is null"));
            }
            report.warn("XLSB module sheets were skipped.");
            continue;
        };
        let rel = rels.iter().find(|r| r.id == id).ok_or_else(|| invalid("worksheet relationship is missing"))?;
        if rel.external {
            return Err(invalid("external sheet relationship"));
        }
        if rel.kind == "chartsheet" {
            let target = rel.target.clone();
            let mut chart = None;
            let named =
                wb.check_sheet_name(&info.name, None).is_ok() && !chart_sheets.iter().any(|c: &ChartSheet| c.name.eq_ignore_ascii_case(&info.name));
            let mut print = PrintSettings::default();
            if let Some((id, sheet_rels, page_setup)) =
                chart_sheet_drawing(&mut package, &types, &target, &mut chart_page, &mut report).filter(|_| named)
            {
                print = page_setup;
                // The drawing is DrawingML XML, as in .xlsx: read into a scratch sheet for its chart.
                let mut scratch = Sheet::new(info.name.clone());
                let mut cx = crate::read::Ctx::new(package);
                cx.theme_colors = wb.theme.colors;
                sheet_drawing(&mut cx, &sheet_rels, &id, &mut scratch, &mut report);
                package = cx.pkg;
                chart = scratch.charts.into_iter().next();
            }
            match chart {
                Some(chart) => {
                    chart_sheets.push(ChartSheet { name: info.name, visibility: info.visibility, before: wb.sheets.len(), chart, print });
                    chart_bounds.push(bound);
                }
                None => unread_chart_sheets = true,
            }
            continue;
        }
        if matches!(rel.kind.as_str(), "xlMacrosheet" | "xlIntlMacrosheet") {
            // An Excel 4.0 macro sheet: its cells are kept for a macro-enabled save, never evaluated.
            let kept_before = budget.formulas_kept;
            match macro_sheet(&mut package, &types, &rel.target, &info.name, &strings, &styles, &dxfs, &links, &cell_meta, &mut budget, &mut report) {
                Some(sheet) => {
                    report.warn(kept_warning(&info.name));
                    macro_sheets.push(MacroSheet {
                        name: info.name.clone(),
                        visibility: info.visibility,
                        before: wb.sheets.len(),
                        bound,
                        intl: rel.kind == "xlIntlMacrosheet",
                        sheet,
                    });
                }
                None => report.warn("Excel 4.0 macro sheets that can't be read were skipped."),
            }
            macro_unread |= budget.formulas_kept > kept_before;
            budget.formulas_kept = kept_before;
            continue;
        }
        if rel.kind != "worksheet" {
            if !matches!(rel.kind.as_str(), "chartsheet" | "dialogsheet" | "xlMacrosheet" | "xlIntlMacrosheet") {
                return Err(invalid("unknown sheet relationship type"));
            }
            report.warn("XLSB non-worksheet sheets were skipped.");
            continue;
        }
        // A worksheet part missing from the package (a truncated file) leaves its sheet empty at its
        // position, as the .xlsx reader does, instead of refusing the workbook. Checked before the
        // content type, since a truncated package may also lack the part's Override.
        let Some(bytes) = package.read(&rel.target)? else {
            report.warn(format!("sheet \"{}\" could not be read: missing part {}", info.name, rel.target));
            let mut sheet = Sheet::new(info.name.clone());
            sheet.visibility = info.visibility;
            if let Some(slot) = model_index.last_mut() {
                *slot = Some(wb.sheets.len());
            }
            wb.sheets.push(Arc::new(sheet));
            continue;
        };
        check_type(&types, &rel.target, WORKSHEET)?;
        let sheet_rels = sheet_relationships(&mut package, &rel.target, &mut report);
        budget.links = 0;
        budget.comments = 0;
        let mut parts = SheetParts::default();
        let mut drawing = None;
        let mut sheet = worksheet(
            &bytes,
            &info.name,
            &strings,
            &styles,
            &dxfs,
            &links,
            &cell_meta,
            &sheet_rels,
            &mut budget,
            &mut parts,
            &mut drawing,
            &mut report,
            false,
        )?;
        if let Some(id) = drawing {
            // Drawing parts are DrawingML XML, as in .xlsx: read them with the .xlsx reader's state.
            let mut cx = crate::read::Ctx::new(package);
            cx.theme_colors = wb.theme.colors;
            // Anchors the drawing reader does not model (SmartArt graphics) are kept for saving,
            // unless the drawing could not be read (already reported).
            if sheet_drawing(&mut cx, &sheet_rels, &id, &mut sheet, &mut report) {
                let kept =
                    kept_drawing.get_or_insert_with(|| crate::passthrough::KeptReader::new(&mut cx)).sheet_drawing(&mut cx, &rel.target, &sheet.name);
                std::mem::take(&mut cx.report.warnings).into_iter().for_each(|w| report.warn(w));
                crate::passthrough::merge_preserved(&mut sheet.preserved, kept);
            }
            package = cx.pkg;
        }
        sheet_comments(&mut package, &types, &sheet_rels, &mut sheet, &mut budget, &mut report);
        comment_parts(&mut package, &sheet_rels, parts.legacy_drawing.as_deref(), &persons, &mut sheet, &mut report);
        tables::read_tables(&mut package, &types, &rel.target, &parts.lists, &links, &dxfs, &mut sheet, &mut table_names, &mut table_id, &mut report);
        sheet.pivots = pivot::read_pivots(&mut package, &types, &sheet_rels, &info.name, &mut caches, &mut report);
        let query_tables = connections::read_query_tables(&mut package, &types, &sheet_rels, &mut kept_connections, &mut report);
        crate::passthrough::merge_preserved(&mut sheet.preserved, query_tables);
        if let Some(id) = parts.printer_settings {
            let kept = keep_printer_settings(&mut package, &types, &sheet_rels, &id, &info.name, &mut report);
            crate::passthrough::merge_preserved(&mut sheet.preserved, kept);
        }
        sheet.visibility = info.visibility;
        if let Some(slot) = model_index.last_mut() {
            *slot = Some(wb.sheets.len());
        }
        wb.sheets.push(Arc::new(sheet));
    }
    if wb.sheets.is_empty() {
        return Err(invalid("no supported worksheets"));
    }
    if unread_chart_sheets {
        report.warn("XLSB chart sheets that could not be read were skipped.");
    }
    add_chart_sheets(&mut wb, chart_sheets);
    if macro_unread {
        report.warn(UNREAD_FORMULAS);
    }
    add_macro_sheets(&mut wb, macro_sheets, &chart_bounds);
    let mut views = Vec::new();
    let mut records = Records::new(&book);
    while let Some((id, payload)) = records.next(&mut report)? {
        if id == 397 {
            if views.len() < crate::binary_custom_views::LIMIT
                && let Some(view) = crate::binary_custom_views::book(payload, true)
            {
                views.push(view);
            } else {
                crate::binary_custom_views::warn(&mut report);
            }
        }
    }
    if !views.is_empty() {
        crate::binary_custom_views::pin_sheet_ids(&mut wb);
    }
    crate::binary_custom_views::finish_book(&views, &mut wb.preserved, |id| {
        sheet_ids.iter().position(|&s| s == id).and_then(|i| model_index.get(i).copied().flatten()).unwrap_or(0)
    });
    add_defined_names(&names, &model_index, &mut wb, &mut report, |l| {
        decode_xlsb(&l.rgce, &l.rgcb, CellRef::new(0, 0), Mode::Name, &links).map(|e| gridcraft_formula::print(&e))
    });
    if budget.formulas_kept > 0 {
        report.warn(format!(
            "{} XLSB formula(s) use features that can't be imported yet (including links to other workbooks); they were kept as their saved results and will not recalculate.",
            budget.formulas_kept
        ));
    }
    let active = itab_cur
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| model_index.get(index).copied().flatten())
        .filter(|&index| wb.sheets.get(index).is_some_and(|s| s.visibility == Visibility::Visible))
        .or_else(|| wb.sheets.iter().position(|s| s.visibility == Visibility::Visible))
        .unwrap_or(0);
    wb.set_active_sheet(active);
    for part in package.checksum_failures() {
        report.warn(crate::package::checksum_warning(part));
    }
    Ok((wb, report))
}

/// Keeps the custom document properties part for saving as .xlsx: an .xlsb package holds the same
/// `docProps/custom.xml` as an .xlsx one, kept byte for byte with its package-root relationship.
fn keep_custom_properties(package: &mut Package<'_>, types: &[Entry], root_rels: &[Rel], wb: &mut Workbook, report: &mut ReadReport) {
    use crate::passthrough::{CT_CUSTOM_PROPERTIES, MAX_KEPT_PART, REL_CUSTOM_PROPERTIES, keep_package_root_part};
    let Some(rel) = root_rels.iter().find(|r| r.kind == "custom-properties") else { return };
    let data = (|| {
        if rel.external {
            return None;
        }
        check_type(types, &rel.target, CT_CUSTOM_PROPERTIES).ok()?;
        package.read_limited(&rel.target, MAX_KEPT_PART).ok().flatten()
    })();
    match data {
        Some(data) => {
            let kept = gridcraft_model::PreservedRel {
                id: rel.id.clone(),
                rel_type: REL_CUSTOM_PROPERTIES.into(),
                target: rel.target.clone(),
                external: false,
            };
            keep_package_root_part(wb, kept, CT_CUSTOM_PROPERTIES, data);
        }
        None => report.warn("The XLSB custom document properties could not be read and are not kept on save."),
    }
}

/// The content type of a printer settings part (ECMA-376 Part 1 §22.3).
const CT_PRINTER_SETTINGS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings";

/// Keeps the printer settings part a worksheet's `BrtPageSetup` names (`szRelID`), the way the
/// `.xlsx` reader keeps the part behind `<pageSetup r:id>`: the sheet's relationship and the part
/// byte for byte, with the `pageSetup` slot naming the relationship. `None`, with one warning
/// naming the sheet, when the relationship is missing, external or points at a missing or damaged
/// part; the other print settings are kept either way.
fn keep_printer_settings(
    package: &mut Package<'_>,
    types: &[Entry],
    sheet_rels: &[Rel],
    id: &str,
    name: &str,
    report: &mut ReadReport,
) -> Option<Arc<gridcraft_model::Preserved>> {
    use gridcraft_model::{Preserved, PreservedPart, PreservedRel, PreservedXml};
    let dropped = |report: &mut ReadReport, why: &str| {
        report.warn(format!("sheet \"{name}\": printer settings were dropped: {why}"));
        None
    };
    let Some(rel) = sheet_rels.iter().find(|r| r.id == id && r.kind == "printerSettings") else {
        return dropped(report, "the page setup names a relationship the sheet does not have");
    };
    if rel.external {
        return dropped(report, "the relationship is external");
    }
    let content_type = match types.iter().find(|e| e.name == "Override" && e.attr("PartName").is_some_and(|p| normalize(p) == normalize(&rel.target)))
    {
        Some(e) => e.attr("ContentType").unwrap_or(CT_PRINTER_SETTINGS).to_string(),
        None => CT_PRINTER_SETTINGS.to_string(),
    };
    let data = match package.read_optional_limited(&rel.target, crate::passthrough::MAX_KEPT_PART) {
        Ok(Some(data)) => data,
        Ok(None) => return dropped(report, &format!("{} is missing", rel.target)),
        Err(e) => return dropped(report, &e.to_string()),
    };
    let mut p = Preserved::default();
    p.parts.push(PreservedPart { name: rel.target.clone(), content_type, data: Arc::from(data), rels: vec![] });
    p.rels.push(PreservedRel { id: rel.id.clone(), rel_type: format!("{REL_TYPE}printerSettings"), target: rel.target.clone(), external: false });
    p.elements.push(PreservedXml { slot: "pageSetup".into(), xml: rel.id.clone() });
    Some(Arc::new(p))
}

pub(crate) fn is_xlsb(bytes: &[u8]) -> bool {
    let inspect = || -> Result<bool, IoError> {
        let mut package = Package::open_repairing(bytes)?;
        let types = content_types(&mut package)?;
        let roots = relationships(&mut package, "")?;
        let mut books = roots.iter().filter(|r| r.kind == "officeDocument");
        let Some(root) = books.next() else { return Ok(false) };
        Ok(books.next().is_none() && !root.external && check_type(&types, &root.target, MAIN).is_ok())
    };
    inspect().unwrap_or(false)
}

fn required(package: &mut Package<'_>, path: &str) -> Result<Vec<u8>, IoError> {
    package.read(path)?.ok_or_else(|| invalid(format!("missing part {path}")))
}

struct Entry {
    name: String,
    attrs: Vec<(String, String)>,
}
impl Entry {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// The two package metadata parts are flat, namespace-qualified XML documents. Keep only
/// direct child entries, and reject truncated XML instead of guessing a partial relationship.
fn metadata(bytes: &[u8], ns: &[u8], root_name: &[u8]) -> Result<Vec<Entry>, IoError> {
    let mut reader = NsReader::from_reader(bytes);
    let mut depth = 0usize;
    let mut seen = false;
    let mut entries = vec![];
    loop {
        let event = reader.read_event().map_err(|e| IoError::Xml(e.to_string()))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let (namespace, local) = reader.resolver_mut().resolve_element(e.name());
                let valid_ns = matches!(namespace, ResolveResult::Bound(n) if n.as_ref() == ns);
                if depth == 0 {
                    if seen || !valid_ns || local.as_ref() != root_name {
                        return Err(invalid("invalid package metadata root"));
                    }
                    seen = true;
                } else if depth == 1 && valid_ns {
                    let mut attrs = vec![];
                    for a in e.attributes() {
                        let a = a.map_err(|e| IoError::Xml(e.to_string()))?;
                        if !a.key.as_ref().contains(&b':') {
                            attrs.push((
                                String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                                a.normalized_value(XmlVersion::Explicit1_0).map_err(|e| IoError::Xml(e.to_string()))?.into_owned(),
                            ));
                        }
                    }
                    entries.push(Entry { name: String::from_utf8_lossy(local.as_ref()).into_owned(), attrs });
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                    if depth > crate::xml::MAX_DEPTH {
                        return Err(invalid("package metadata is nested too deeply"));
                    }
                }
            }
            Event::End(_) => depth = depth.checked_sub(1).ok_or_else(|| invalid("unexpected metadata end"))?,
            Event::Text(t) if depth == 0 && !t.xml_content(XmlVersion::Explicit1_0).map_err(|e| IoError::Xml(e.to_string()))?.trim().is_empty() => {
                return Err(invalid("text outside package metadata"));
            }
            Event::DocType(_) => return Err(invalid("DOCTYPE in package metadata is unsupported")),
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen || depth != 0 {
        return Err(invalid("incomplete package metadata"));
    }
    Ok(entries)
}

fn content_types(package: &mut Package<'_>) -> Result<Vec<Entry>, IoError> {
    let data = package.read_limited("[Content_Types].xml", MAX_META)?.ok_or_else(|| invalid("missing content types"))?;
    metadata(&data, CT_NS, b"Types")
}

fn check_type(types: &[Entry], path: &str, expected: &str) -> Result<(), IoError> {
    let found = types.iter().find(|e| e.name == "Override" && e.attr("PartName").is_some_and(|p| normalize(p) == normalize(path))).or_else(|| {
        types.iter().find(|e| {
            e.name == "Default" && e.attr("Extension").is_some_and(|ext| path.rsplit('.').next().is_some_and(|p| p.eq_ignore_ascii_case(ext)))
        })
    });
    let actual = found.and_then(|e| e.attr("ContentType"));
    if actual != Some(expected) && !(expected == MAIN && actual == Some(BINARY_MAIN)) {
        return Err(invalid(format!("wrong or missing content type for {path}")));
    }
    Ok(())
}

fn rels_path(path: &str) -> String {
    let (dir, file) = split_dir(path);
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

fn relationships(package: &mut Package<'_>, path: &str) -> Result<Vec<Rel>, IoError> {
    let data = package.read_limited(&rels_path(path), MAX_META)?.ok_or_else(|| invalid(format!("missing relationships for {path}")))?;
    parse_relationships(&data, path)
}

/// Reads the DrawingML part named by a worksheet's BrtDrawing relationship `id` into `sheet`
/// (pictures, shapes, text boxes and charts), with the .xlsx drawing reader. A drawing that can't
/// be found or read is skipped with a warning; the sheet's cells are kept either way. True when the
/// drawing part was read.
fn sheet_drawing(cx: &mut crate::read::Ctx<'_>, sheet_rels: &[Rel], id: &str, sheet: &mut Sheet, report: &mut ReadReport) -> bool {
    let Some(target) = sheet_rels.iter().find(|r| r.id == id && r.kind == "drawing" && !r.external).map(|r| r.target.clone()) else {
        report.warn(format!("The drawing of XLSB sheet '{}' has no drawing relationship and was skipped.", sheet.name));
        return false;
    };
    let kept = (sheet.images.len(), sheet.shapes.len(), sheet.charts.len());
    let result = crate::drawing::read_drawing(cx, &target, sheet);
    let warnings = std::mem::take(&mut cx.report.warnings);
    match result {
        Ok(()) => {
            // The drawing reader skips a part it can't parse with this warning (`Ctx::optional_xml`).
            let skipped = format!("skipped {target}: ");
            let read = !warnings.iter().any(|w| w.starts_with(&skipped));
            warnings.into_iter().for_each(|w| report.warn(w));
            read
        }
        Err(e) => {
            sheet.images.truncate(kept.0);
            sheet.shapes.truncate(kept.1);
            sheet.charts.truncate(kept.2);
            report.warn(format!("The drawing {target} of XLSB sheet '{}' could not be read ({e}) and was skipped.", sheet.name));
            false
        }
    }
}

/// BrtCsPageSetup ([MS-XLSB] 2.4.343): a chart sheet's page setup.
const CS_PAGE_SETUP: u16 = 652;
/// BrtBeginUserCsView / BrtEndUserCsView ([MS-XLSB] 2.4.291, 2.4.641): a custom view's own page
/// setup sits between them.
const BEGIN_USER_CS_VIEW: u16 = 655;
const END_USER_CS_VIEW: u16 = 656;

/// The drawing and page setup of an XLSB chart sheet part ([MS-XLSB] 2.1.7.7): the RelID of its
/// first BrtDrawing record, the part's relationships, and the BrtMargins, BrtCsPageSetup and
/// header/footer records outside custom views. `None` when the part is missing, mistyped, too
/// large, malformed before that record, or has no drawing. A damaged page record keeps the
/// defaults for its part, with one warning for each kind of record in `page`.
fn chart_sheet_drawing(
    package: &mut Package<'_>,
    types: &[Entry],
    path: &str,
    page: &mut PageState,
    report: &mut ReadReport,
) -> Option<(String, Vec<Rel>, PrintSettings)> {
    check_type(types, path, CHARTSHEET).ok()?;
    let data = package.read_limited(path, MAX_META).ok()??;
    let mut records = Records::new(&data);
    let mut print = PrintSettings::default();
    let mut drawing = None;
    let mut in_view = false;
    // Page records come before BrtDrawing; a part damaged after it still has its chart.
    while let Ok(Some((id, payload))) = records.next(report) {
        match id {
            BEGIN_USER_CS_VIEW => in_view = true,
            END_USER_CS_VIEW => in_view = false,
            _ if in_view => {}
            476 => {
                if let Some(m) = page.read(report, "BrtMargins", || read_margins(payload)) {
                    print.margins = m;
                }
            }
            CS_PAGE_SETUP => {
                if let Some((paper, orientation)) = page.read(report, "BrtCsPageSetup", || read_cs_page_setup(payload)) {
                    if let Some(paper) = paper {
                        print.paper = paper.to_string();
                    }
                    print.orientation = orientation;
                }
            }
            479 => {
                if let Some(hf) = page.read(report, "BrtBeginHeaderFooter", || read_header_footer(payload)) {
                    hf.store(&mut print);
                }
            }
            550 if drawing.is_none() => drawing = Some(drawing_rel(Bytes::new(payload)).ok()??),
            _ => {}
        }
    }
    let id = drawing?;
    let rels = package.read_limited(&rels_path(path), MAX_META).ok()??;
    Some((id, parse_relationships(&rels, path).ok()?, print))
}

/// `BrtCsPageSetup` ([MS-XLSB] 2.4.343), reduced to the paper size and the orientation (a chart
/// sheet has no scale or fit-to-pages fields).
fn read_cs_page_setup(payload: &[u8]) -> Result<(Option<&'static str>, Orientation), IoError> {
    let mut b = Bytes::new(payload);
    let paper = b.u32()?;
    b.u32()?; // iRes
    b.u32()?; // iVRes
    b.u32()?; // iCopies
    b.u16()?; // iPageStart
    let flags = b.u16()?;
    b.string(260, true)?; // szRelID: the printer settings part.
    b.finish()?;
    // fNoOrient (bit 3) leaves the orientation to the printer; fLandscape is bit 0.
    let landscape = flags & 0x0008 == 0 && flags & 0x0001 != 0;
    Ok((crate::tables::paper_name(paper), if landscape { Orientation::Landscape } else { Orientation::Portrait }))
}

/// The relationship id of a BrtDrawing record ([MS-XLSB] 2.4.385): a RelID, a nullable wide
/// string. `None` for a null or empty id.
fn drawing_rel(mut b: Bytes<'_>) -> Result<Option<String>, IoError> {
    let id = b.string(255, true)?;
    b.finish()?;
    Ok(id.filter(|id| !id.is_empty()))
}

/// A worksheet's relationships (hyperlink targets, comments): none when the part has no
/// relationships, and none with one warning when they can't be read.
fn sheet_relationships(package: &mut Package<'_>, path: &str, report: &mut ReadReport) -> Vec<Rel> {
    let rels = match package.read_limited(&rels_path(path), MAX_META) {
        Ok(None) => return vec![],
        Ok(Some(data)) => parse_relationships(&data, path),
        Err(e) => Err(e),
    };
    rels.unwrap_or_else(|_| {
        report.warn(
            "Unreadable XLSB worksheet relationships were skipped; that sheet's hyperlinks to files or web pages and its comments were not imported.",
        );
        vec![]
    })
}

fn parse_relationships(data: &[u8], path: &str) -> Result<Vec<Rel>, IoError> {
    let (dir, _) = split_dir(path);
    let mut out: Vec<Rel> = vec![];
    for e in metadata(data, REL_NS, b"Relationships")? {
        if e.name != "Relationship" {
            continue;
        }
        let id = e.attr("Id").filter(|s| !s.is_empty()).ok_or_else(|| invalid("relationship has no ID"))?;
        if out.iter().any(|r| r.id == id) {
            return Err(invalid("duplicate relationship ID"));
        }
        let ty = e.attr("Type").ok_or_else(|| invalid("relationship has no type"))?;
        let kind = ty
            .strip_prefix(REL_TYPE)
            .or_else(|| ty.strip_prefix(STRICT_REL_TYPE))
            .or_else(|| ty.strip_prefix(MS_REL_TYPE))
            .or_else(|| (ty == "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties").then_some("core-properties"))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("unsupported:{ty}"));
        let target = e.attr("Target").ok_or_else(|| invalid("relationship has no target"))?;
        let external = e.attr("TargetMode") == Some("External");
        out.push(Rel { id: id.into(), kind, target: if external { target.into() } else { resolve(dir, target) }, external });
    }
    Ok(out)
}

struct Bytes<'a> {
    rest: &'a [u8],
}
impl<'a> Bytes<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], IoError> {
        let out = self.rest.get(..n).ok_or_else(|| invalid("truncated record or string"))?;
        self.rest = self.rest.get(n..).ok_or_else(|| invalid("truncated record"))?;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, IoError> {
        self.take(1)?.first().copied().ok_or_else(|| invalid("truncated byte"))
    }
    fn u16(&mut self) -> Result<u16, IoError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(|_| invalid("truncated u16"))?))
    }
    fn u32(&mut self) -> Result<u32, IoError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid("truncated u32"))?))
    }
    fn f64(&mut self) -> Result<f64, IoError> {
        let n = f64::from_le_bytes(self.take(8)?.try_into().map_err(|_| invalid("truncated number"))?);
        if !n.is_finite() {
            return Err(invalid("nonfinite numeric value"));
        }
        Ok(n)
    }
    fn string(&mut self, max: u32, nullable: bool) -> Result<Option<String>, IoError> {
        let count = self.u32()?;
        if nullable && count == u32::MAX {
            return Ok(None);
        }
        if count > max {
            return Err(IoError::TooLarge("XLSB string length exceeds its limit".into()));
        }
        let size = (count as usize).checked_mul(2).ok_or_else(|| invalid("string length overflow"))?;
        let bytes = self.take(size)?;
        let mut text = String::new();
        for ch in char::decode_utf16(bytes.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes)) {
            text.push(ch.map_err(|_| invalid("invalid UTF-16 string"))?);
        }
        Ok(Some(text))
    }
    fn text(&mut self, max: u32) -> Result<String, IoError> {
        self.string(max, false)?.ok_or_else(|| invalid("null string"))
    }
    fn finish(self) -> Result<(), IoError> {
        if !self.rest.is_empty() {
            return Err(invalid("unexpected bytes after record payload"));
        }
        Ok(())
    }
}

struct Records<'a> {
    bytes: Bytes<'a>,
    count: usize,
    blocks: Vec<u16>,
    /// Future-record and alternate-content markers are returned like other records, for parts
    /// whose content lives inside them.
    raw: bool,
    /// Whether a record inside an extension block has been dropped (and reported) yet.
    warned_skipped_blocks: bool,
}
impl<'a> Records<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes: Bytes::new(bytes), count: 0, blocks: vec![], raw: false, warned_skipped_blocks: false }
    }
    fn raw(bytes: &'a [u8]) -> Self {
        Self { raw: true, ..Self::new(bytes) }
    }
    fn next(&mut self, report: &mut ReadReport) -> Result<Option<(u16, &'a [u8])>, IoError> {
        while !self.bytes.rest.is_empty() {
            self.count += 1;
            if self.count > MAX_RECORDS {
                return Err(IoError::TooLarge("XLSB part contains too many records".into()));
            }
            let first = self.bytes.u8()?;
            let id = if first & 0x80 == 0 {
                first as u16
            } else {
                let id = u16::from(first & 0x7f) | (u16::from(self.bytes.u8()? & 0x7f) << 7);
                if id < 128 {
                    return Err(invalid("invalid two-byte record type"));
                }
                id
            };
            let mut size = 0usize;
            for shift in [0, 7, 14, 21] {
                let b = self.bytes.u8()?;
                size |= usize::from(b & 0x7f) << shift;
                // The high bit of the fourth length byte is ignored by MS-XLSB.
                if b & 0x80 == 0 || shift == 21 {
                    break;
                }
            }
            let payload = self.bytes.take(size)?;
            match id {
                35 | 37 if !self.raw => {
                    let mut b = Bytes::new(payload);
                    if id == 35 {
                        b.take(4)?;
                    } else {
                        let count = b.u16()?;
                        if count == 0 {
                            return Err(invalid("empty alternate-content version list"));
                        }
                        b.take(usize::from(count) * 4)?;
                    }
                    b.finish()?;
                    if self.blocks.len() >= 64 {
                        return Err(invalid("extension blocks nested too deeply"));
                    }
                    self.blocks.push(id + 1);
                }
                36 | 38 if !self.raw => {
                    if self.blocks.pop() != Some(id) || !payload.is_empty() {
                        return Err(invalid("mismatched extension block end"));
                    }
                }
                _ if !self.blocks.is_empty() && !sparklines::is_record(id) => {
                    if !self.warned_skipped_blocks {
                        self.warned_skipped_blocks = true;
                        report.warn("Unsupported XLSB future-record and alternate-content blocks were skipped.");
                    }
                }
                _ => return Ok(Some((id, payload))),
            }
        }
        if !self.blocks.is_empty() {
            return Err(invalid("unterminated extension block"));
        }
        Ok(None)
    }
    fn first(&mut self, id: u16, report: &mut ReadReport) -> Result<(), IoError> {
        if self.next(report)? != Some((id, &[])) {
            return Err(invalid("missing part begin record"));
        }
        Ok(())
    }
    fn end(&mut self, payload: &[u8], report: &mut ReadReport) -> Result<(), IoError> {
        if !payload.is_empty() || self.next(report)?.is_some() {
            return Err(invalid("records after part end"));
        }
        Ok(())
    }
}

struct SheetInfo {
    sheet_id: u32,
    name: String,
    rel: Option<String>,
    visibility: Visibility,
}
/// Reads the workbook part: the sheet list, the date system, the supporting links and XTIs
/// formulas refer to other sheets through ([MS-XLSB] 2.1.7.61 EXTERNALS), the defined names and
/// the selected sheet tab.
fn workbook(
    bytes: &[u8],
    wb: &mut Workbook,
    report: &mut ReadReport,
    itab_cur: &mut Option<u32>,
) -> Result<(Vec<SheetInfo>, Vec<Lbl>, Links), IoError> {
    let mut records = Records::new(bytes);
    records.first(131, report)?;
    let mut sheets = vec![];
    let mut names = vec![];
    let mut links = Links::default();
    let mut bundle = false;
    let mut bundle_seen = false;
    let mut view_seen = false;
    while let Some((id, payload)) = records.next(report)? {
        let mut b = Bytes::new(payload);
        match id {
            132 => {
                if bundle || !bundle_seen {
                    return Err(invalid("incomplete worksheet list"));
                }
                records.end(payload, report)?;
                return Ok((sheets, names, links));
            }
            143 => {
                if bundle_seen {
                    return Err(invalid("duplicate worksheet list"));
                }
                bundle_seen = true;
                bundle = true;
                b.finish()?;
            }
            144 => {
                if !bundle {
                    return Err(invalid("unexpected worksheet list end"));
                }
                bundle = false;
                b.finish()?;
            }
            153 => {
                wb.date_system = if b.u32()? & 1 != 0 { DateSystem::D1904 } else { DateSystem::D1900 };
                b.u32()?;
                // BrtWbProp ([MS-XLSB] 2.4.820) ends with strName, the workbook's code name.
                wb.code_name = Some(b.text(31)?).filter(|n| !n.is_empty());
                b.finish()?;
            }
            158 if !view_seen => {
                view_seen = true;
                // [MS-XLSB] BrtBookView: the first view's active tab is at byte 24.
                if payload.len() == 29 {
                    *itab_cur = b.take(24).and_then(|_| b.u32()).ok();
                }
                // [MS-XLSB] BrtBookView: iTabRatio at byte 16, display flags at 28.
                if let Some(bytes) = payload.get(16..20).and_then(|bytes| <[u8; 4]>::try_from(bytes).ok()) {
                    wb.view.tab_ratio = u32::from_le_bytes(bytes).min(1000);
                }
                if let Some(flags) = payload.get(28) {
                    wb.view.show_horizontal_scroll = flags & 0x08 != 0;
                    wb.view.show_vertical_scroll = flags & 0x10 != 0;
                    wb.view.show_sheet_tabs = flags & 0x20 != 0;
                }
            }
            157 => calc_prop(payload, wb, report),
            534 | 677 => book_protection(id, payload, wb, report),
            156 => {
                if !bundle {
                    return Err(invalid("worksheet outside the worksheet list"));
                }
                if sheets.len() >= MAX_SHEETS {
                    return Err(IoError::TooLarge("XLSB exceeds 1024 sheets".into()));
                }
                let visibility = match b.u32()? {
                    0 => Visibility::Visible,
                    1 => Visibility::Hidden,
                    2 => Visibility::VeryHidden,
                    _ => return Err(invalid("invalid sheet visibility")),
                };
                let sheet_id = b.u32()?;
                if !(1..=65535).contains(&sheet_id) {
                    return Err(invalid("invalid worksheet ID"));
                }
                let rel = b.string(255, true)?;
                // Keep the usual text resource cap; sheet-name validity is repaired after parsing.
                let name = b.text(32767)?;
                b.finish()?;
                links.sheets.push(name.clone());
                sheets.push(SheetInfo { sheet_id, name, rel, visibility });
            }
            // BrtSupBookSrc (another workbook), BrtSupSelf / BrtSupSame (this one), BrtSupAddin.
            355 | 357 | 358 | 667 if links.books.len() < MAX_LINKS => {
                // BrtSupBookSrc ([MS-XLSB] 2.4.811): strRelID names the external link part.
                let rel = if id == 355 { b.string(MAX_REL_ID, true).ok().flatten() } else { None };
                links.books.push(SupBook { internal: matches!(id, 357 | 358), addin: id == 667, rel, ..SupBook::default() });
            }
            362 => {
                // BrtExternSheet: cXti, then XTIs of iSupBook u32, itabFirst i32, itabLast i32.
                let n = usize::try_from(b.u32()?).unwrap_or(usize::MAX);
                for _ in 0..n.min(b.rest.len() / 12) {
                    if links.xti.len() >= MAX_LINKS {
                        break;
                    }
                    let book = b.u32()?;
                    let first = b.u32()? as i32;
                    let last = b.u32()? as i32;
                    links.xti.push(Xti { book, first, last });
                }
            }
            39 if names.len() < MAX_NAMES => {
                // A malformed name keeps its slot so later names keep their PtgName index.
                let lbl = name_record(&mut b).unwrap_or_else(|_| {
                    report.warn("Some XLSB defined names could not be read and were skipped.");
                    Lbl {
                        name: String::new(),
                        builtin: None,
                        hidden: false,
                        macro_name: false,
                        future_function: false,
                        scope: None,
                        skip: false,
                        rgce: vec![],
                        rgcb: vec![],
                    }
                });
                links.names.push(lbl.name.clone());
                names.push(lbl);
            }
            _ => {}
        }
    }
    Err(invalid("missing workbook end record"))
}

/// BrtName ([MS-XLSB] 2.4.647): flags, chKey, itab, name, then a NameParsedFormula (cce, rgce,
/// cb, rgcb). The comment and macro strings that follow are not needed.
fn name_record(b: &mut Bytes<'_>) -> Result<Lbl, IoError> {
    let flags = b.u32()?;
    let _key = b.u8()?;
    let itab = b.u32()?;
    let raw = b.text(255)?;
    let cce = b.u32()?;
    if cce > 16_384 {
        return Err(invalid("defined name formula too long"));
    }
    let rgce = b.take(cce as usize)?.to_vec();
    let cb = b.u32()?;
    let rgcb = b.take(usize::try_from(cb).map_err(|_| invalid("defined name data too long"))?)?.to_vec();
    let (name, builtin) = if flags & 0x0020 != 0 {
        // Built-in names are spelled out, with or without the `_xlnm.` prefix.
        let bare = raw.strip_prefix("_xlnm.").unwrap_or(&raw);
        match BUILTIN_NAMES.iter().position(|n| n.eq_ignore_ascii_case(bare)) {
            Some(i) => (BUILTIN_NAMES.get(i).map_or_else(|| bare.to_string(), |n| (*n).to_string()), u8::try_from(i).ok()),
            None => (raw.clone(), Some(u8::MAX)),
        }
    } else {
        (raw, None)
    };
    Ok(Lbl {
        name,
        builtin,
        hidden: flags & 0x0001 != 0,
        macro_name: flags & 0x000A != 0,
        future_function: flags & 0x0002_0000 != 0,
        scope: if itab == u32::MAX { None } else { usize::try_from(itab).ok() },
        skip: false,
        rgce,
        rgcb,
    })
}

fn rich_string(b: &mut Bytes<'_>) -> Result<String, IoError> {
    let flags = b.u8()?;
    let text = b.text(32767)?;
    // A RichStr is always the last field of its record (BrtSSTItem,
    // BrtCellRichSt) and neither its formatting runs nor its phonetic data
    // are imported, so once the main text is read a short or oddly sized
    // tail drops the rest of the record instead of refusing the file.
    if flags & 1 != 0 {
        let count = b.u32()?;
        if count > 32767 {
            return Err(invalid("too many rich-text runs"));
        }
        if b.take(count as usize * 4).is_err() {
            b.rest = &[];
            return Ok(text);
        }
    }
    if flags & 2 != 0 {
        // [MS-XLSB] 2.5.103 PhRun: ichFirst, ichMom, cchMom, ifnt (u16 each)
        // then phType/alcH/unused1 (16 bits) = 10 bytes per run. Writers
        // disagree with the documented size, so read it best-effort.
        let _ = phonetic_tail(b);
        b.rest = &[];
    }
    Ok(text)
}

fn phonetic_tail(b: &mut Bytes<'_>) -> Result<(), IoError> {
    b.text(32767)?;
    let count = b.u32()?;
    if count > 32767 {
        return Err(invalid("too many phonetic runs"));
    }
    b.take(count as usize * 10)?;
    Ok(())
}

fn shared_strings(bytes: &[u8], budget: &mut Budget, report: &mut ReadReport) -> Result<Vec<Arc<str>>, IoError> {
    let mut records = Records::new(bytes);
    let Some((159, payload)) = records.next(report)? else {
        return Err(invalid("missing shared-string begin record"));
    };
    let mut header = Bytes::new(payload);
    let total = header.u32()?;
    let unique = header.u32()?;
    header.finish()?;
    if unique > total || unique as usize > MAX_STRINGS {
        return Err(IoError::TooLarge("invalid or oversized shared-string count".into()));
    }
    let mut strings = vec![];
    while let Some((id, payload)) = records.next(report)? {
        match id {
            160 => {
                records.end(payload, report)?;
                if strings.len() != unique as usize {
                    return Err(invalid("shared-string count does not match records"));
                }
                return Ok(strings);
            }
            19 => {
                if strings.len() >= unique as usize {
                    return Err(invalid("too many shared-string items"));
                }
                let mut b = Bytes::new(payload);
                let text = rich_string(&mut b)?;
                b.finish()?;
                budget.text(text.len())?;
                strings.push(Arc::from(text));
            }
            _ => return Err(invalid("unexpected shared-string record")),
        }
    }
    Err(invalid("missing shared-string end record"))
}

/// Optional formatting must not prevent recovery of worksheet values. Parse completely before
/// interning styles, and discard parser warnings and text charges if the styles part is damaged.
fn import_styles(
    package: &mut Package<'_>,
    types: &[Entry],
    rels: &[Rel],
    wb: &mut Workbook,
    budget: &mut Budget,
    report: &mut ReadReport,
) -> (Vec<StyleId>, HashMap<u16, String>) {
    let mut rels = rels.iter().filter(|r| r.kind == "styles");
    let Some(rel) = rels.next() else { return (vec![], HashMap::new()) };
    let text_before = budget.text;
    let mut parse = || -> Result<_, IoError> {
        if rel.external || rels.next().is_some() {
            return Err(invalid("external or multiple styles relationships"));
        }
        check_type(types, &rel.target, STYLES)?;
        styles::parse(&required(package, &rel.target)?, budget)
    };
    match parse() {
        Ok(parsed) => {
            let formats = parsed.formats.clone();
            (styles::build(parsed, wb, report), formats)
        }
        Err(e) => {
            budget.text = text_before;
            report.warn(format!("XLSB styles could not be imported; default styles were used: {e}"));
            (vec![], HashMap::new())
        }
    }
}

fn error(code: u8) -> Result<CellError, IoError> {
    Ok(match code {
        0x00 => CellError::Null,
        0x07 => CellError::Div0,
        0x0f => CellError::Value,
        0x17 => CellError::Ref,
        0x1d => CellError::Name,
        0x24 => CellError::Num,
        0x2a => CellError::NA,
        0x2b => CellError::GettingData,
        _ => return Err(invalid("unknown cell error code")),
    })
}
fn boolean(b: &mut Bytes<'_>) -> Result<bool, IoError> {
    match b.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid boolean value")),
    }
}
/// The formula flags and `CellParsedFormula` after a formula cell's saved result: returns the
/// tokens (`rgce`) and extra data (`rgcb`).
fn cached_formula<'a>(b: &mut Bytes<'a>) -> Result<FormulaBytes<'a>, IoError> {
    b.take(2)?;
    parsed_formula(b)
}
/// A parsed formula's `cce`, `rgce`, `cb` and `rgcb` (`CellParsedFormula`,
/// `SharedParsedFormula`, `ArrayParsedFormula`): the tokens and their extra data.
type FormulaBytes<'a> = (&'a [u8], &'a [u8]);

fn parsed_formula<'a>(b: &mut Bytes<'a>) -> Result<FormulaBytes<'a>, IoError> {
    let cce = b.u32()?;
    if !(1..=16384).contains(&cce) {
        return Err(invalid("invalid formula token length"));
    }
    let rgce = b.take(cce as usize)?;
    let extra = b.u32()?;
    Ok((rgce, b.take(extra as usize)?))
}
/// The anchor row of a formula that is exactly a `PtgExp` (0x01 then a 4-byte row,
/// [MS-XLSB] 2.5.98.40): the cell is part of the shared or array formula in that row.
fn exp_row(rgce: &[u8]) -> Option<u32> {
    match rgce {
        [0x01, a, b, c, d] => Some(u32::from_le_bytes([*a, *b, *c, *d])),
        _ => None,
    }
}
/// An `RfX` range (rwFirst, rwLast, colFirst, colLast); `None` when it isn't a range of the grid.
fn rfx(b: &mut Bytes<'_>) -> Result<Option<RangeRef>, IoError> {
    let (r0, r1, c0, c1) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
    if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
        return Ok(None);
    }
    Ok(Some(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1))))
}

/// A worksheet's shared (`BrtShrFmla`) and array (`BrtArrFmla`) formulas, and the cells that
/// refer to them through `PtgExp`. The records follow the first cell that uses them, so they
/// are resolved once the sheet has been read, by the same code as `.xls`.
#[derive(Default)]
struct Groups {
    pending: Vec<(CellRef, u32)>,
    shared: HashMap<CellRef, Parsed>,
    arrays: HashMap<CellRef, (RangeRef, Parsed)>,
    /// Shared and array ranges by first row, then first column.
    starts: HashMap<u32, BTreeMap<u32, RangeRef>>,
    /// `PtgExp` cells whose cell metadata marks a dynamic array.
    dynamic: HashSet<CellRef>,
    invalid: bool,
}

impl Groups {
    fn add(&mut self, range: Option<RangeRef>, rgce: &[u8], rgcb: &[u8], array: bool) {
        let Some(range) = range else {
            self.invalid = true;
            return;
        };
        let p = Parsed { rgce: rgce.to_vec(), rgcb: rgcb.to_vec() };
        if array {
            self.arrays.insert(range.start, (range, p));
        } else {
            self.shared.insert(range.start, p);
        }
        self.starts.entry(range.start.row).or_default().insert(range.start.col, range);
    }

    /// The anchor of a cell's `PtgExp`: the first cell of the range that starts in that row
    /// and holds the cell (the cell's own row and column when there is none).
    fn anchor(&self, pos: CellRef, row: u32) -> CellRef {
        self.starts
            .get(&row)
            .and_then(|cols| cols.range(..=pos.col).next_back())
            .map(|(_, r)| *r)
            .filter(|r| r.contains(pos))
            .map_or(CellRef::new(row, pos.col), |r| r.start)
    }

    /// Attaches the formulas the sheet's `PtgExp` cells refer to; the others keep their saved
    /// values and count as kept.
    fn resolve(self, sheet: &mut Sheet, links: &Links, budget: &mut Budget, report: &mut ReadReport, commands: bool) {
        if self.invalid {
            report.warn("Invalid XLSB shared or array formula ranges were skipped; their cells kept their saved results.");
        }
        let pending: Vec<Pending> = self.pending.iter().map(|&(pos, row)| Pending { pos, anchor: self.anchor(pos, row) }).collect();
        let mut target = Target { sheet, links, commands };
        if resolve_pending(&mut target, &self.arrays, &self.shared, &pending) >= MAX_SHARED_ATTACHES {
            report.warn("An XLSB sheet has more shared formulas than can be expanded; the rest kept their saved results.");
        }
        let sheet = target.sheet;
        let array_of = |pos: CellRef| sheet.cell(pos).and_then(|c| c.formula.as_ref()).and_then(|f| f.array);
        budget.formulas_kept += pending
            .iter()
            .filter(|p| {
                let own = sheet.cell(p.pos).is_some_and(|c| c.formula.is_some());
                !own && !array_of(p.anchor).is_some_and(|r| r.contains(p.pos))
            })
            .count();
        blank_array_followers(&mut Target { sheet, links, commands }, &pending);
        spill_dynamic(sheet, &self.dynamic);
    }
}

/// Turns the array formulas whose anchors carry dynamic-array cell metadata into dynamic-array
/// formulas: their range is the spill range, whose other cells import empty (as `in_dynamic`
/// does for .xlsx) so the formula refills them. Legacy arrays are left as they are.
fn spill_dynamic(sheet: &mut Sheet, anchors: &HashSet<CellRef>) {
    let mut budget = MAX_SPILL_CELLS;
    for &pos in anchors {
        let Some(f) = sheet.cells.get_mut(pos).and_then(|c| c.formula.as_mut()) else { continue };
        let f = Arc::make_mut(f);
        f.dynamic = true;
        f.legacy = false;
        let Some(range) = f.array.take() else { continue };
        let stale: Vec<CellRef> = sheet
            .cells
            .iter_range(range)
            .filter(|(p, c)| *p != pos && c.formula.is_none() && !c.value.is_empty())
            .map(|(p, _)| p)
            .take(budget)
            .collect();
        budget = budget.saturating_sub(stale.len());
        for p in stale {
            if let Some(cell) = sheet.cells.get_mut(p) {
                cell.value = Value::Empty;
            }
        }
    }
}

/// The sheet shared and array formulas are attached to.
struct Target<'a> {
    commands: bool,
    sheet: &'a mut Sheet,
    links: &'a Links,
}

impl FormulaGroups for Target<'_> {
    fn sheet(&self) -> &Sheet {
        self.sheet
    }
    fn sheet_mut(&mut self) -> &mut Sheet {
        self.sheet
    }
    fn attach_group(&mut self, pos: CellRef, p: &Parsed, _shared: bool, array: Option<RangeRef>) {
        // Shared formulas use PtgRefN / PtgAreaN, which the decoder resolves against `pos`.
        let Ok(Some(mut f)) = formula(&p.rgce, &p.rgcb, pos, self.links, &self.sheet.name, self.commands) else { return };
        f.array = array.filter(|r| !r.is_single());
        // A shared formula is a legacy formula until its cell metadata marks it dynamic
        // (`spill_dynamic`); an array formula, even of one cell, is evaluated as an array.
        f.legacy = array.is_none();
        if let Some(cell) = self.sheet.cells.get_mut(pos) {
            cell.formula = Some(Arc::new(f));
        } else {
            self.sheet.cells.set(pos, Cell { value: Value::Empty, formula: Some(Arc::new(f)), style: StyleId::DEFAULT, py: None });
        }
    }
    fn refuse(&mut self, _why: &'static str) {
        // Counted per cell once the sheet is resolved.
    }
}
/// A cell's value and, for formula cells, the formula's tokens.
fn value<'a>(id: u16, b: &mut Bytes<'a>, strings: &[Arc<str>], budget: &mut Budget) -> Result<(Value, Option<FormulaBytes<'a>>), IoError> {
    let value = match id {
        1 => Value::Empty,
        2 => {
            let rk = b.u32()?;
            let n = if rk & 2 != 0 { ((rk as i32) >> 2) as f64 } else { f64::from_bits(u64::from(rk & !3) << 32) };
            let n = if rk & 1 != 0 { n / 100.0 } else { n };
            if !n.is_finite() {
                return Err(invalid("nonfinite RK number"));
            }
            Value::Number(n)
        }
        3 | 11 => Value::Error(error(b.u8()?)?),
        4 | 10 => Value::Bool(boolean(b)?),
        5 | 9 => Value::Number(b.f64()?),
        6 | 8 | 62 => {
            let s = if id == 62 { rich_string(b)? } else { b.text(32767)? };
            budget.text(s.len())?;
            Value::Text(Arc::from(s))
        }
        7 => Value::Text(strings.get(b.u32()? as usize).cloned().ok_or_else(|| invalid("shared-string index is out of bounds"))?),
        _ => return Err(invalid("unsupported cell record")),
    };
    let rgce = if (8..=11).contains(&id) { Some(cached_formula(b)?) } else { None };
    Ok((value, rgce))
}

/// The formula of `rgce` at `pos`: `Ok(None)` for a lone constant (`=1`), which calculates
/// nothing and is kept as its saved value; `Err` when it can't be represented.
fn formula(rgce: &[u8], rgcb: &[u8], pos: CellRef, links: &Links, sheet: &str, commands: bool) -> Result<Option<Formula>, ()> {
    let e = decode_xlsb_at(rgce, rgcb, pos, Mode::Cell, links, Site { sheet, table: None, external: true, commands }).map_err(|_| ())?;
    if matches!(e, Expr::Number(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Text(_)) {
        return Ok(None);
    }
    // References into other workbooks keep the `.xlsx` text (`[1]Data!B2`) and their saved
    // value, as the .xlsx reader keeps them, even where that text doesn't parse.
    let (text, external) = external_sheet_text(&gridcraft_formula::print(&e));
    let f = Formula::new(&text);
    if f.expr.is_none() && !external {
        return Err(());
    }
    Ok(Some(f))
}

/// `BrtMargins` ([MS-XLSB] 2.4.709): the six page margins, in inches, in the order the model and
/// `<pageMargins>` keep them.
fn read_margins(payload: &[u8]) -> Result<[f32; 6], IoError> {
    let mut b = Bytes::new(payload);
    let mut margins = [0.0f32; 6];
    for slot in &mut margins {
        *slot = b.f64()?.clamp(0.0, 50.0) as f32;
    }
    b.finish()?;
    Ok(margins)
}

/// `BrtPrintOptions` ([MS-XLSB] 2.4.760).
struct PrintOptions {
    gridlines: bool,
    headings: bool,
    center_h: bool,
    center_v: bool,
}
fn read_print_options(payload: &[u8]) -> Result<PrintOptions, IoError> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    b.finish()?;
    Ok(PrintOptions { center_h: flags & 0x0001 != 0, center_v: flags & 0x0002 != 0, headings: flags & 0x0004 != 0, gridlines: flags & 0x0008 != 0 })
}
impl PrintOptions {
    fn store(self, print: &mut PrintSettings) {
        print.gridlines = self.gridlines;
        print.headings = self.headings;
        print.center_h = self.center_h;
        print.center_v = self.center_v;
    }
}

/// `BrtPageSetup` ([MS-XLSB] 2.4.722), reduced to the fields the model keeps: paper size, scale,
/// orientation and the fit-to-pages dimensions (joined with the fit-to-page flag at the end), plus
/// the relationship id of the sheet's printer settings part.
struct PageSetup {
    paper: Option<&'static str>,
    scale: Option<u16>,
    orientation: Orientation,
    fit: (u16, u16),
    /// `szRelID`: the printer settings part, kept as pass-through content. `None` for a null or
    /// empty id.
    rel_id: Option<String>,
}
fn read_page_setup(payload: &[u8]) -> Result<PageSetup, IoError> {
    let mut b = Bytes::new(payload);
    let paper = b.u32()?;
    let scale = b.u32()?;
    b.u32()?; // iRes
    b.u32()?; // iVRes
    b.u32()?; // iCopies
    b.u32()?; // iPageStart
    let fit_width = b.u32()?;
    let fit_height = b.u32()?;
    let flags = b.u16()?;
    let rel_id = b.string(MAX_REL_ID, true)?.filter(|id| !id.is_empty());
    b.finish()?;
    Ok(PageSetup {
        paper: crate::tables::paper_name(paper),
        // Excel writes 0 for a scale it never stated; keep the default 100% like the `.xlsx` reader.
        scale: (scale != 0).then(|| scale.clamp(10, 400) as u16),
        // fNoOrient (bit 6) leaves the orientation to the printer; fLandscape is bit 1.
        orientation: if flags & 0x0040 == 0 && flags & 0x0002 != 0 { Orientation::Landscape } else { Orientation::Portrait },
        fit: (fit_width.min(32767) as u16, fit_height.min(32767) as u16),
        rel_id,
    })
}
impl PageSetup {
    /// Applies the modelled fields and returns the fit-to-pages dimensions.
    fn store(self, print: &mut PrintSettings) -> (u16, u16) {
        if let Some(paper) = self.paper {
            print.paper = paper.to_string();
        }
        if let Some(scale) = self.scale {
            print.scale = scale;
        }
        print.orientation = self.orientation;
        self.fit
    }
}

/// `BrtBeginHeaderFooter` ([MS-XLSB] 2.4.90): the four page-variant flags and the six
/// `&L&C&R` code strings, kept verbatim.
#[derive(Default)]
struct HeaderFooter {
    texts: [String; 6],
    different_odd_even: bool,
    different_first: bool,
    scale_with_doc: bool,
    align_with_margins: bool,
}
fn read_header_footer(payload: &[u8]) -> Result<HeaderFooter, IoError> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    let mut hf = HeaderFooter {
        different_odd_even: flags & 0x0001 != 0,
        different_first: flags & 0x0002 != 0,
        scale_with_doc: flags & 0x0004 != 0,
        align_with_margins: flags & 0x0008 != 0,
        ..HeaderFooter::default()
    };
    for slot in &mut hf.texts {
        // `HeaderFooterString` is capped at 255 characters; so is each of the model's strings.
        *slot = b.string(255, true)?.map(|t| t.chars().take(255).collect()).unwrap_or_default();
    }
    b.finish()?;
    Ok(hf)
}
impl HeaderFooter {
    fn store(self, print: &mut PrintSettings) {
        let [header, footer, even_header, even_footer, first_header, first_footer] = self.texts;
        print.header = header;
        print.footer = footer;
        print.even_header = even_header;
        print.even_footer = even_footer;
        print.first_header = first_header;
        print.first_footer = first_footer;
        print.different_odd_even = self.different_odd_even;
        print.different_first = self.different_first;
        print.hf_scale_with_doc = self.scale_with_doc;
        print.hf_align_with_margins = self.align_with_margins;
    }
}

/// `BrtBrk` ([MS-XLSB] 2.4.315): the row (or column) index of one manual page break.
fn read_break(payload: &[u8], row: bool) -> Result<u32, IoError> {
    let mut b = Bytes::new(payload);
    let at = b.u32()?;
    b.u32()?; // unColRwStrt
    b.u32()?; // unColRwEnd
    b.u32()?; // fMan
    b.u32()?; // fPivot
    b.finish()?;
    let max = if row { MAX_ROWS } else { MAX_COLS };
    if at == 0 || at >= max {
        return Err(invalid("page break is out of bounds"));
    }
    Ok(at)
}

/// Print records as a worksheet part presents them: `BrtWsProp` carries the fit-to-page flag before
/// `BrtPageSetup` carries the dimensions, so the two are joined at the end, and a `BrtBrk` is a row
/// or a column break only while its collection is open.
#[derive(Default)]
struct PageState {
    fit_to_page: bool,
    fit: Option<(u16, u16)>,
    /// `Some(true)` between `BrtBeginRwBrk` and `BrtEndRwBrk`, `Some(false)` between the column pair.
    breaks: Option<bool>,
    /// Record kinds already warned about: one warning for each kind of damaged record.
    warned: Vec<&'static str>,
}
impl PageState {
    /// Parses one optional print record. A damaged record is skipped so it can neither abort the
    /// import nor hide the other print settings.
    fn read<T>(&mut self, report: &mut ReadReport, kind: &'static str, parse: impl FnOnce() -> Result<T, IoError>) -> Option<T> {
        match parse() {
            Ok(value) => Some(value),
            Err(e) => {
                if !self.warned.contains(&kind) {
                    self.warned.push(kind);
                    report.warn(format!("A damaged XLSB {kind} record was skipped: {e}"));
                }
                None
            }
        }
    }
}

/// Reads an Excel 4.0 macro sheet part ([MS-XLSB] Macro Sheet part: the worksheet record stream)
/// into a detached sheet, or `None` when it can't be read.
#[allow(clippy::too_many_arguments)] // workbook-level tables the sheet decoder reads
fn macro_sheet(
    package: &mut Package<'_>,
    types: &[Entry],
    path: &str,
    name: &str,
    strings: &[Arc<str>],
    styles: &[StyleId],
    dxfs: &[gridcraft_model::Style],
    links: &Links,
    cell_meta: &cell_meta::CellMeta,
    budget: &mut Budget,
    report: &mut ReadReport,
) -> Option<Sheet> {
    check_type(types, path, MACRO_SHEET).or_else(|_| check_type(types, path, INTL_MACRO_SHEET)).ok()?;
    let bytes = package.read(path).ok().flatten()?;
    budget.links = 0;
    budget.comments = 0;
    // A part that fails part-way reports only that it was skipped.
    let mut notes = ReadReport::default();
    let sheet =
        worksheet(&bytes, name, strings, styles, dxfs, links, cell_meta, &[], budget, &mut SheetParts::default(), &mut None, &mut notes, true)
            .ok()?;
    for note in notes.warnings {
        report.warn(note);
    }
    Some(sheet)
}

#[allow(clippy::too_many_arguments)] // workbook-level tables the sheet decoder reads
fn worksheet(
    bytes: &[u8],
    name: &str,
    strings: &[Arc<str>],
    styles: &[StyleId],
    dxfs: &[gridcraft_model::Style],
    links: &Links,
    cell_meta: &cell_meta::CellMeta,
    rels: &[Rel],
    budget: &mut Budget,
    parts: &mut SheetParts,
    drawing: &mut Option<String>,
    report: &mut ReadReport,
    commands: bool,
) -> Result<Sheet, IoError> {
    let mut records = Records::new(bytes);
    records.first(129, report)?;
    let mut sheet = Sheet::new(name);
    let mut data = false;
    let mut data_seen = false;
    let mut row = None;
    let mut col = None;
    let mut col_budget = MAX_COL_ASSIGNMENTS;
    let mut view_seen = false;
    let mut in_first_view = false;
    let mut custom_views = crate::binary_custom_views::SheetViews::default();
    let mut selections = Selections::default();
    let mut groups = Groups::default();
    let mut page = PageState::default();
    let mut rules = rules::Rules::default();
    let mut sparks = sparklines::Sparklines::default();
    // The `icmb` of a BrtCellMeta, for the cell record that follows it.
    let mut meta: Option<u32> = None;
    while let Some((id, payload)) = records.next(report)? {
        if id != 130 && custom_views.record(id, payload, true, report) {
            continue;
        }
        let mut b = Bytes::new(payload);
        match id {
            130 => {
                custom_views.finish(&mut sheet.preserved, report);
                if data || !data_seen {
                    return Err(invalid("incomplete worksheet data"));
                }
                records.end(payload, report)?;
                groups.resolve(&mut sheet, links, budget, report, commands);
                selections.apply(&mut sheet, report);
                if page.fit_to_page {
                    let (w, h) = page.fit.unwrap_or((1, 1));
                    sheet.print.fit_to = Some((w, h));
                }
                rules.finish(name, report);
                return Ok(sheet);
            }
            145 => {
                if data_seen {
                    return Err(invalid("duplicate worksheet data"));
                }
                data_seen = true;
                data = true;
                b.finish()?;
            }
            146 => {
                if !data {
                    return Err(invalid("unexpected worksheet data end"));
                }
                data = false;
                b.finish()?;
            }
            0 => {
                if !data {
                    return Err(invalid("row outside worksheet data"));
                }
                let r = b.u32()?;
                if r >= MAX_ROWS || row.is_some_and(|previous| r <= previous) {
                    return Err(invalid("row is out of bounds or unordered"));
                }
                let ixfe = b.u32()?;
                let height = b.u16()?;
                let flags = b.u16()?;
                b.take(1)?;
                let spans = b.u32()?;
                if spans > 16 {
                    return Err(invalid("too many row spans"));
                }
                b.take(spans as usize * 8)?;
                b.finish()?;
                row_layout(&mut sheet, r, ixfe, height, flags, styles);
                row = Some(r);
                col = None;
                meta = None;
            }
            49 => {
                // BrtCellMeta ([MS-XLSB] 2.4.325): a damaged one still marks the next cell as
                // having cell metadata, with an index no block has.
                meta = Some(page.read(report, "BrtCellMeta", || Bytes::new(payload).u32()).unwrap_or(0));
            }
            1..=11 | 62 => {
                let cm = meta.take();
                if !data {
                    return Err(invalid("cell outside worksheet data"));
                }
                let r = row.ok_or_else(|| invalid("cell before row header"))?;
                let c = b.u32()?;
                if c >= MAX_COLS || col.is_some_and(|previous| c <= previous) {
                    return Err(invalid("column is out of bounds or unordered"));
                }
                let style_ref = b.u32()? & 0x00ff_ffff; // Low 24 bits are iStyleRef.
                let style = styles.get(style_ref as usize).copied().unwrap_or(StyleId::DEFAULT);
                let (value, rgce) = value(id, &mut b, strings, budget)?;
                b.finish()?;
                let pos = CellRef::new(r, c);
                let exp = rgce.and_then(|(tokens, _)| exp_row(tokens));
                if let Some(anchor_row) = exp {
                    groups.pending.push((pos, anchor_row));
                }
                // Cell metadata of the dynamic-array type marks the formula as a dynamic array,
                // as `cm` does in .xlsx: an array formula's range becomes its spill range.
                let dynamic = rgce.is_some() && cm.is_some_and(|i| cell_meta.dynamic(i));
                if dynamic && exp.is_some() {
                    groups.dynamic.insert(pos);
                }
                let formula = rgce.filter(|_| exp.is_none()).and_then(|(rgce, rgcb)| {
                    formula(rgce, rgcb, pos, links, name, commands).unwrap_or_else(|()| {
                        budget.formulas_kept += 1;
                        None
                    })
                });
                // Without that metadata it is a legacy formula, calculated with implicit
                // intersection as Excel does.
                let formula = formula.map(|f| Formula { dynamic, legacy: !dynamic, ..f });
                if !value.is_empty() || formula.is_some() || style != StyleId::DEFAULT {
                    budget.cell()?;
                    let mut cell = Cell::value(value);
                    cell.formula = formula.map(Arc::new);
                    cell.style = style;
                    sheet.cells.set(pos, cell);
                }
                col = Some(c);
            }
            426 | 427 => {
                // BrtArrFmla: RfX, flags, ArrayParsedFormula; BrtShrFmla: RfX, SharedParsedFormula.
                let range = rfx(&mut b)?;
                if id == 426 {
                    b.u8()?;
                }
                let (rgce, rgcb) = parsed_formula(&mut b)?;
                b.finish()?;
                groups.add(range, rgce, rgcb, id == 426);
            }
            535 | 678 => sheet_protection(id, payload, &mut sheet, report),
            536 | 680 => range_protection(id, payload, &mut sheet, report),
            176 => {
                if data {
                    return Err(invalid("merge inside worksheet data"));
                }
                let r0 = b.u32()?;
                let r1 = b.u32()?;
                let c0 = b.u32()?;
                let c1 = b.u32()?;
                b.finish()?;
                if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
                    report.warn("Invalid XLSB merged ranges were skipped.");
                    continue;
                }
                if sheet.merges.len() >= 100_000 {
                    return Err(IoError::TooLarge("XLSB exceeds 100000 merges".into()));
                }
                sheet.merges.push(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)));
            }
            60 => {
                if col_info(&mut sheet, payload, styles, &mut col_budget).is_err() {
                    report.warn("Malformed XLSB column information records were skipped.");
                }
            }
            485 => {
                if sheet_format(&mut sheet, payload).is_err() {
                    report.warn("Malformed XLSB default row height and column width records were skipped.");
                }
            }
            147 => {
                // BrtWsProp ([MS-XLSB] 2.4.875): fApplyStyles (bit 5), fRowSumsBelow (bit 6),
                // fColSumsRight (bit 7), fFitToPage (bit 8) and the tab colour.
                if let Some(flags) = page.read(report, "BrtWsProp", || Bytes::new(payload).u16()) {
                    page.fit_to_page = flags & 0x0100 != 0;
                    sheet.summary_below = flags & 0x0040 != 0;
                    sheet.summary_right = flags & 0x0080 != 0;
                    sheet.outline_apply_styles = flags & 0x0020 != 0;
                }
                if tab_color(&mut sheet, payload).is_err() {
                    report.warn("Malformed XLSB sheet property records were skipped.");
                }
                sheet.code_name = sheet_code_name(payload);
            }
            137 => match sheet_view(&mut sheet, payload, view_seen) {
                Ok(first) => {
                    view_seen = true;
                    in_first_view = first;
                }
                Err(_) => report.warn("Malformed XLSB sheet view records were skipped."),
            },
            138 => in_first_view = false,
            151 if in_first_view => selections.pane(pane(&mut sheet, payload), report),
            152 if in_first_view => selections.selection(selection(payload)),
            494 => hyperlink(payload, rels, &mut sheet, budget, report),
            161 if !data => sheet_autofilter(&mut records, payload, dxfs, &mut sheet, report)?,
            tables::LIST_PART => list_part(payload, &mut parts.lists, report),
            550 => match drawing_rel(b) {
                Ok(id) => *drawing = id,
                Err(_) => report.warn("An unreadable XLSB drawing record was skipped."),
            },
            LEGACY_DRAWING => match b.string(MAX_REL_ID, true) {
                Ok(id) => parts.legacy_drawing = id,
                Err(_) => report.warn("A damaged XLSB legacy drawing record was skipped; its notes stay hidden with their default box."),
            },
            476 => {
                if let Some(m) = page.read(report, "BrtMargins", || read_margins(payload)) {
                    sheet.print.margins = m;
                }
            }
            477 => {
                if let Some(o) = page.read(report, "BrtPrintOptions", || read_print_options(payload)) {
                    o.store(&mut sheet.print);
                }
            }
            478 => {
                if let Some(p) = page.read(report, "BrtPageSetup", || read_page_setup(payload)) {
                    let rel_id = p.rel_id.clone();
                    page.fit = Some(p.store(&mut sheet.print));
                    if rel_id.is_some() {
                        parts.printer_settings = rel_id;
                    }
                }
            }
            479 => {
                if let Some(hf) = page.read(report, "BrtBeginHeaderFooter", || read_header_footer(payload)) {
                    hf.store(&mut sheet.print);
                }
            }
            392 => page.breaks = Some(true),
            393 => page.breaks = None,
            394 => page.breaks = Some(false),
            395 => page.breaks = None,
            396 => {
                let Some(row) = page.breaks else {
                    // A BrtBrk outside its collection is not a page break; it is not an error either.
                    continue;
                };
                if let Some(at) = page.read(report, "BrtBrk", || read_break(payload, row)) {
                    let breaks = if row { &mut sheet.print.row_breaks } else { &mut sheet.print.col_breaks };
                    if !breaks.contains(&at) {
                        breaks.push(at);
                        breaks.sort_unstable();
                    }
                }
            }
            64 | 573 | 574 => rules.validation(id, payload, &mut sheet, budget, links)?,
            461..=471 | 564 => rules.cond_format(id, payload, &mut sheet, dxfs, links),
            _ if sparklines::is_record(id) => sparks.record(id, payload, links, &mut sheet, report),
            _ => {}
        }
    }
    Err(invalid("missing worksheet end record"))
}

/// Row height, visibility, outline and style from the BrtRowHdr fields after `rw`.
fn row_layout(sheet: &mut Sheet, r: u32, ixfe: u32, height: u16, flags: u16, styles: &[StyleId]) {
    let on = |bit: u16| flags & bit != 0;
    let mut info = LineInfo { custom: on(0x2000), hidden: on(0x1000), ..LineInfo::default() };
    let px = pt_to_px(f64::from(height) / 20.0);
    if px > 0.0 && (info.custom || info.hidden || (px - sheet.default_row_height).abs() > 0.5) {
        info.size = Some(px);
    }
    info.outline = ((flags >> 8) & 7) as u8;
    info.collapsed = on(0x0800);
    // fGhostDirty: the row carries its own format.
    if on(0x4000) {
        let style = styles.get(ixfe as usize).copied().unwrap_or(StyleId::DEFAULT);
        if style != StyleId::DEFAULT {
            info.style = Some(style);
        }
    }
    if info != LineInfo::default() {
        sheet.rows.insert(r, info);
    }
}

/// BrtColInfo: colFirst, colLast, coldx (1/256 characters), ixfe and flags.
fn col_info(sheet: &mut Sheet, payload: &[u8], styles: &[StyleId], budget: &mut u64) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    let first = b.u32()?;
    let last = b.u32()?.min(MAX_COLS - 1);
    let width = b.u32()?;
    let ixfe = b.u32()?;
    let flags = b.u16()?;
    if first > last {
        return Err(invalid("invalid column range"));
    }
    let mut info = LineInfo::default();
    let px = col_width_to_px(f64::from(width) / 256.0);
    // fUserSet: a custom width equal to the default still pins the column.
    if flags & 0x0002 != 0 || px != sheet.default_col_width {
        info.size = Some(px);
    }
    info.hidden = flags & 0x0001 != 0;
    info.outline = ((flags >> 8) & 7) as u8;
    info.collapsed = flags & 0x1000 != 0;
    let style = styles.get(ixfe as usize).copied().unwrap_or(StyleId::DEFAULT);
    if style != StyleId::DEFAULT {
        info.style = Some(style);
    }
    if info == LineInfo::default() {
        return Ok(());
    }
    *budget = budget.checked_sub(u64::from(last - first) + 1).ok_or_else(|| invalid("too many column assignments"))?;
    for c in first..=last {
        sheet.cols.insert(c, info);
    }
    Ok(())
}

/// BrtWsFmtInfo: default column width (dxGCol, else cchDefColWidth) and default row height.
fn sheet_format(sheet: &mut Sheet, payload: &[u8]) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    let width = b.u32()?;
    let chars = b.u16()?;
    let height = b.u16()?;
    let px = pt_to_px(f64::from(height) / 20.0);
    if px > 0.0 {
        sheet.default_row_height = px;
    }
    if width != u32::MAX {
        let px = col_width_to_px(f64::from(width) / 256.0);
        if px > 0.0 {
            sheet.default_col_width = px;
        }
    } else {
        // A base width in characters gets the cell padding, rounded up to whole 8 px steps.
        let px = ((f64::from(chars).min(255.0) * 7.0 + 5.0) / 8.0).ceil() * 8.0;
        sheet.default_col_width = px as f32;
    }
    Ok(())
}

/// BrtWsProp: the tab colour (BrtColor) follows three bytes of flags.
fn tab_color(sheet: &mut Sheet, payload: &[u8]) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    b.take(3)?;
    let color = styles::color(&mut b)?;
    sheet.tab_color = if color == Color::Auto { None } else { Some(color) };
    Ok(())
}

/// BrtWsProp's strName, the sheet's code name, after the flags, the tab colour (BrtColor),
/// rwSync and colSync. `None` when it is empty, null or missing.
fn sheet_code_name(payload: &[u8]) -> Option<String> {
    let mut b = Bytes::new(payload);
    b.take(3 + 8 + 4 + 4).ok()?;
    b.string(31, true).ok().flatten().filter(|n| !n.is_empty())
}

/// BrtBeginWsView: display options of the first workbook view's sheet view. `Ok(true)` when
/// this view was applied (its pane applies too).
fn sheet_view(sheet: &mut Sheet, payload: &[u8], seen: bool) -> Result<bool, IoError> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    let view = b.u32()?; // xlView.
    b.take(12)?; // rwTop, colLeft, icvHdr and reserved fields.
    let zoom = b.u16()?;
    b.take(6)?; // wScaleNormal, wScaleSLV, wScalePLV.
    let book_view = b.u32()?;
    if seen || book_view != 0 {
        return Ok(false);
    }
    let on = |bit: u16| flags & bit != 0;
    sheet.show_formulas = on(0x0002);
    sheet.show_gridlines = on(0x0004);
    sheet.show_headings = on(0x0008);
    sheet.show_zeros = on(0x0010);
    sheet.right_to_left = on(0x0020);
    sheet.show_outline_symbols = on(0x0100);
    sheet.zoom = if zoom == 0 { 100 } else { zoom.clamp(10, 400) };
    sheet.view = match view {
        1 => SheetViewMode::PageBreakPreview,
        2 => SheetViewMode::PageLayout,
        _ => SheetViewMode::Normal,
    };
    Ok(true)
}

/// BrtPane: frozen panes keep their counts of frozen rows and columns; split panes are ignored.
/// `Ok(Some(pnnAct))` when the record names the active pane.
fn pane(sheet: &mut Sheet, payload: &[u8]) -> Result<Option<u32>, IoError> {
    let mut b = Bytes::new(payload);
    let x = b.f64()?;
    let y = b.f64()?;
    let top = b.u32()?;
    let left = b.u32()?;
    let active = b.u32()?; // pnnAct.
    let flags = b.u8()?;
    if flags & 0x01 == 0 {
        return Ok(None);
    }
    let cols = x.clamp(0.0, f64::from(MAX_COLS - 1)) as u32;
    let rows = y.clamp(0.0, f64::from(MAX_ROWS - 1)) as u32;
    if rows > 0 || cols > 0 {
        sheet.freeze = Some((rows, cols));
        if top < MAX_ROWS && left < MAX_COLS {
            sheet.view_top_left = CellRef::new(top, left);
        }
    }
    Ok(Some(active))
}

/// The first sheet view's active pane and selections, resolved to the sheet's active cell
/// once the worksheet ends.
#[derive(Default)]
struct Selections {
    active_pane: Option<u32>,
    cells: Vec<(u32, CellRef)>,
    malformed: bool,
}

impl Selections {
    fn pane(&mut self, pane: Result<Option<u32>, IoError>, report: &mut ReadReport) {
        match pane {
            Ok(p) => self.active_pane = p,
            Err(_) => report.warn("Malformed XLSB pane records were skipped."),
        }
    }

    fn selection(&mut self, selection: Result<Option<(u32, CellRef)>, IoError>) {
        match selection {
            Ok(Some(s)) => self.cells.push(s),
            Ok(None) => {}
            Err(_) => self.malformed = true,
        }
    }

    /// The active pane's selection, else the last one (as the .xlsx reader does).
    fn apply(&self, sheet: &mut Sheet, report: &mut ReadReport) {
        let chosen = self.active_pane.and_then(|p| self.cells.iter().find(|(pnn, _)| *pnn == p)).or(self.cells.last());
        if let Some((_, at)) = chosen {
            sheet.view_active = *at;
        }
        if self.malformed {
            report.warn("Malformed XLSB selection records were skipped.");
        }
    }
}

/// BrtSel ([MS-XLSB] 2.4.722): the pane it belongs to (`pnn`), the active cell (`rwAct`,
/// `colAct`) and the selected ranges. Only the active cell is kept; `None` when it is outside
/// the sheet.
fn selection(payload: &[u8]) -> Result<Option<(u32, CellRef)>, IoError> {
    let mut b = Bytes::new(payload);
    let pnn = b.u32()?;
    let row = b.u32()?;
    let col = b.u32()?;
    b.u32()?; // dwRfxAct.
    let count = b.u32()?;
    for _ in 0..count {
        b.take(16)?; // UncheckedRfX.
    }
    if row >= MAX_ROWS || col >= MAX_COLS {
        return Ok(None);
    }
    Ok(Some((pnn, CellRef::new(row, col))))
}

/// An `UncheckedRfX` ([MS-XLSB] 2.5.158): first row, last row, first column, last column.
/// `None` when it is reversed or outside the sheet.
fn range(b: &mut Bytes<'_>) -> Result<Option<RangeRef>, IoError> {
    let (r0, r1, c0, c1) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
    if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
        return Ok(None);
    }
    Ok(Some(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1))))
}

/// BrtHLink ([MS-XLSB] 2.4.676): the range, the relationship id of a file or web page, the
/// in-workbook location, the tooltip and the display text. The target is built the way the
/// .xlsx reader builds it from `<hyperlink>`, and every cell of the range gets the link.
fn hyperlink(payload: &[u8], rels: &[Rel], sheet: &mut Sheet, budget: &mut Budget, report: &mut ReadReport) {
    let parse = || -> Result<_, IoError> {
        let mut b = Bytes::new(payload);
        let range = range(&mut b)?;
        let rel = b.string(32767, true)?.unwrap_or_default();
        let location = b.text(32767)?;
        let tooltip = b.text(32767)?;
        b.text(32767)?; // display: the cell's own value is what shows.
        Ok((range, rel, location, tooltip))
    };
    let Ok((range, rel, location, tooltip)) = parse() else {
        report.warn("Malformed XLSB hyperlinks were skipped.");
        return;
    };
    let Some(range) = range else {
        report.warn("XLSB hyperlinks outside the sheet were skipped.");
        return;
    };
    let mut target = String::new();
    if !rel.is_empty() {
        let Some(r) = rels.iter().find(|r| r.id == rel && r.kind == "hyperlink") else {
            report.warn("XLSB hyperlinks with a missing relationship were skipped.");
            return;
        };
        target = r.target.clone();
    }
    // A relationship is a file or web page; a bare location is a place in this workbook.
    let external = !target.is_empty();
    if !location.is_empty() {
        if target.is_empty() {
            target = location.trim_start_matches('#').to_string();
        } else {
            target = format!("{target}#{location}");
        }
    }
    if target.is_empty() {
        return;
    }
    if budget.text(target.len().saturating_add(tooltip.len())).is_err() {
        report.warn("XLSB hyperlinks past the text limit were skipped.");
        return;
    }
    let link = Hyperlink { target, tooltip: (!tooltip.is_empty()).then_some(tooltip), external };
    for c in range.iter() {
        if budget.links >= MAX_LINK_CELLS {
            report.warn(format!("XLSB hyperlinks past the first {MAX_LINK_CELLS} cells of a sheet were dropped."));
            return;
        }
        budget.links += 1;
        sheet.hyperlinks.insert(c, link.clone());
    }
}

/// Imports the notes of the sheet's comments part, if any. A part that is missing, of the wrong
/// type or damaged is skipped with one warning; the sheet keeps its cells.
fn sheet_comments(package: &mut Package<'_>, types: &[Entry], rels: &[Rel], sheet: &mut Sheet, budget: &mut Budget, report: &mut ReadReport) {
    let Some(rel) = rels.iter().find(|r| r.kind == "comments") else { return };
    let mut part = || -> Result<Vec<u8>, IoError> {
        if rel.external {
            return Err(invalid("external comments relationship"));
        }
        check_type(types, &rel.target, COMMENTS)?;
        required(package, &rel.target)
    };
    let parsed = part().and_then(|bytes| comments(&bytes, budget, report));
    match parsed {
        Ok(notes) => {
            for (cell, note) in notes {
                sheet.comments.insert(cell, note);
            }
        }
        Err(_) => report.warn("Damaged XLSB comments were skipped."),
    }
}

/// Relationship ids a worksheet's records name, resolved once the sheet is read.
#[derive(Default)]
struct SheetParts {
    /// BrtListPart ids, one per table.
    lists: Vec<String>,
    /// The BrtLegacyDrawing id: the VML drawing holding the notes' shapes.
    legacy_drawing: Option<String>,
    /// The BrtPageSetup `szRelID`: the sheet's printer settings part.
    printer_settings: Option<String>,
}

/// The workbook's persons part (authors of threaded comments), by person id. None without the
/// part; none with one warning when it can't be read.
fn persons(package: &mut Package<'_>, rels: &[Rel], report: &mut ReadReport) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(rel) = rels.iter().find(|r| r.kind.strip_prefix("unsupported:") == Some(PERSON_REL)) else { return out };
    match xml_part(package, rel) {
        Ok(x) => {
            for p in x.kids("person") {
                if let (Some(id), Some(n)) = (p.attr("id"), p.attr("displayName")) {
                    out.insert(id.to_string(), n.to_string());
                }
            }
        }
        Err(_) => report.warn("The XLSB persons part could not be read; threaded comments were imported without their authors."),
    }
    out
}

/// An internal XML part named by a relationship; an error when it is external, missing or damaged.
fn xml_part(package: &mut Package<'_>, rel: &Rel) -> Result<crate::xml::El, IoError> {
    if rel.external {
        return Err(invalid("external relationship"));
    }
    package.read_xml(&rel.target)?.ok_or_else(|| invalid("missing part"))
}

/// The sheet's XML comment parts, read after its binary notes the way the .xlsx reader reads
/// them: threaded comments replace their flattened placeholder notes, then the VML drawing named
/// by BrtLegacyDrawing gives the notes their Show Note state and box. A part that is missing or
/// damaged keeps the plain notes, with one warning.
fn comment_parts(
    package: &mut Package<'_>,
    rels: &[Rel],
    legacy_drawing: Option<&str>,
    persons: &HashMap<String, String>,
    sheet: &mut Sheet,
    report: &mut ReadReport,
) {
    for rel in rels.iter().filter(|r| r.kind.strip_prefix("unsupported:") == Some(THREADED_REL)) {
        match xml_part(package, rel) {
            Ok(x) => crate::sheet_read::read_threaded(persons, &x, sheet),
            Err(_) => report.warn("Damaged XLSB threaded comments were skipped; their cells keep the plain notes."),
        }
    }
    let Some(id) = legacy_drawing.filter(|_| !sheet.comments.is_empty()) else { return };
    let vml = rels
        .iter()
        .find(|r| r.id == id && r.kind == "vmlDrawing")
        .ok_or_else(|| invalid("missing legacy drawing relationship"))
        .and_then(|rel| Ok((xml_part(package, rel)?, rel.target.clone())));
    match vml {
        Ok((x, part)) => crate::sheet_read::read_note_shapes(&x, sheet, 0, &mut crate::sheet_read::NotePictures::new(package, &part)),
        Err(_) => report.warn("The XLSB notes drawing could not be read; notes stay hidden with their default box."),
    }
}

/// The comments part ([MS-XLSB] 2.1.7.8): BrtBeginComments, the author list, then the comment
/// list with one BrtBeginComment / BrtCommentText / BrtEndComment group per note.
fn comments(bytes: &[u8], budget: &mut Budget, report: &mut ReadReport) -> Result<Vec<(CellRef, Comment)>, IoError> {
    let mut records = Records::new(bytes);
    records.first(628, report)?;
    let mut authors: Vec<String> = vec![];
    let mut out = vec![];
    // The open comment: its author index and cell (None when outside the sheet), then its text.
    let mut open: Option<(u32, Option<CellRef>, Option<String>)> = None;
    let (mut bad_author, mut bad_cell) = (false, false);
    while let Some((id, payload)) = records.next(report)? {
        let mut b = Bytes::new(payload);
        match id {
            629 => {
                if open.is_some() {
                    return Err(invalid("unterminated comment"));
                }
                records.end(payload, report)?;
                if bad_author {
                    report.warn("XLSB comments with an unknown author were imported without one.");
                }
                if bad_cell {
                    report.warn("XLSB comments outside the sheet were skipped.");
                }
                return Ok(out);
            }
            632 => {
                let author = b.text(32767)?;
                budget.text(author.len())?;
                if authors.len() >= MAX_COMMENTS {
                    return Err(IoError::TooLarge("too many XLSB comment authors".into()));
                }
                authors.push(author);
            }
            635 => {
                if open.is_some() {
                    return Err(invalid("nested comment"));
                }
                let author = b.u32()?;
                let cell = range(&mut b)?.map(|r| r.start);
                b.take(16)?; // guid
                b.finish()?;
                open = Some((author, cell, None));
            }
            637 => {
                let Some((_, _, text)) = open.as_mut() else { return Err(invalid("comment text outside a comment")) };
                let t = rich_string(&mut b)?;
                budget.text(t.len())?;
                *text = Some(t);
            }
            636 => {
                let (author, cell, text) = open.take().ok_or_else(|| invalid("unexpected comment end"))?;
                let Some(cell) = cell else {
                    bad_cell = true;
                    continue;
                };
                if out.len() >= MAX_COMMENTS || budget.comments >= MAX_COMMENTS {
                    report.warn(format!("XLSB comments past the first {MAX_COMMENTS} of a sheet were dropped."));
                    continue;
                }
                let author = authors.get(author as usize).cloned().unwrap_or_else(|| {
                    bad_author = true;
                    String::new()
                });
                let mut text = text.unwrap_or_default();
                // Excel starts a note with an "Author:" line, as in .xlsx.
                if !author.is_empty()
                    && let Some(rest) = text.strip_prefix(&format!("{author}:"))
                {
                    text = rest.strip_prefix('\n').unwrap_or(rest).to_string();
                }
                budget.comments += 1;
                out.push((
                    cell,
                    Comment {
                        author,
                        text,
                        replies: vec![],
                        threaded: false,
                        resolved: false,
                        visible: false,
                        dates: vec![],
                        note_box: None,
                        picture_fill: None,
                        note_shape: None,
                    },
                ));
            }
            _ => {}
        }
    }
    Err(invalid("missing comments end record"))
}

/// The worksheet AutoFilter block (`BrtBeginAFilter` ... `BrtEndAFilter`); only the first is kept.
fn sheet_autofilter(
    records: &mut Records<'_>,
    payload: &[u8],
    dxfs: &[gridcraft_model::Style],
    sheet: &mut Sheet,
    report: &mut ReadReport,
) -> Result<(), IoError> {
    let af = tables::autofilter(records, payload, dxfs, report)?;
    if sheet.autofilter.is_none() {
        sheet.autofilter = af;
    }
    Ok(())
}

/// A `BrtListPart`: the relationship id of one of the sheet's table parts.
fn list_part(payload: &[u8], list_parts: &mut Vec<String>, report: &mut ReadReport) {
    match tables::list_part(payload) {
        Ok(Some(id)) if list_parts.len() < tables::MAX_TABLES => list_parts.push(id),
        Ok(Some(_)) => report.warn("XLSB tables past the first 1024 on a sheet were skipped."),
        Ok(None) | Err(_) => report.warn("XLSB tables whose part is missing or malformed were skipped."),
    }
}

/// [MS-XLSB] 2.4.318: the thread count precedes the final two-byte flags.
fn calc_prop(payload: &[u8], wb: &mut Workbook, report: &mut ReadReport) {
    let parse = || -> Result<_, IoError> {
        let mut b = Bytes::new(payload);
        b.u32()?; // recalcID
        let mode = match b.u32()? {
            0 => CalcMode::Manual,
            1 => CalcMode::Automatic,
            2 => CalcMode::AutomaticExceptTables,
            _ => return Err(invalid("invalid calculation mode")),
        };
        let count = b.u32()?;
        let delta = b.f64()?;
        b.u32()?; // cUserThreadCount (not modelled)
        let flags = b.u16()?;
        b.finish()?;
        Ok((mode, count, delta, flags))
    };
    match parse() {
        Ok((mode, count, delta, flags)) => {
            wb.calc.mode = mode;
            wb.calc.set_max_iterations(count);
            wb.calc.set_max_change(delta);
            wb.calc.iterative = flags & 4 != 0;
            wb.calc.precision_as_displayed = flags & 8 == 0;
        }
        Err(_) => report.warn("Invalid XLSB BrtCalcProp record was skipped."),
    }
}

fn legacy_password(value: u16) -> Option<PasswordHash> {
    (value != 0).then_some(PasswordHash::Legacy(value))
}

/// IsoPasswordData ([MS-XLSB] 2.5.80): two LPByteBufs and an XLNullableWideString.
fn iso_password(b: &mut Bytes<'_>, spin_count: u32) -> Result<Option<PasswordHash>, IoError> {
    if spin_count > 10_000_000 {
        return Err(invalid("password spin count exceeds its limit"));
    }
    // Bound encoded allocations independently of the record's untrusted length fields.
    let mut buffer = || -> Result<_, IoError> {
        let len = b.u32()?;
        if len > 65_536 {
            return Err(invalid("password data exceeds its limit"));
        }
        b.take(len as usize)
    };
    let hash = buffer()?;
    let salt = buffer()?;
    let algorithm = b.string(255, true)?.unwrap_or_default();
    if hash.is_empty() {
        if !salt.is_empty() || !algorithm.is_empty() {
            return Err(invalid("password data without a hash"));
        }
        return Ok(None);
    }
    if algorithm.is_empty() {
        return Err(invalid("password hash without an algorithm"));
    }
    Ok(Some(PasswordHash::Iterated { algorithm, hash: gridcraft_model::encode_base64(hash), salt: gridcraft_model::encode_base64(salt), spin_count }))
}

/// [MS-XLSB] 2.4.311–312. Window and revision protection are not modelled.
fn book_protection(id: u16, payload: &[u8], wb: &mut Workbook, report: &mut ReadReport) {
    let parse = || -> Result<_, IoError> {
        let mut b = Bytes::new(payload);
        let (flags, password) = if id == 677 {
            let spin = b.u32()?;
            let revision_spin = b.u32()?;
            let flags = b.u16()?;
            let password = iso_password(&mut b, spin)?;
            let revision = iso_password(&mut b, revision_spin)?;
            if password.is_none() && revision.is_none() {
                return Err(invalid("empty workbook ISO protection"));
            }
            (flags, password)
        } else {
            let password = legacy_password(b.u16()?);
            b.u16()?; // protpwdRev
            (b.u16()?, password)
        };
        b.finish()?;
        Ok((flags, password))
    };
    match parse() {
        Ok((flags, password)) => {
            // ISO records normally precede their legacy companion, which must not erase them.
            if id == 677 || !matches!(wb.structure_password, Some(PasswordHash::Iterated { .. })) {
                wb.protected_structure = flags & 1 != 0;
                wb.structure_password = password;
            }
        }
        Err(_) => report.warn(format!("Invalid XLSB workbook protection record {id} was skipped.")),
    }
}

/// [MS-XLSB] 2.4.792–793: 16 four-byte protection flags in both record types.
fn sheet_protection(id: u16, payload: &[u8], sheet: &mut Sheet, report: &mut ReadReport) {
    let parse = || -> Result<_, IoError> {
        let mut b = Bytes::new(payload);
        let spin = if id == 678 { Some(b.u32()?) } else { None };
        let legacy = if spin.is_none() { legacy_password(b.u16()?) } else { None };
        let mut flags = [0; 16];
        for flag in &mut flags {
            *flag = b.u32()?;
            if *flag > 1 {
                return Err(invalid("invalid sheet protection boolean"));
            }
        }
        let password =
            if let Some(spin) = spin { Some(iso_password(&mut b, spin)?.ok_or_else(|| invalid("empty sheet ISO protection"))?) } else { legacy };
        b.finish()?;
        let [
            locked,
            _,
            _,
            cells,
            columns,
            rows,
            insert_columns,
            insert_rows,
            _,
            delete_columns,
            delete_rows,
            select_locked,
            sort,
            autofilter,
            _,
            select_unlocked,
        ] = flags;
        Ok((locked != 0).then_some(SheetProtection {
            password,
            legacy_password_hash: None,
            select_locked: select_locked == 0,
            select_unlocked: select_unlocked == 0,
            format_cells: cells == 0,
            format_columns: columns == 0,
            format_rows: rows == 0,
            insert_columns: insert_columns == 0,
            insert_rows: insert_rows == 0,
            delete_columns: delete_columns == 0,
            delete_rows: delete_rows == 0,
            sort: sort == 0,
            autofilter: autofilter == 0,
            ..Default::default()
        }))
    };
    match parse() {
        Ok(protection) => {
            if id == 678 || !sheet.protection.as_ref().is_some_and(|p| matches!(p.password, Some(PasswordHash::Iterated { .. }))) {
                sheet.protection = protection;
            }
        }
        Err(_) => report.warn(format!("Invalid XLSB sheet protection record {id} was skipped.")),
    }
}

// BrtRangeProtection / BrtRangeProtectionIso ([MS-XLSB] 2.4.763 / 2.4.765).
fn range_protection(id: u16, payload: &[u8], sheet: &mut Sheet, report: &mut ReadReport) {
    let parse = || -> Result<_, IoError> {
        let mut b = Bytes::new(payload);
        let spin = if id == 680 { Some(b.u32()?) } else { None };
        let legacy = if spin.is_none() { legacy_password(b.u16()?) } else { None };
        // UncheckedSqRfX (2.5.156): count followed by 16-byte UncheckedRfX values.
        let count = b.u32()? as usize;
        if count == 0 || count >= 8192 || count > b.rest.len() / 16 {
            return Err(invalid("invalid edit range count"));
        }
        let mut ranges = Vec::with_capacity(count);
        for _ in 0..count {
            ranges.push(range(&mut b)?.ok_or_else(|| invalid("invalid edit range reference"))?);
        }
        // RangeProtectionTitleSDRel (2.5.113).
        let title = b.text(255)?;
        if title.is_empty() {
            return Err(invalid("empty edit range title"));
        }
        let size = b.u32()? as usize;
        b.take(size)?;
        let password =
            if let Some(spin) = spin { Some(iso_password(&mut b, spin)?.ok_or_else(|| invalid("empty edit range ISO password"))?) } else { legacy };
        b.finish()?;
        Ok((EditRange { title, ranges, password, security_descriptor: None }, size != 0))
    };
    match parse() {
        Ok((edit, permissions)) => {
            // ISO records precede a passwordless legacy duplicate. Keep the stronger hash.
            let duplicate = id == 536
                && edit.password.is_none()
                && sheet.edit_ranges.last().is_some_and(|previous| {
                    previous.title == edit.title && previous.ranges == edit.ranges && matches!(previous.password, Some(PasswordHash::Iterated { .. }))
                });
            if !duplicate {
                sheet.edit_ranges.push(edit);
            }
            if permissions {
                report.warn("Windows user permissions on Allow Edit Ranges were not kept; their passwords still apply.");
            }
        }
        Err(_) => report.warn("An invalid XLSB Allow Edit Ranges record was skipped."),
    }
}
