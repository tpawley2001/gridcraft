//! Legacy Excel 97-2003 `.xls` (BIFF8) import, implemented from Microsoft's public
//! specifications [MS-CFB] (the compound file container) and [MS-XLS] (the Workbook stream):
//! <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/cd03cb5f-ca02-4934-a391-bb674cb8aa06>.
//! Excel 5.0/95 workbooks (BIFF5, a `Book` stream) go through the same reader: their strings
//! are 8-bit text in the workbook's code page, there is no shared string table, and formula
//! tokens, defined names and 3-D references use the BIFF5 layouts ([`formula::Biff5`]).
//!
//! Read: worksheets, shared strings, cell values, formulas (text and cached results, including
//! shared and array formulas), cell formatting (number formats, fonts, fills, borders,
//! alignment, protection, the colour palette, theme and RGB cell colours from XFExt records and
//! user-defined cell styles), the workbook theme (Theme record), the 1904 date
//! system, merged cells, column widths, row heights, document properties, defined names, print areas, print titles,
//! the calculation settings (mode, iteration, precision as displayed), data validation and
//! conditional formats (cell-value and formula rules, and the Excel 2007+ kinds: colour scales,
//! data bars, icon sets, top/bottom, above average and text/date templates), tables (name,
//! range, header and totals rows, columns and table style), page setup (paper, orientation,
//! scale, fit to page, margins, headers and footers, print options, manual page breaks), the
//! sheet tab colour, sheet protection, Allow Edit Ranges (titles, references and legacy passwords), workbook structure protection, the sheet AutoFilter
//! (range and criteria), charts: embedded charts and chart sheets become live charts whose
//! series refer to the cells (see `chart`), PNG and JPEG pictures (see `drawing`), and PivotTables on worksheet-range or defined-name
//! sources (cache streams in `_SX_DB_CUR` and the SxView views, in `xls/pivot.rs`), and query tables with
//! their data connections (kept for .xlsx, in `xls/connections.rs`), and links to other workbooks
//! (kept for .xlsx, in `xls/external_links.rs`). Everything else (other shapes, comments, macros) is
//! skipped with a warning.
//! Workbooks encrypted with Excel's default password (`VelvetSweatshop`) are decrypted first
//! (see `xls_crypto`), as Excel opens them without a prompt; others open with their password
//! ([`read_xls_with_password`]). Versions before BIFF5 are refused.
//!
//! Every formula in the file is a legacy (pre-dynamic-array) one, so it is read with Excel's
//! implicit `@` (see `array_formula.rs`), like an `.xlsx` formula saved without `cm`.

mod autofilter;
mod cetab;
mod chart;
mod cond_format;
mod connections;
mod doc_props;
mod drawing;
mod external_links;
pub(crate) mod formula;
mod ftab;
pub(crate) mod macro_sheet;
mod page_setup;
mod pivot;
mod png;
mod protection;
mod records;
mod styles;
mod tables;
mod textbox;
mod validation;

pub(crate) use chart::{ChartSheet, add_chart_sheets};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gridcraft_core::{CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_model::{
    CalcMode, CalcSettings, Cell, DefinedName, Formula, LineInfo, Sheet, SheetViewMode, StyleId, Visibility, Workbook, WorkbookView,
};

use crate::DecryptError;
use crate::cfb::{self, Compound};
use crate::tables::{col_width_to_px, pt_to_px};
use crate::xls_crypto;
use crate::{FunctionInfo, IoError, ReadReport};
use formula::{Biff5, Extern, ExternKind, Links, Mode, SupBook, Unsupported, Xti, cell_error, decode, decode_cell, decode_name, external_sheet_text};
use records::{Cursor, Record, Records};

const MAX_CELLS: usize = 4_000_000;
const MAX_STRINGS: usize = 4_000_000;
const MAX_TEXT: usize = 256 * 1024 * 1024;
const MAX_SHEETS: usize = 4096;
const MAX_NAMES: usize = 65_536;
const MAX_XFS: usize = 65_536;
const MAX_MERGES: usize = 100_000;
/// Picture bytes kept per workbook at most.
const MAX_PICTURE_BYTES: usize = 512 * 1024 * 1024;
/// Records read from the whole Workbook stream at most (a 65,536 × 256 sheet has fewer). One
/// allowance shared by every substream: many BoundSheet records pointing into one huge
/// substream must not each get their own.
const MAX_RECORDS: usize = 40_000_000;

const BOF: u16 = 0x0809;
const EOF: u16 = 0x000A;
const BIFF8: u16 = 0x0600;
const BIFF5: u16 = 0x0500;
/// WsBool (MS-XLS 2.4.351): its first byte's `fDialog` bit marks a dialog sheet, whose BOF says worksheet.
const WSBOOL: u16 = 0x0081;
const WSBOOL_DIALOG: u8 = 0x10;
/// WsBool as a little-endian u16: `fRowSumsBelow`, `fColSumsRight` and `fFitToPage`.
const WSBOOL_ROW_SUMS_BELOW: u16 = 0x0040;
const WSBOOL_COL_SUMS_RIGHT: u16 = 0x0080;
const WSBOOL_FIT_TO_PAGE: u16 = 0x0100;
/// WsBool as a little-endian u16: `fApplyStyles`, Outline Settings "Automatic styles".
const WSBOOL_APPLY_STYLES: u16 = 0x0020;

pub(crate) fn invalid(msg: impl Into<String>) -> IoError {
    IoError::Format(format!("XLS: {}", msg.into()))
}

/// The message for an encrypted workbook (a FilePass record).
const ENCRYPTED: &str =
    "this .xls workbook is password-protected (encrypted). encrypted .xls files can't be opened; remove the password in Excel and try again.";

/// Whether the bytes are a compound file holding an Excel workbook stream (BIFF8 `Workbook`
/// or BIFF5 `Book`).
pub(crate) fn is_xls(bytes: &[u8]) -> bool {
    let Ok(cf) = Compound::open(bytes) else { return false };
    cf.has_root_stream("Workbook") || cf.has_root_stream("Book")
}

/// Most shared formulas one sheet expands, and the longest template text one attach may
/// produce: a template shared by its followers was costing every one of them a full decode and
/// print however often that could never succeed, so each anchor is tried once and failed ones
/// are skipped for the rest of the sheet. Beyond the cap the remaining references keep their
/// cached values.
pub(crate) const MAX_SHARED_ATTACHES: usize = 262_144;
const MAX_SHARED_TEMPLATE: usize = 8192;

pub(crate) struct Budget {
    cells: usize,
    text: usize,
    records: usize,
    max_records: usize,
    /// Charts read so far.
    charts: usize,
    /// Picture bytes kept so far.
    pictures: usize,
    /// A shared formula's text was refused: no later shared formula, in any sheet, is expanded.
    shared_text_spent: bool,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { cells: 0, text: 0, records: 0, max_records: MAX_RECORDS, charts: 0, pictures: 0, shared_text_spent: false }
    }
}

impl Budget {
    /// A budget with a smaller record allowance (tests).
    #[cfg(test)]
    pub(crate) fn with_max_records(max_records: usize) -> Self {
        Budget { max_records, ..Budget::default() }
    }
    /// A budget with only `left` bytes of text allowance (tests).
    #[cfg(test)]
    pub(crate) fn with_text_left(left: usize) -> Self {
        Budget { text: MAX_TEXT.saturating_sub(left), ..Budget::default() }
    }
    fn record(&mut self) -> Result<(), IoError> {
        self.records = self.records.saturating_add(1);
        if self.records > self.max_records {
            return Err(IoError::TooLarge("XLS: too many records".into()));
        }
        Ok(())
    }
    /// Counts a picture's bytes; `false` when they would pass `MAX_PICTURE_BYTES`.
    fn picture(&mut self, len: usize) -> bool {
        match self.pictures.checked_add(len) {
            Some(total) if total <= MAX_PICTURE_BYTES => {
                self.pictures = total;
                true
            }
            _ => false,
        }
    }
    fn cell(&mut self) -> Result<(), IoError> {
        self.cells += 1;
        if self.cells > MAX_CELLS {
            return Err(IoError::TooLarge("XLS: more than 4 million cells".into()));
        }
        Ok(())
    }
    /// Charges `len` bytes of text if they fit the allowance, leaving it untouched otherwise:
    /// for text the import can do without (expanded shared formulas keep their cached values).
    fn try_text(&mut self, len: usize) -> bool {
        let total = self.text.saturating_add(len);
        let fits = total <= MAX_TEXT;
        if fits {
            self.text = total;
        }
        fits
    }
    /// [`try_text`](Self::try_text) for a shared formula's text. The first refusal ends shared
    /// expansion for the whole workbook, so a shorter formula on a later sheet doesn't fit in the
    /// room the refused one left, and refused formulas aren't decoded again on each sheet.
    fn try_shared_text(&mut self, len: usize) -> bool {
        self.shared_text_spent = self.shared_text_spent || !self.try_text(len);
        !self.shared_text_spent
    }
    fn text(&mut self, len: usize) -> Result<(), IoError> {
        self.text = self.text.saturating_add(len);
        if self.text > MAX_TEXT {
            return Err(IoError::TooLarge("XLS: more than 256 MB of text".into()));
        }
        Ok(())
    }
}

struct BoundSheet {
    pos: usize,
    name: String,
    visibility: Visibility,
    kind: u8,
}

/// A defined name as stored in a BIFF8 Lbl or BIFF12 BrtName record.
pub(crate) struct Lbl {
    pub name: String,
    /// The built-in name's index in [`BUILTIN_NAMES`] (`Print_Area` is 6).
    pub builtin: Option<u8>,
    pub hidden: bool,
    pub macro_name: bool,
    /// A newer function's name (`_xlfn.…`, BIFF12 only): referenced by calls, never imported.
    pub future_function: bool,
    /// The zero-based sheet the name is local to; `None` for workbook scope.
    pub scope: Option<usize>,
    /// Set when the name can't be imported (`.xls` only): it keeps its slot so formulas that
    /// refer to names by index stay aligned, but it isn't added to the workbook.
    pub skip: bool,
    pub rgce: Vec<u8>,
    pub rgcb: Vec<u8>,
}

#[derive(Default)]
struct Globals {
    /// The BIFF version every substream's BOF must carry.
    version: u16,
    custom_views: Vec<(u32, String)>,
    tab_ids: Vec<u16>,
    /// BIFF5 tables (`None` for BIFF8).
    biff5: Option<Biff5>,
    sheets: Vec<BoundSheet>,
    date1904: bool,
    itab_cur: Option<u32>,
    view: WorkbookView,
    formats: HashMap<u16, String>,
    styles: styles::StyleRecords,
    sst: Vec<Arc<str>>,
    links: Links,
    /// Links to other workbooks (BIFF8 SupBook, XCT and CRN records).
    external: external_links::Reader,
    names: Vec<Lbl>,
    precision_as_displayed: bool,
    protection: protection::Protection,
    pivot_caches: pivot::Caches,
    /// The pictures of the MsoDrawingGroup records.
    pictures: drawing::BStore,
    code_name: Option<String>,
}

/// Reads an Excel 97-2003 workbook, with the default [`FunctionInfo`]. Inspect the report's
/// warnings for what was skipped.
pub fn read_xls(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    read_xls_budgeted(bytes, Budget::default(), FunctionInfo::default())
}

/// [`read_xls`] with the function library's facts, which decide where the legacy formulas get
/// Excel's implicit `@`.
pub fn read_xls_with(bytes: &[u8], functions: FunctionInfo) -> Result<(Workbook, ReadReport), IoError> {
    read_xls_budgeted(bytes, Budget::default(), functions)
}

/// [`read_xls_with`] with an explicit resource budget.
pub(crate) fn read_xls_budgeted(bytes: &[u8], budget: Budget, functions: FunctionInfo) -> Result<(Workbook, ReadReport), IoError> {
    read_xls_inner(bytes, budget, Lock::Refuse, functions).map_err(|e| match e {
        XlsOpenError::Io(e) => e,
        // `Lock::Refuse` turns these into ENCRYPTED already.
        XlsOpenError::PasswordRequired | XlsOpenError::Decrypt(_) => invalid(ENCRYPTED),
    })
}

/// How [`read_xls_inner`] treats a workbook encrypted with a password other than the default.
#[derive(Clone, Copy)]
enum Lock<'a> {
    /// Refuse it with [`ENCRYPTED`] ([`read_xls`]).
    Refuse,
    /// Decrypt it with this password, or ask for one when there is none.
    Password(Option<&'a str>),
}

fn read_xls_inner(bytes: &[u8], mut budget: Budget, lock: Lock<'_>, functions: FunctionInfo) -> Result<(Workbook, ReadReport), XlsOpenError> {
    let cf = Compound::open(bytes).map_err(invalid)?;
    // Excel 5.0/95 keeps the workbook in a `Book` stream.
    let (stream, book) = match cf.root_stream("Workbook").map_err(invalid)? {
        Some(s) => (s, false),
        None => match cf.root_stream("Book").map_err(invalid)? {
            Some(s) => (s, true),
            None => return Err(invalid("the file has no Workbook stream, so it is not an Excel workbook").into()),
        },
    };
    let encrypted = || invalid(if book { format!("Excel 5.0/95 (BIFF5) workbook: {ENCRYPTED}") } else { ENCRYPTED.to_string() });
    let mut report = ReadReport::default();
    let password = match lock {
        Lock::Refuse => None,
        Lock::Password(p) => p,
    };
    let (stream, decrypted) = match (unlock(stream, password), lock) {
        (Err(XlsOpenError::PasswordRequired | XlsOpenError::Decrypt(_)), Lock::Refuse) => return Err(encrypted().into()),
        (other, _) => other?,
    };
    let mut g = match globals(&stream, decrypted, &mut budget, &mut report) {
        Err(IoError::Format(msg)) if book && msg.contains(ENCRYPTED) => return Err(encrypted().into()),
        other => other?,
    };

    crate::read::repair_sheet_names(g.sheets.iter_mut().map(|s| &mut s.name), &mut report);
    // BIFF5 can also identify a local sheet by its original EXTERNSHEET name.
    if let Some(b5) = &mut g.biff5 {
        for entry in &mut b5.externs {
            if let ExternKind::Sheet(name) = &mut entry.kind
                && let Some(sheet) = g.links.sheets.iter().position(|s| s.eq_ignore_ascii_case(name)).and_then(|i| g.sheets.get(i))
            {
                *name = sheet.name.clone();
            }
        }
    }
    g.links.sheets = g.sheets.iter().map(|s| s.name.clone()).collect();

    let mut wb = Workbook::new();
    if let Some(custom) = doc_props::read(&cf, &mut wb.props, &mut report) {
        keep_custom_properties(&mut wb, custom);
    }
    wb.sheets.clear();
    std::mem::take(&mut g.external).finish(&mut g.links, &mut wb, &mut report);
    // Names into the linked workbooks decode only now that the links are kept.
    g.links.skipped = skipped_names(&g.names, &g.links, g.biff5.as_ref());
    wb.view = g.view;
    wb.code_name = g.code_name.clone();
    wb.protected_structure = g.protection.locked;
    wb.structure_password = g.protection.password.clone();
    wb.date_system = if g.date1904 { DateSystem::D1904 } else { DateSystem::D1900 };
    if let Some(theme) = &g.styles.theme {
        match theme.read() {
            Ok(Some(theme)) => wb.theme = theme,
            Ok(None) => {}
            Err(_) => report.warn("The .xls workbook theme could not be read; theme colours and fonts use the default Office theme."),
        }
    }
    // One model style per XF.
    let styles = styles::build(&g.styles, &g.formats, &mut wb);
    if g.styles.short {
        report.warn("Some cell formatting records were shorter than expected; the formatting present was kept.");
    }
    if styles::ext_ignored(&g.styles) {
        report.warn("Some extended cell colours (XFExt records) could not be read; the palette colours were kept.");
    }

    let mut connections = connections::Kept::default();
    let mut pivots = pivot::Loader::new(&cf, &g.pivot_caches, &g.formats, wb.date_system);
    // BoundSheet8 index → model sheet index.
    let mut model_index: Vec<Option<usize>> = Vec::with_capacity(g.sheets.len());
    let mut chart_sheets: Vec<chart::ChartSheet> = vec![];
    // BoundSheet8 position of each chart sheet, for the order of the kept sheets.
    let mut chart_bounds: Vec<usize> = vec![];
    let mut macro_sheets: Vec<macro_sheet::MacroSheet> = vec![];
    // Worksheets read so far; their shared formulas are attached once every sheet's mandatory
    // text is charged.
    let mut parsed = Vec::new();
    // Names of the readable sheets so far: `wb.sheets` is only filled once every sheet is parsed.
    let mut names: Vec<&str> = Vec::new();
    let mut cut_text = CutText::default();
    for (bound, bs) in g.sheets.iter().enumerate() {
        if bs.kind == 2 {
            match chart::chart_sheet(&stream, bs.pos, &mut budget, &g.links, g.biff5.as_ref(), &mut report)? {
                Some((chart, print)) => {
                    chart_sheets.push(chart::ChartSheet { name: bs.name.clone(), visibility: bs.visibility, before: parsed.len(), chart, print });
                    chart_bounds.push(bound);
                }
                None => report.warn(format!("Chart sheets that can't be read were skipped ('{}').", bs.name)),
            }
            model_index.push(None);
            continue;
        }
        if bs.kind == 1 {
            // An Excel 4.0 macro sheet: its cells are kept for a macro-enabled save, never evaluated.
            let read = match worksheet(&stream, bs, None, &g, &styles, &mut budget, &mut report, true)? {
                Some((st, filter)) => {
                    cut_text.sheet(&st, &bs.name);
                    Some(finish_sheet(st, filter, &g, &mut budget, &mut report)?)
                }
                None => None,
            };
            match read {
                Some((sheet, _, _, _)) => {
                    report.warn(macro_sheet::kept_warning(&bs.name));
                    macro_sheets.push(macro_sheet::MacroSheet {
                        name: bs.name.clone(),
                        visibility: bs.visibility,
                        before: parsed.len(),
                        bound,
                        intl: false,
                        sheet,
                    });
                }
                None => report.warn("Excel 4.0 macro sheets that can't be read were skipped."),
            }
            model_index.push(None);
            continue;
        }
        if bs.kind != 0 {
            report.warn("VBA modules and other non-worksheet sheets were skipped.");
            model_index.push(None);
            continue;
        }
        wb.check_sheet_name(&bs.name, None).map_err(invalid)?;
        if names.iter().any(|n| n.eq_ignore_ascii_case(&bs.name)) {
            return Err(invalid("That name is already taken. Try a different one.").into());
        }
        let filter = filter_database(&g, bound);
        let Some((st, filter)) = worksheet(&stream, bs, filter, &g, &styles, &mut budget, &mut report, false)? else {
            report.warn("Dialog sheets and unreadable sheets were skipped.");
            model_index.push(None);
            continue;
        };
        cut_text.sheet(&st, &bs.name);
        names.push(&bs.name);
        model_index.push(Some(parsed.len()));
        parsed.push((bs.visibility, st, filter));
    }
    cut_text.warn(&mut report);
    // Every sheet's mandatory text is charged by now; shared formulas take what is left.
    for (visibility, st, filter) in parsed {
        let (mut sheet, calc, views, queries) = finish_sheet(st, filter, &g, &mut budget, &mut report)?;
        sheet.visibility = visibility;
        sheet.pivots = pivots.build(views, &mut report);
        connections.sheet(queries, &mut sheet.preserved);
        // Every worksheet repeats the workbook's calculation settings; Excel applies the first.
        if wb.sheets.is_empty() {
            wb.calc = calc;
        }
        wb.sheets.push(Arc::new(sheet));
    }
    if wb.sheets.is_empty() {
        return Err(invalid("the workbook has no worksheets that can be read").into());
    }
    wb.calc.precision_as_displayed = g.precision_as_displayed;
    // Chart, picture and shape ids are unique in the workbook, as the .xlsx reader numbers
    // objects (0 is never an object's id).
    let mut next_id = 1u32;
    for sheet in wb.sheets.iter_mut().filter(|s| !s.charts.is_empty() || !s.images.is_empty() || !s.shapes.is_empty()) {
        let sheet = Arc::make_mut(sheet);
        for ch in sheet.charts.iter_mut() {
            ch.id = next_id;
            next_id = next_id.saturating_add(1);
        }
        for image in sheet.images.iter_mut() {
            image.id = next_id;
            next_id = next_id.saturating_add(1);
        }
        for shape in sheet.shapes.iter_mut() {
            shape.id = next_id;
            next_id = next_id.saturating_add(1);
        }
    }
    for cs in &mut chart_sheets {
        cs.chart.id = next_id;
        next_id = next_id.saturating_add(1);
    }
    chart::add_chart_sheets(&mut wb, chart_sheets);
    macro_sheet::add_macro_sheets(&mut wb, macro_sheets, &chart_bounds);
    defined_names(&g.names, &g.links, g.biff5.as_ref(), &model_index, &mut wb, &mut report);
    // Every .xls formula is a legacy (pre-dynamic-array) one: read with Excel's implicit `@`, like
    // an .xlsx formula without `cm`. Ctrl+Shift+Enter arrays are read as they are.
    let legacy = wb.sheets.iter().enumerate().map(|(si, s)| (si, legacy_cells(s))).collect();
    crate::array_formula::apply_legacy(&mut wb, legacy, functions);
    connections.finish(&mut wb, &mut report);
    if !g.custom_views.is_empty() {
        crate::binary_custom_views::pin_sheet_ids(&mut wb);
    }
    crate::binary_custom_views::finish_book(&g.custom_views, &mut wb.preserved, |id| {
        let index = if g.tab_ids.is_empty() {
            usize::try_from(id).ok().and_then(|i| i.checked_sub(1))
        } else {
            g.tab_ids.iter().position(|&tab| u32::from(tab) == id)
        };
        index.and_then(|i| model_index.get(i).copied().flatten()).unwrap_or(0)
    });
    tables::finish_workbook(&mut wb);
    let active = g
        .itab_cur
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| model_index.get(index).copied().flatten())
        .filter(|&index| wb.sheets.get(index).is_some_and(|s| s.visibility == Visibility::Visible))
        .or_else(|| wb.sheets.iter().position(|s| s.visibility == Visibility::Visible))
        .unwrap_or(0);
    wb.set_active_sheet(active);
    // Macros: the `_VBA_PROJECT_CUR` storage holds what an .xlsm's vbaProject.bin root holds
    // ([MS-XLS] 2.1.7, [MS-OVBA] 2.2.1), so it is repackaged as its own compound file and
    // carried through unchanged. Signatures at the file's root are left behind: they can't
    // stay valid.
    match cf.root_storage("_VBA_PROJECT_CUR").and_then(|tree| tree.map(|nodes| cfb::write_compound(&nodes)).transpose()) {
        Ok(Some(bin)) => {
            wb.vba_project = Some(Arc::new(bin));
            report.warn("macros (VBA project) were kept but not executed");
        }
        Ok(None) => {}
        Err(e) => report.warn(format!("macros (VBA project) could not be read and were dropped: {e}")),
    }
    // The overview comes first.
    report.warnings.insert(
        0,
        if g.biff5.is_some() {
            "Imported from an Excel 5.0/95 (.xls) workbook: values, formulas, cell formatting, column widths, row heights, defined names, charts, pictures, text boxes and drawn shapes were read. Comments and other features are not imported yet. Save as .xlsx; saving .xls is not supported."
        } else {
            "Imported from an Excel 97-2003 (.xls) workbook: values, formulas, cell formatting, merged cells, column widths, row heights, defined names, charts and pictures were read. Comments and other features are not imported yet. Save as .xlsx; saving .xls is not supported."
        }
        .into(),
    );
    Ok((wb, report))
}

/// Keeps the custom document properties as `docProps/custom.xml`, the part an .xlsx package holds
/// them in, so saving as .xlsx writes them.
fn keep_custom_properties(wb: &mut Workbook, xml: String) {
    use crate::passthrough::{CT_CUSTOM_PROPERTIES, REL_CUSTOM_PROPERTIES, keep_package_root_part};
    let rel = gridcraft_model::PreservedRel {
        id: "rIdCustomProperties".into(),
        rel_type: REL_CUSTOM_PROPERTIES.into(),
        target: "docProps/custom.xml".into(),
        external: false,
    };
    keep_package_root_part(wb, rel, CT_CUSTOM_PROPERTIES, xml.into_bytes());
}

/// The cells holding a formula that is not a Ctrl+Shift+Enter array.
fn legacy_cells(sheet: &Sheet) -> Vec<CellRef> {
    sheet.cells.iter().filter(|(_, c)| c.formula.as_ref().is_some_and(|f| f.array.is_none())).map(|(at, _)| at).collect()
}

fn bof(rec: &Record<'_>) -> Result<(u16, u16), IoError> {
    if rec.id != BOF {
        return Err(invalid("a substream does not start with a BOF record"));
    }
    let mut c = Cursor::new(&rec.parts);
    Ok((c.u16()?, c.u16()?))
}

/// Excel's built-in password for workbooks that are encrypted but open without a prompt.
const DEFAULT_PASSWORD: &str = "VelvetSweatshop";

/// Decrypts a stream whose globals begin with a FilePass ([MS-XLS] 2.4.117). Excel's default
/// password is tried first, as Excel opens those workbooks without asking; then `password`, or
/// [`XlsOpenError::PasswordRequired`] without one. A FilePass no password can open is refused
/// at once. Returns the stream and whether it was decrypted.
fn unlock(stream: Vec<u8>, password: Option<&str>) -> Result<(Vec<u8>, bool), XlsOpenError> {
    let header = |at: usize| -> Option<(u16, usize)> {
        let h = stream.get(at..at.checked_add(4)?)?;
        Some((u16::from_le_bytes([*h.first()?, *h.get(1)?]), usize::from(u16::from_le_bytes([*h.get(2)?, *h.get(3)?]))))
    };
    let Some((BOF, bof_len)) = header(0) else { return Ok((stream, false)) };
    let biff5 = header(4).map(|(v, _)| v) == Some(BIFF5);
    let mut at = 4 + bof_len;
    // The globals may begin with WriteProtect before the FilePass ([MS-XLS] 2.1.7.20.1).
    if let Some((0x0086, len)) = header(at) {
        at += 4 + len;
    }
    let Some((0x002F, len)) = header(at) else { return Ok((stream, false)) };
    let filepass = stream.get(at + 4..at + 4 + len).ok_or_else(|| DecryptError::Malformed("the FilePass record is cut short".into()))?;
    let wrong_password = match xls_crypto::decrypt(filepass, biff5, DEFAULT_PASSWORD, &stream) {
        Ok(plain) => return Ok((plain, true)),
        Err(e) => e,
    };
    if wrong_password != xls_crypto::XlsCryptoError::WrongPassword {
        return Err(unreadable_filepass(filepass, biff5).into());
    }
    let password = password.ok_or(XlsOpenError::PasswordRequired)?;
    match xls_crypto::decrypt(filepass, biff5, password, &stream) {
        Ok(plain) => Ok((plain, true)),
        Err(xls_crypto::XlsCryptoError::WrongPassword) => Err(DecryptError::WrongPassword.into()),
        Err(xls_crypto::XlsCryptoError::Unsupported) => Err(unreadable_filepass(filepass, biff5).into()),
    }
}

/// Why no password can open a FilePass: an encryption scheme we don't know, or a damaged
/// record for one we do.
fn unreadable_filepass(filepass: &[u8], biff5: bool) -> DecryptError {
    let word = |i: usize| Some(u16::from_le_bytes([*filepass.get(i)?, *filepass.get(i + 1)?]));
    let damaged = || DecryptError::Malformed("the FilePass record can't be read".into());
    if biff5 {
        return damaged();
    }
    match (word(0), word(2), word(4)) {
        (Some(0), ..) | (Some(1), Some(1), Some(1)) | (Some(1), Some(2..=4), Some(2)) => damaged(),
        (Some(1), Some(major), Some(minor)) => DecryptError::Unsupported(format!("RC4 version {major}.{minor}")),
        (Some(1), ..) | (None, ..) => damaged(),
        (Some(kind), ..) => DecryptError::Unsupported(format!("type {kind}")),
    }
}

fn globals(stream: &[u8], decrypted: bool, budget: &mut Budget, report: &mut ReadReport) -> Result<Globals, IoError> {
    let mut recs = Records::new(stream, 0);
    let first = recs.next_record()?.ok_or_else(|| invalid("empty Workbook stream"))?;
    let (vers, dt) = bof(&first)?;
    if vers != BIFF8 && vers != BIFF5 {
        return Err(invalid("unsupported BIFF version (only Excel 5.0/95 BIFF5 and Excel 97-2003 BIFF8 files are read)"));
    }
    if dt != 0x0005 {
        return Err(invalid("the Workbook stream does not start with the workbook globals"));
    }
    let mut g = Globals { version: vers, ..Globals::default() };
    if vers == BIFF5 {
        g.biff5 = Some(Biff5 { codepage: 1252, externs: vec![] });
    }
    let mut window_seen = false;
    let mut filepass_seen = false;
    let mut offsets = std::collections::HashSet::new();
    let mut drawing_group = vec![];
    // Whether the previous record was the Theme record or one of its ContinueFrt12 records.
    let mut in_theme = false;
    while let Some(rec) = recs.next_record()? {
        budget.record()?;
        let mut c = Cursor::new(&rec.parts);
        let after_theme = std::mem::replace(&mut in_theme, matches!(rec.id, 0x0896 | 0x087F));
        if let Some(b5) = g.biff5.as_mut()
            && biff5_global(&rec, &mut c, b5, &mut g.formats, &mut g.styles, budget, report)?
        {
            continue;
        }
        match rec.id {
            EOF => break,
            0x013D if g.biff5.is_none() => {
                while c.remaining() >= 2 && g.tab_ids.len() < 65535 {
                    g.tab_ids.push(c.u16()?);
                }
            }
            0x01A9 if g.biff5.is_none() => {
                let data: Vec<u8> = rec.parts.iter().flat_map(|p| p.iter().copied()).take(2048).collect();
                if g.custom_views.len() < crate::binary_custom_views::LIMIT
                    && let Some(view) = crate::binary_custom_views::book(&data, false)
                {
                    g.custom_views.push(view);
                } else {
                    crate::binary_custom_views::warn(report);
                }
            }
            0x0012 => protection::protect(&mut c, &mut g.protection, report),
            0x0013 => protection::password(&mut c, &mut g.protection, report),
            // The FilePass `unlock` already used; any other one is refused.
            0x002F if decrypted && !filepass_seen => filepass_seen = true,
            0x002F => return Err(invalid(ENCRYPTED)),
            0x003D if !window_seen => {
                window_seen = true;
                // [MS-XLS] Window1: the first window's selected tab is at byte 10.
                if c.remaining() == 18 {
                    g.itab_cur = c.skip(10).and_then(|()| c.u16()).ok().map(u32::from);
                }
                // Window1 display flags are at byte 8; wTabRatio is at byte 16.
                let mut view = Cursor::new(&rec.parts);
                if let Ok(flags) = view.skip(8).and_then(|()| view.u16()) {
                    g.view.show_horizontal_scroll = flags & 0x0008 != 0;
                    g.view.show_vertical_scroll = flags & 0x0010 != 0;
                    g.view.show_sheet_tabs = flags & 0x0020 != 0;
                    if let Ok(ratio) = view.skip(6).and_then(|()| view.u16()) {
                        g.view.tab_ratio = u32::from(ratio).min(1000);
                    }
                }
            }
            0x0085 => {
                if g.sheets.len() >= MAX_SHEETS {
                    return Err(IoError::TooLarge("XLS: too many sheets".into()));
                }
                let pos = c.u32()? as usize;
                if !offsets.insert(pos) {
                    return Err(invalid("two sheets point at the same substream"));
                }
                let state = c.u8()?;
                let kind = c.u8()?;
                let name = match &g.biff5 {
                    // BIFF5: a 1-byte count and 8-bit characters.
                    Some(b5) => {
                        let cch = usize::from(c.u8()?);
                        b5.text(&c.bytes(cch)?)
                    }
                    None => c.short_string()?,
                };
                let visibility = match state & 3 {
                    0 => Visibility::Visible,
                    1 => Visibility::Hidden,
                    _ => Visibility::VeryHidden,
                };
                g.links.sheets.push(name.clone());
                g.sheets.push(BoundSheet { pos, name, visibility, kind });
            }
            0x0022 => g.date1904 = c.u16()? == 1,
            CODE_NAME => g.code_name = code_name(&mut c),
            0x000E => g.precision_as_displayed = c.u16()? == 0,
            0x041E => {
                let ifmt = c.u16()?;
                let cch = usize::from(c.u16()?);
                let high = c.u8()? & 1 != 0;
                // Some writers count one character more than the record holds; Excel opens
                // those files, so keep the characters that are there.
                let fit = c.remaining() / if high { 2 } else { 1 };
                if cch > fit {
                    report.warn("Some number formats were shorter than their declared length; the characters present were kept.");
                }
                let code = c.chars(cch.min(fit), high)?;
                budget.text(code.len())?;
                g.formats.insert(ifmt, code);
            }
            // MsoDrawingGroup: one OfficeArtDggContainer, split over records and their Continues.
            0x00EB => {
                for part in &rec.parts {
                    if drawing_group.len().saturating_add(part.len()) <= MAX_TEXT {
                        drawing_group.extend_from_slice(part);
                    }
                }
            }
            0x00E0 => g.styles.xf(&mut c, MAX_XFS),
            0x0031 => g.styles.font(&mut c),
            0x0092 => g.styles.palette(&mut c),
            0x0293 => g.styles.style(&mut c),
            0x087D => g.styles.xf_ext(&mut c),
            0x0896 => g.styles.theme(&mut c),
            0x087F if after_theme => g.styles.theme_continue(&mut c),
            0x087F => in_theme = false,
            0x00FC => g.sst = shared_strings(&mut c, budget)?,
            0x01AE => {
                let ctab = c.u16()?;
                let cch = c.u16()?;
                let (internal, addin) = (cch == 0x0401, cch == 0x3A01);
                g.links.books.push(SupBook { internal, addin, ..SupBook::default() });
                if internal || addin {
                    g.external.other_supbook();
                } else {
                    g.external.supbook(&mut c, g.links.books.len().saturating_sub(1), ctab, cch, report);
                }
            }
            0x0059 if g.biff5.is_none() => g.external.xct(&mut c, report),
            0x005A if g.biff5.is_none() => g.external.crn(&mut c, report),
            // ExternName of a linked workbook: kept with the link.
            0x0023 if g.external.extern_name(&mut c) => {}
            0x0023 => {
                // ExternName: flags, then an AddinUdf or ExternDocName body; both start with
                // four bytes before the name.
                c.u16()?;
                c.skip(4)?;
                let name = c.short_string()?;
                if let Some(book) = g.links.books.last_mut()
                    && book.names.len() < MAX_NAMES
                {
                    book.names.push(name);
                }
            }
            0x0017 => {
                let n = usize::from(c.u16()?);
                for _ in 0..n.min(c.remaining() / 6) {
                    let book = c.u16()?;
                    let first = c.u16()? as i16;
                    let last = c.u16()? as i16;
                    g.links.xti.push(Xti { book: u32::from(book), first: i32::from(first), last: i32::from(last) });
                }
            }
            0x00D5 => g.pivot_caches.stream_id(&mut c),
            0x00E3 => g.pivot_caches.source_type(&mut c),
            0x0051 => g.pivot_caches.dcon_ref(&mut c),
            0x0052 => g.pivot_caches.dcon_name(&mut c),
            0x01B5 => g.pivot_caches.dcon_bin(),
            0x0018 => {
                if g.names.len() >= MAX_NAMES {
                    continue;
                }
                let lbl = match &g.biff5 {
                    Some(b5) => label_biff5(&mut c, b5)?,
                    None => label(&mut c)?,
                };
                g.names.push(lbl);
            }
            _ => {}
        }
    }
    g.pictures = drawing::bstore(&drawing_group, budget, report);
    rename_invalid_names(&mut g, report);
    // The same test `defined_names` applies, before any cell formula is decoded. A name that was
    // skipped (unnameable) is marked the same way, so formulas using it keep their saved result.
    g.links.skipped = skipped_names(&g.names, &g.links, g.biff5.as_ref());
    Ok(g)
}

/// Which names `defined_names` skips (unnameable, or a formula that can't be decoded).
fn skipped_names(names: &[Lbl], links: &Links, biff5: Option<&Biff5>) -> Vec<bool> {
    names.iter().map(|l| l.skip || (!l.rgce.is_empty() && decode_name(&l.rgce, &l.rgcb, links, biff5).is_err())).collect()
}

/// Built-in defined names ([MS-XLS] 2.4.150, Lbl.Name).
pub(crate) const BUILTIN_NAMES: [&str; 14] = [
    "Consolidate_Area",
    "Auto_Open",
    "Auto_Close",
    "Extract",
    "Database",
    "Criteria",
    "Print_Area",
    "Print_Titles",
    "Recorder",
    "Data_Form",
    "Auto_Activate",
    "Auto_Deactivate",
    "Sheet_Title",
    "_FilterDatabase",
];

/// The globals records whose BIFF5 layout differs from BIFF8; returns whether `rec` was one.
fn biff5_global(
    rec: &Record<'_>,
    c: &mut Cursor<'_, '_>,
    b5: &mut Biff5,
    formats: &mut HashMap<u16, String>,
    styles: &mut styles::StyleRecords,
    budget: &mut Budget,
    report: &mut ReadReport,
) -> Result<bool, IoError> {
    match rec.id {
        // CodePage: the code page of every 8-bit string.
        0x0042 => {
            let cp = c.u16()?;
            if crate::csv::codepage_supported(cp) {
                b5.codepage = cp;
            } else {
                b5.codepage = 1252;
                report.warn(format!("The workbook's text uses code page {cp}, which isn't supported; it was read as Windows-1252."));
            }
        }
        // Format: index, 1-byte count, 8-bit characters.
        0x041E => {
            let ifmt = c.u16()?;
            let cch = usize::from(c.u8()?);
            let fit = c.remaining();
            if cch > fit {
                report.warn("Some number formats were shorter than their declared length; the characters present were kept.");
            }
            let code = b5.text(&c.bytes(cch.min(fit))?);
            budget.text(code.len())?;
            formats.insert(ifmt, code);
        }
        // ExternSheet: one entry per record, a count then the encoded target.
        0x0017 => {
            if b5.externs.len() >= MAX_SHEETS {
                return Ok(true);
            }
            let cch = usize::from(c.u8()?);
            let raw = c.bytes(cch.min(c.remaining()))?;
            let kind = b5.extern_kind(&raw);
            b5.externs.push(Extern { kind, names: vec![] });
        }
        // ExternName: flags, 4 reserved bytes, then a 1-byte count and 8-bit characters.
        0x0023 => {
            c.u16()?;
            c.skip(4)?;
            let cch = usize::from(c.u8()?);
            let name = b5.text(&c.bytes(cch)?);
            if let Some(e) = b5.externs.last_mut()
                && e.names.len() < MAX_NAMES
            {
                e.names.push(name);
            }
        }
        // XF: 16 bytes with the alignment, fill and borders packed differently from BIFF8.
        0x00E0 => styles.xf_biff5(c, MAX_XFS),
        // Font: the name is a 1-byte count and 8-bit characters.
        0x0031 => styles.font_biff5(c, |raw| b5.text(raw)),
        0x0293 => {}
        // No SST or SupBook in BIFF5.
        0x00FC | 0x01AE => {}
        _ => return Ok(false),
    }
    Ok(true)
}

fn label(c: &mut Cursor<'_, '_>) -> Result<Lbl, IoError> {
    let flags = c.u16()?;
    let _key = c.u8()?;
    let cch = usize::from(c.u8()?);
    let cce = usize::from(c.u16()?);
    c.skip(2)?;
    let itab = c.u16()?;
    c.skip(4)?;
    let high = c.u8()? & 1 != 0;
    let raw = c.chars(cch, high)?;
    Ok(lbl(flags, raw, itab, cce, &c.rest(), 0))
}

/// A BIFF5 NAME record: as BIFF8's Lbl but with 8-bit text, and the menu, description, help
/// and status texts (whose lengths the header gives) after the formula.
fn label_biff5(c: &mut Cursor<'_, '_>, b5: &Biff5) -> Result<Lbl, IoError> {
    let flags = c.u16()?;
    let _key = c.u8()?;
    let cch = usize::from(c.u8()?);
    let cce = usize::from(c.u16()?);
    c.skip(2)?;
    let itab = c.u16()?;
    let mut texts = 0usize;
    for _ in 0..4 {
        texts += usize::from(c.u8()?);
    }
    let raw = b5.text(&c.bytes(cch)?);
    Ok(lbl(flags, raw, itab, cce, &c.rest(), texts))
}

/// A defined name from its header fields; `rest` holds the formula, its extra data, and
/// `trailing` bytes of other texts.
fn lbl(flags: u16, raw: String, itab: u16, cce: usize, rest: &[u8], trailing: usize) -> Lbl {
    let builtin = flags & 0x0020 != 0;
    let (name, builtin) = if builtin {
        let code = raw.chars().next().map_or(0, |ch| ch as u32);
        let code = u8::try_from(code).unwrap_or(u8::MAX);
        (BUILTIN_NAMES.get(usize::from(code)).map_or_else(|| raw.clone(), |n| (*n).to_string()), Some(code))
    } else {
        (raw, None)
    };
    let rgce = rest.get(..cce).unwrap_or(rest).to_vec();
    let extra = rest.get(cce..).unwrap_or(&[]);
    let rgcb = extra.get(..extra.len().saturating_sub(trailing)).unwrap_or(&[]).to_vec();
    let scope = usize::from(itab).checked_sub(1);
    Lbl { name, builtin, hidden: flags & 1 != 0, macro_name: flags & 0x000A != 0, future_function: false, scope, skip: false, rgce, rgcb }
}

/// Whether `n` is a legal defined name in an `.xlsx` workbook: the same rule as the engine's
/// `formulas.defineName` (first character a letter, `_` or `\`; other characters letters, digits,
/// `_`, `.`, `\` or `?`; not a cell address or R1C1-like; at most 255 characters).
fn valid_name(n: &str) -> bool {
    let mut chars = n.chars();
    let Some(first) = chars.next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '\\') {
        return false;
    }
    if n.chars().count() > 255 || CellRef::parse(n).is_some() || is_r1c1(n) {
        return false;
    }
    n.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?'))
}

/// Looks like an R1C1 reference (`R`, `C`, `RC`, `R2`, `C3`, `R1C1`), which Excel doesn't accept
/// as a name.
fn is_r1c1(n: &str) -> bool {
    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let mut rest = n;
    if let Some(r) = rest.strip_prefix(['R', 'r']) {
        rest = r.get(digits(r)..).unwrap_or("");
    }
    if let Some(c) = rest.strip_prefix(['C', 'c']) {
        rest = c.get(digits(c)..).unwrap_or("");
    }
    rest.is_empty() && !n.is_empty()
}

/// Renames the defined names that aren't legal in an `.xlsx` workbook (a space, a leading `?` or
/// digit, a `-`): other office suites write such NAME records, and Excel refuses the saved file
/// until it repairs it. Every character that can't be in a name becomes `_`, a bad first character
/// gets a `_` in front, and a clash with another name of the same scope gets more `_` in front.
/// Valid names never move; formulas refer to names by index, so they follow. A name that is still
/// invalid (empty, or over 255 characters) is skipped.
fn rename_invalid_names(g: &mut Globals, report: &mut ReadReport) {
    let mut renamed = false;
    // The names that are valid as they are, per scope: they keep theirs, so a renamed name moves
    // out of their way (Excel compares names case-insensitively).
    let mut taken: HashMap<Option<usize>, HashSet<String>> = HashMap::new();
    for lbl in g.names.iter() {
        if lbl.builtin.is_none() && !lbl.macro_name && valid_name(&lbl.name) {
            taken.entry(lbl.scope).or_default().insert(lbl.name.to_ascii_lowercase());
        }
    }
    for lbl in g.names.iter_mut() {
        if lbl.builtin.is_some() || lbl.macro_name || valid_name(&lbl.name) {
            continue;
        }
        // An empty name has nothing to rename.
        if lbl.name.is_empty() {
            lbl.skip = true;
            report.warn("A defined name isn't valid in .xlsx workbooks and was skipped.");
            continue;
        }
        let mut name: String = lbl.name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?') { c } else { '_' }).collect();
        let first = name.chars().next();
        if !matches!(first, Some(c) if c.is_alphabetic() || c == '_' || c == '\\') {
            name.insert(0, '_');
        }
        if CellRef::parse(&name).is_some() || is_r1c1(&name) {
            name.insert(0, '_');
        }
        let used = taken.entry(lbl.scope).or_default();
        while used.contains(&name.to_ascii_lowercase()) {
            name.insert(0, '_');
        }
        if valid_name(&name) {
            used.insert(name.to_ascii_lowercase());
            lbl.name = name;
            renamed = true;
        } else {
            lbl.skip = true;
            report.warn("A defined name isn't valid in .xlsx workbooks and was skipped.");
        }
    }
    if renamed {
        report.warn("Some defined names aren't valid in .xlsx workbooks and were renamed (spaces and other characters became underscores).");
    }
    g.links.names = g.names.iter().map(|l| l.name.clone()).collect();
}

fn shared_strings(c: &mut Cursor<'_, '_>, budget: &mut Budget) -> Result<Vec<Arc<str>>, IoError> {
    let _total = c.u32()?;
    let unique = c.u32()? as usize;
    if unique > MAX_STRINGS {
        return Err(IoError::TooLarge("XLS: too many shared strings".into()));
    }
    // Each string takes at least three bytes: the declared count can't exceed what's there.
    let mut out = Vec::with_capacity(unique.min(c.remaining() / 3));
    for _ in 0..unique {
        if c.remaining() == 0 {
            break;
        }
        let s = c.rich_string()?;
        budget.text(s.len())?;
        out.push(Arc::from(s));
    }
    Ok(out)
}

/// A formula cell waiting for its shared or array formula (which follows it in the file).
pub(crate) struct Pending {
    pub pos: CellRef,
    pub anchor: CellRef,
}

/// A parsed formula: tokens and extra data.
pub(crate) struct Parsed {
    pub rgce: Vec<u8>,
    pub rgcb: Vec<u8>,
}

/// A worksheet reader whose formula cells may refer to shared and array formulas (`.xls` and
/// `.xlsb`), as used by [`resolve_pending`] and [`blank_array_followers`].
pub(crate) trait FormulaGroups {
    fn sheet(&self) -> &Sheet;
    fn sheet_mut(&mut self) -> &mut Sheet;
    /// Decodes `p` at `pos` (a shared formula's tokens when `shared`) and attaches it, as an
    /// array formula over `array` if given, or records why it can't be.
    fn attach_group(&mut self, pos: CellRef, p: &Parsed, shared: bool, array: Option<RangeRef>);
    /// Records why a formula was kept as its value.
    fn refuse(&mut self, why: &'static str);
}

impl FormulaGroups for SheetState<'_> {
    fn sheet(&self) -> &Sheet {
        &self.sheet
    }
    fn sheet_mut(&mut self) -> &mut Sheet {
        &mut self.sheet
    }
    fn attach_group(&mut self, pos: CellRef, p: &Parsed, shared: bool, array: Option<RangeRef>) {
        self.attach(pos, p, if shared { Mode::Shared } else { Mode::Cell }, array);
    }
    fn refuse(&mut self, why: &'static str) {
        self.unsupported(why);
    }
}

fn parsed(c: &mut Cursor<'_, '_>) -> Result<Parsed, IoError> {
    let cce = usize::from(c.u16()?);
    let rest = c.rest();
    if cce > rest.len() {
        return Err(invalid("formula runs past the end of its record"));
    }
    Ok(Parsed { rgce: rest.get(..cce).unwrap_or(&[]).to_vec(), rgcb: rest.get(cce..).unwrap_or(&[]).to_vec() })
}

struct SheetState<'g> {
    macro_sheet: bool,
    protection: protection::Protection,
    g: &'g Globals,
    styles: &'g [StyleId],
    sheet: Sheet,
    pending: Vec<Pending>,
    shared: HashMap<CellRef, Parsed>,
    arrays: HashMap<CellRef, (RangeRef, Parsed)>,
    last_formula: Option<CellRef>,
    /// The cell waiting for its String record, with its style: a blank cell isn't stored, so a
    /// formula kept as its value may have no cell to receive the text.
    pending_string: Option<(CellRef, StyleId)>,
    dropped: usize,
    unsupported: Vec<&'static str>,
    calc: CalcSettings,
    /// Conditional formats read so far.
    cond: cond_format::CondState,
    page: page_setup::PageState,
    /// The AutoFilter records read so far: built when the substream ends, with the range the
    /// `_FilterDatabase` name gives.
    autofilter: autofilter::State,
    tables: tables::SheetTables,
    /// Embedded charts read so far, placed on the sheet when the substream ends.
    charts: chart::Embedded,
    /// The PivotTable views read so far: built into model PivotTables once every sheet is read.
    pivots: pivot::Views,
    /// The query tables read so far, as their records: translated once every sheet is read.
    queries: connections::Blocks,
    /// The SELECTION records read so far: pane and active cell.
    selections: Vec<(u8, CellRef)>,
    /// A SELECTION record was cut short.
    bad_selection: bool,
    /// The result ranges of the What-If data tables read so far (a repeated one is skipped).
    data_table_ranges: HashSet<RangeRef>,
    /// A data table result cell (PtgTbl) was read, and a TABLE record was read: a result cell
    /// whose sheet has no TABLE record lost its table, which is reported.
    data_table_cell: bool,
    data_table_record: bool,
    /// How many cells' text was cut to [`CELL_TEXT_MAX`], and the first of them.
    cut_text: usize,
    first_cut: Option<CellRef>,
}

/// The most text one cell holds, in UTF-16 code units (the way `LEN` counts). A BIFF8 string may
/// declare up to 65,535 characters, so a file can carry more than a cell may hold.
const CELL_TEXT_MAX: usize = 32_767;

/// `s` cut to its first [`CELL_TEXT_MAX`] UTF-16 code units, or `None` when it fits. A character
/// that would be split (a surrogate pair straddling the limit) is dropped whole.
fn cut_cell_text(s: &str) -> Option<&str> {
    // A UTF-8 byte never stands for more than one UTF-16 unit.
    if s.len() <= CELL_TEXT_MAX {
        return None;
    }
    let mut units = 0usize;
    for (i, ch) in s.char_indices() {
        units += ch.len_utf16();
        if units > CELL_TEXT_MAX {
            return s.get(..i);
        }
    }
    None
}

/// A cell value with its text cut to fit one cell, and whether it was cut.
fn fit_cell_value(value: Value) -> (Value, bool) {
    let cut = match &value {
        Value::Text(t) => cut_cell_text(t).map(Arc::<str>::from),
        _ => None,
    };
    match cut {
        Some(t) => (Value::Text(t), true),
        None => (value, false),
    }
}

/// Cells whose text was cut to fit, across the workbook, and the first of them as `Sheet!A1`.
#[derive(Default)]
struct CutText {
    cells: usize,
    first: Option<String>,
}

impl CutText {
    fn sheet(&mut self, st: &SheetState<'_>, name: &str) {
        self.cells += st.cut_text;
        if let (None, Some(at)) = (&self.first, st.first_cut) {
            self.first = Some(format!("{}!{}", gridcraft_core::quote_sheet(name), at.a1()));
        }
    }

    /// One warning when any cell was cut.
    fn warn(self, report: &mut ReadReport) {
        let Some(first) = self.first else { return };
        report.warn(if self.cells == 1 {
            format!("1 cell held more text than Excel's 32,767 characters and was cut to fit (first: {first}).")
        } else {
            format!("{} cells held more text than Excel's 32,767 characters and were cut to fit (first: {first}).", self.cells)
        });
    }
}

impl SheetState<'_> {
    /// Stores a cell's text value, cut to fit one cell.
    fn put_text(&mut self, budget: &mut Budget, row: u16, col: u16, ixfe: u16, value: Value) -> Result<(), IoError> {
        let (value, cut) = fit_cell_value(value);
        if let Some(pos) = self.put(budget, row, col, ixfe, value)?
            && cut
        {
            self.note_cut(pos);
        }
        Ok(())
    }

    /// Counts a cell whose text was cut to fit.
    fn note_cut(&mut self, pos: CellRef) {
        self.cut_text += 1;
        self.first_cut.get_or_insert(pos);
    }

    /// Records why a formula was kept as its value (each reason once).
    fn unsupported(&mut self, why: &'static str) {
        if !self.unsupported.contains(&why) {
            self.unsupported.push(why);
        }
    }

    fn style(&self, ixfe: u16) -> StyleId {
        self.styles.get(usize::from(ixfe)).copied().unwrap_or(StyleId::DEFAULT)
    }

    fn pos(&mut self, row: u16, col: u16) -> Option<CellRef> {
        let (r, c) = (u32::from(row), u32::from(col));
        if r >= MAX_ROWS || c >= MAX_COLS.min(256) {
            self.dropped += 1;
            return None;
        }
        Some(CellRef::new(r, c))
    }

    fn put(&mut self, budget: &mut Budget, row: u16, col: u16, ixfe: u16, value: Value) -> Result<Option<CellRef>, IoError> {
        let Some(pos) = self.pos(row, col) else { return Ok(None) };
        let style = self.style(ixfe);
        if value.is_empty() && style == StyleId::DEFAULT {
            return Ok(Some(pos));
        }
        budget.cell()?;
        self.sheet.cells.set(pos, Cell { value, formula: None, style, py: None });
        Ok(Some(pos))
    }

    /// Decodes a formula and attaches it to the cell at `pos`, or records why it can't be.
    fn attach(&mut self, pos: CellRef, p: &Parsed, mode: Mode, array: Option<RangeRef>) {
        match decode_cell(&p.rgce, &p.rgcb, pos, mode, &self.g.links, self.g.biff5.as_ref(), self.macro_sheet) {
            Ok(e) => {
                // References into other workbooks keep the `.xlsx` text (`[1]Data!B2`) and their
                // saved value, as the .xlsx reader keeps them, even where that text doesn't parse.
                let (text, external) = external_sheet_text(&gridcraft_formula::print(&e));
                let mut f = Formula::new(&text);
                if f.expr.is_none() && !external {
                    self.unsupported("syntax that can't be parsed yet");
                    return;
                }
                // A one-cell Ctrl+Shift+Enter array keeps its range, as when an .xlsx is read:
                // it evaluates as an array, not with implicit intersection.
                f.array = array;
                if let Some(cell) = self.sheet.cells.get_mut(pos) {
                    cell.formula = Some(Arc::new(f));
                } else {
                    self.sheet.cells.set(pos, Cell { value: Value::Empty, formula: Some(Arc::new(f)), style: StyleId::DEFAULT, py: None });
                }
            }
            Err(Unsupported(why)) => self.unsupported(why),
        }
    }
}

/// RK number ([MS-XLS] 2.5.217): a 30-bit integer or the top of an IEEE double, maybe ×100.
fn rk(v: u32) -> Value {
    let n = if v & 2 != 0 { f64::from((v as i32) >> 2) } else { f64::from_bits(u64::from(v & !3) << 32) };
    Value::number(if v & 1 != 0 { n / 100.0 } else { n })
}

/// Attaches every pending formula to its anchor's template, returning how many shared
/// templates were expanded. An anchor whose template can't be imported is refused once:
/// its followers re-decode the same template for nothing, so they are skipped instead (they
/// keep their cached values).
pub(crate) fn resolve_pending(
    st: &mut impl FormulaGroups,
    budget: &mut Budget,
    arrays: &HashMap<CellRef, (RangeRef, Parsed)>,
    shared: &HashMap<CellRef, Parsed>,
    pending: &[Pending],
) -> usize {
    let mut failed: HashSet<CellRef> = HashSet::new();
    let mut shared_attaches = 0usize;
    for p in pending {
        if let Some((range, f)) = arrays.get(&p.anchor) {
            if p.pos == p.anchor {
                st.attach_group(p.pos, f, false, Some(*range));
            }
        } else if let Some(f) = shared.get(&p.anchor) {
            if budget.shared_text_spent {
                st.refuse("more shared-formula text than the import allows");
                continue;
            }
            if failed.contains(&p.anchor) || shared_attaches >= MAX_SHARED_ATTACHES {
                continue;
            }
            shared_attaches += 1;
            st.attach_group(p.pos, f, true, None);
            let len = st.sheet().cell(p.pos).and_then(|c| c.formula.as_ref()).map(|f| f.text.len());
            // Every follower keeps its own text: charge it to the workbook's text allowance, and
            // past it drop the formula again so the cell keeps its cached value.
            if let Some(len) = len
                && !budget.try_shared_text(len)
            {
                if let Some(cell) = st.sheet_mut().cells.get_mut(p.pos) {
                    cell.formula = None;
                }
                st.refuse("more shared-formula text than the import allows");
                continue;
            }
            let imported = st.sheet().cell(p.pos).and_then(|c| c.formula.as_ref()).is_some_and(|f| f.text.chars().count() <= MAX_SHARED_TEMPLATE);
            if !imported {
                failed.insert(p.anchor);
            }
        } else {
            st.refuse("a shared formula that is missing from the file");
        }
    }
    if shared_attaches >= MAX_SHARED_ATTACHES {
        st.refuse("more shared formulas than the expansion cap; some formulas kept their values");
    }
    shared_attaches
}

/// The other cells of a legacy array formula hold only cached results: dropped, as when an
/// .xlsx is read, so that the formula fills its range on recalculation. If the formula itself
/// couldn't be imported, they keep their values.
pub(crate) fn blank_array_followers(st: &mut impl FormulaGroups, pending: &[Pending]) {
    for p in pending {
        let filled = st.sheet().cell(p.anchor).and_then(|c| c.formula.as_ref()).and_then(|f| f.array).is_some_and(|r| r.contains(p.pos));
        if p.pos != p.anchor && filled {
            let style = st.sheet().style_id(p.pos);
            st.sheet_mut().cells.set(p.pos, Cell { value: Value::Empty, formula: None, style, py: None });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn worksheet<'g>(
    stream: &[u8],
    bs: &BoundSheet,
    filter: Option<RangeRef>,
    g: &'g Globals,
    styles: &'g [StyleId],
    budget: &mut Budget,
    report: &mut ReadReport,
    macro_sheet: bool,
) -> Result<Option<(SheetState<'g>, Option<RangeRef>)>, IoError> {
    if bs.pos >= stream.len() {
        return Err(invalid(format!("sheet '{}' points outside the Workbook stream", bs.name)));
    }
    let mut recs = Records::new(stream, bs.pos);
    let first = recs.next_record()?.ok_or_else(|| invalid("missing sheet substream"))?;
    let (vers, dt) = bof(&first)?;
    // The globals select record layouts, including when a BIFF5 writer stamps a BIFF8 sheet BOF.
    let compatible_version = vers == g.version || (g.version == BIFF5 && vers == BIFF8);
    if !compatible_version || dt != if macro_sheet { 0x0040 } else { 0x0010 } {
        return Ok(None);
    }
    let mut st = SheetState {
        macro_sheet,
        protection: protection::Protection::default(),
        g,
        styles,
        sheet: Sheet::new(&bs.name),
        pending: vec![],
        shared: HashMap::new(),
        arrays: HashMap::new(),
        last_formula: None,
        pending_string: None,
        dropped: 0,
        unsupported: vec![],
        calc: CalcSettings::default(),
        cond: cond_format::CondState::default(),
        page: page_setup::PageState::default(),
        autofilter: autofilter::State::default(),
        tables: tables::SheetTables::default(),
        charts: chart::Embedded::new(g.styles.color_table(), g.styles.drawing_fonts()),
        pivots: pivot::Views::default(),
        queries: connections::Blocks::default(),
        selections: vec![],
        bad_selection: false,
        data_table_ranges: HashSet::new(),
        data_table_cell: false,
        data_table_record: false,
        cut_text: 0,
        first_cut: None,
    };
    let mut custom_views = crate::binary_custom_views::SheetViews::default();
    let mut depth = 1usize;
    while let Some(rec) = recs.next_record()? {
        budget.record()?;
        match rec.id {
            BOF if depth == 1 && chart::is_chart_bof(&rec) => {
                // An embedded chart's substream, read to its EOF.
                if !st.charts.chart(&mut recs, budget, &g.links, g.biff5.as_ref(), report)? {
                    break;
                }
                continue;
            }
            BOF => {
                depth += 1;
                continue;
            }
            EOF => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            _ if depth > 1 => continue,
            WSBOOL if rec.parts.first().and_then(|p| p.first()).is_some_and(|b| b & WSBOOL_DIALOG != 0) => return Ok(None),
            WSBOOL => {
                if let Some(&[lo, hi]) = rec.parts.first().and_then(|p| p.get(..2)) {
                    let flags = u16::from_le_bytes([lo, hi]);
                    st.page.fit_to_page = flags & WSBOOL_FIT_TO_PAGE != 0;
                    st.sheet.summary_below = flags & WSBOOL_ROW_SUMS_BELOW != 0;
                    st.sheet.summary_right = flags & WSBOOL_COL_SUMS_RIGHT != 0;
                    st.sheet.outline_apply_styles = flags & WSBOOL_APPLY_STYLES != 0;
                }
            }
            _ => {}
        }
        if g.biff5.is_none() {
            let data = rec.parts.first().copied().unwrap_or_default();
            if custom_views.record(rec.id, data, false, report) {
                continue;
            }
        }
        sheet_record(&rec, &mut st, budget, report)?;
    }
    custom_views.finish(&mut st.sheet.preserved, report);
    st.sheet.protection = st.protection.sheet();
    Ok(Some((st, filter)))
}

/// Attaches a sheet's shared and array formulas (their definitions follow the first cell that
/// uses them) and finishes the sheet. Run for every worksheet only after all sheets' mandatory
/// text is charged, so the optional shared-formula text can't use up the allowance a later
/// sheet's strings need.
fn finish_sheet(
    mut st: SheetState<'_>,
    filter: Option<RangeRef>,
    g: &Globals,
    budget: &mut Budget,
    report: &mut ReadReport,
) -> Result<(Sheet, CalcSettings, pivot::Views, connections::Blocks), IoError> {
    let macro_sheet = st.macro_sheet;
    let (arrays, shared) = (std::mem::take(&mut st.arrays), std::mem::take(&mut st.shared));
    let pending = std::mem::take(&mut st.pending);
    resolve_pending(&mut st, budget, &arrays, &shared, &pending);
    blank_array_followers(&mut st, &pending);
    std::mem::take(&mut st.tables).finish(&mut st.sheet, budget, report)?;
    st.page.finish(&mut st.sheet.print);
    st.cond.finish(report);
    // The top-left pane's selection (the only one without frozen panes), else the last one.
    let chosen = st.selections.iter().find(|(pnn, _)| *pnn == 3).or(st.selections.last());
    if let Some((_, at)) = chosen {
        st.sheet.view_active = *at;
    }
    if st.bad_selection {
        report.warn("Malformed .xls selection records were skipped.");
    }
    if st.data_table_cell && !st.data_table_record {
        st.unsupported("what-if data tables");
    }
    st.sheet.autofilter = std::mem::take(&mut st.autofilter).finish(filter, report);
    std::mem::take(&mut st.charts).finish(&mut st.sheet, &g.pictures, report);
    if st.dropped > 0 {
        report.warn(format!("{} cells outside the grid were skipped.", st.dropped));
    }
    if macro_sheet {
        if !st.unsupported.is_empty() {
            report.warn(macro_sheet::UNREAD_FORMULAS);
        }
    } else {
        for why in &st.unsupported {
            report.warn(format!("Some formulas use {why}; their saved results were kept as values."));
        }
    }
    Ok((st.sheet, st.calc, st.pivots, st.queries))
}

/// CodeName ([MS-XLS] 2.4.37): the VBA code name of the workbook (in the globals) or of a sheet.
const CODE_NAME: u16 = 0x01BA;

/// A CodeName record's XLUnicodeString; `None` when it is empty or truncated.
fn code_name(c: &mut Cursor<'_, '_>) -> Option<String> {
    c.string().ok().filter(|n| !n.is_empty())
}

/// The shortest WINDOW2 a file holds (the chart-sheet form); a shorter one is truncated.
const WINDOW2_MIN_LEN: usize = 10;
/// Columns in a BIFF8 sheet (IV).
const XLS_MAX_COLS: u32 = 256;

/// SCL ([MS-XLS] 2.4.247): the current view's zoom as the fraction nscl / dscl. A zero
/// denominator or a zoom outside 10..=400% leaves the sheet at 100%.
fn read_scl(c: &mut Cursor<'_, '_>, sheet: &mut Sheet) {
    let (Ok(n), Ok(d)) = (c.u16(), c.u16()) else { return };
    if d == 0 {
        return;
    }
    let pct = (u32::from(n) * 100 + u32::from(d) / 2) / u32::from(d);
    if let Ok(pct) = u16::try_from(pct)
        && (10..=400).contains(&pct)
    {
        sheet.zoom = pct;
    }
}

/// PLV ([MS-XLS] 2.4.200): FrtHeader (12 bytes), wScalePLV, then flags whose bit 0 is
/// fPageLayoutView.
fn read_plv(c: &mut Cursor<'_, '_>, sheet: &mut Sheet) {
    if c.skip(14).is_ok()
        && let Ok(flags) = c.u16()
        && flags & 0x0001 != 0
    {
        sheet.view = SheetViewMode::PageLayout;
    }
}

/// WINDOW2 fSLV: Page Break Preview. A PLV record's Page Layout view wins, whichever comes first.
fn read_fslv(fslv: bool, sheet: &mut Sheet) {
    if fslv && sheet.view != SheetViewMode::PageLayout {
        sheet.view = SheetViewMode::PageBreakPreview;
    }
}

/// ROW ([MS-XLS] 2.4.221): the row's height, hidden/outline flags and style.
fn read_row(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>) -> Result<(), IoError> {
    let r = c.u16()?;
    c.skip(4)?;
    let height = c.u16()?;
    c.skip(4)?;
    let flags = c.u32()?;
    // [MS-XLS] 2.4.221: miyRw MUST be 2..=8192 twips. Outside it (0, or 0x8000 in real
    // files) the record carries no height, so the row keeps the sheet default height (as
    // Excel opens it) instead of collapsing to 0.
    let usable = (2..=8192).contains(&height);
    let mut info = LineInfo { custom: usable && flags & 0x40 != 0, hidden: flags & 0x20 != 0, ..LineInfo::default() };
    if usable {
        let px = pt_to_px(f64::from(height) / 20.0);
        if info.custom || (px - st.sheet.default_row_height).abs() > 0.5 {
            info.size = Some(px);
        }
    }
    info.outline = (flags & 7) as u8;
    info.collapsed = flags & 0x10 != 0;
    if flags & 0x80 != 0 {
        let style = st.style(((flags >> 16) & 0x0FFF) as u16);
        if style != StyleId::DEFAULT {
            info.style = Some(style);
        }
    }
    if info != LineInfo::default() && u32::from(r) < MAX_ROWS {
        st.sheet.rows.insert(u32::from(r), info);
    }
    Ok(())
}

fn sheet_record(rec: &Record<'_>, st: &mut SheetState<'_>, budget: &mut Budget, report: &mut ReadReport) -> Result<(), IoError> {
    let mut c = Cursor::new(&rec.parts);
    // A formula's string result is the String record right after it (shared/array records may sit between).
    if !matches!(rec.id, 0x0207 | 0x04BC | 0x0221 | 0x0236) {
        st.pending_string = None;
    }
    st.cond.next_record(rec.id);
    st.autofilter.next_record(rec.id, report);
    st.charts.next_record(rec.id, report);
    // BIFF5 has no query tables.
    if st.g.biff5.is_none() {
        st.queries.record(rec);
    }
    if page_setup::read(rec, &mut st.sheet.print, &mut st.sheet.tab_color, &mut st.page, st.g.biff5.as_ref(), report) {
        return Ok(());
    }
    match rec.id {
        0x0012 => protection::protect(&mut c, &mut st.protection, report),
        0x0013 => protection::password(&mut c, &mut st.protection, report),
        0x0867 => protection::feat_hdr(&mut c, &mut st.protection, report),
        0x0868 => protection::feat(&mut c, &mut st.sheet, report),
        CODE_NAME => st.sheet.code_name = code_name(&mut c),
        0x0203 => {
            let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
            let v = Value::number(c.f64()?);
            st.put(budget, r, col, x, v)?;
        }
        0x027E => {
            let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
            let v = rk(c.u32()?);
            st.put(budget, r, col, x, v)?;
        }
        0x00BD => {
            let r = c.u16()?;
            let first = c.u16()?;
            let n = c.remaining().saturating_sub(2) / 6;
            for i in 0..n {
                let x = c.u16()?;
                let v = rk(c.u32()?);
                let col = u16::try_from(usize::from(first) + i).unwrap_or(u16::MAX);
                st.put(budget, r, col, x, v)?;
            }
        }
        0x00FD => {
            let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
            let i = c.u32()? as usize;
            match st.g.sst.get(i) {
                Some(s) => {
                    st.put_text(budget, r, col, x, Value::Text(s.clone()))?;
                }
                None => report.warn("Some cells refer to missing shared strings and were left empty."),
            }
        }
        // Label, and BIFF5's RString (a label with formatting runs, which are skipped).
        0x0204 | 0x00D6 => {
            let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
            let s = text(&mut c, st.g)?;
            budget.text(s.len())?;
            st.put_text(budget, r, col, x, Value::from(s))?;
        }
        0x0205 => {
            let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
            let (v, is_err) = (c.u8()?, c.u8()?);
            let v = if is_err != 0 { Value::Error(cell_error(v)) } else { Value::Bool(v != 0) };
            st.put(budget, r, col, x, v)?;
        }
        0x0201 => {
            let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
            st.put(budget, r, col, x, Value::Empty)?;
        }
        0x00BE => {
            let r = c.u16()?;
            let first = c.u16()?;
            let n = c.remaining().saturating_sub(2) / 2;
            for i in 0..n {
                let x = c.u16()?;
                let col = u16::try_from(usize::from(first) + i).unwrap_or(u16::MAX);
                st.put(budget, r, col, x, Value::Empty)?;
            }
        }
        0x0006 => formula_cell(&mut c, st, budget)?,
        0x0207 => {
            if let Some((pos, style)) = st.pending_string.take() {
                let s = text(&mut c, st.g)?;
                budget.text(s.len())?;
                let (value, cut) = fit_cell_value(Value::from(s));
                if cut {
                    st.note_cut(pos);
                }
                if let Some(cell) = st.sheet.cells.get_mut(pos) {
                    cell.value = value;
                } else {
                    st.sheet.cells.set(pos, Cell { value, formula: None, style, py: None });
                }
            }
        }
        0x04BC => {
            // RefU (the cells that may use it), reserved, cUse.
            c.skip(8)?;
            let f = parsed(&mut c)?;
            if let Some(at) = st.last_formula {
                st.shared.insert(at, f);
            }
        }
        0x0221 => {
            let (r0, r1) = (c.u16()?, c.u16()?);
            let (c0, c1) = (c.u8()?, c.u8()?);
            c.skip(6)?;
            let f = parsed(&mut c)?;
            let range = RangeRef::new(CellRef::new(u32::from(r0), u32::from(c0)), CellRef::new(u32::from(r1.max(r0)), u32::from(c1.max(c0))));
            if let Some(at) = st.last_formula {
                st.arrays.insert(at, (range, f));
            }
        }
        0x000D => {
            st.calc.mode = match c.u16()? as i16 {
                0 => CalcMode::Manual,
                -1 => CalcMode::AutomaticExceptTables,
                _ => CalcMode::Automatic,
            }
        }
        0x000C => st.calc.set_max_iterations(u32::from(c.u16()?)),
        0x0011 => st.calc.iterative = c.u16()? == 1,
        0x0010 => st.calc.set_max_change(c.f64()?),
        0x0236 => data_table(&rec.parts, st, report),
        0x00E5 => {
            let n = usize::from(c.u16()?);
            for _ in 0..n.min(c.remaining() / 8) {
                let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
                if r1 < r0 || c1 < c0 || u32::from(c1) >= 256 {
                    continue;
                }
                if st.sheet.merges.len() >= MAX_MERGES {
                    return Err(IoError::TooLarge("XLS: too many merged ranges".into()));
                }
                let range = RangeRef::new(CellRef::new(u32::from(r0), u32::from(c0)), CellRef::new(u32::from(r1), u32::from(c1)));
                if !range.is_single() {
                    st.sheet.merges.push(range);
                }
            }
        }
        0x007D => {
            let (first, last) = (c.u16()?, c.u16()?);
            let width = c.u16()?;
            let x = c.u16()?;
            let flags = c.u16()?;
            let last = last.min(255);
            if first > last {
                return Ok(());
            }
            let mut info = LineInfo::default();
            let px = col_width_to_px(f64::from(width) / 256.0);
            if flags & 0x0002 != 0 || px != st.sheet.default_col_width {
                info.size = Some(px);
            }
            info.hidden = flags & 0x0001 != 0;
            info.outline = ((flags >> 8) & 7) as u8;
            info.collapsed = flags & 0x1000 != 0;
            let style = st.style(x);
            if style != StyleId::DEFAULT {
                info.style = Some(style);
            }
            if info != LineInfo::default() {
                for col in first..=last {
                    st.sheet.cols.insert(u32::from(col), info);
                }
            }
        }
        0x0208 => read_row(&mut c, st)?,
        0x0225 => {
            let flags = c.u16()?;
            let height = c.u16()?;
            if flags & 0x0002 == 0 && height > 0 {
                st.sheet.default_row_height = pt_to_px(f64::from(height) / 20.0);
            }
        }
        0x023E => {
            let flags = if c.remaining() >= WINDOW2_MIN_LEN { c.u16().ok() } else { None };
            let Some(flags) = flags else { return Ok(()) };
            let on = |bit: u16| flags & bit != 0;
            st.sheet.show_formulas = on(0x0001);
            st.sheet.show_gridlines = on(0x0002);
            st.sheet.show_headings = on(0x0004);
            st.sheet.show_zeros = on(0x0010);
            st.sheet.right_to_left = on(0x0040);
            read_fslv(on(0x0800), &mut st.sheet);
            // rwTop and colLeft: the scroll position, which frozen panes replace.
            if let (Ok(top), Ok(left)) = (c.u16(), c.u16())
                && st.sheet.freeze.is_none()
                && u32::from(top) < MAX_ROWS
                && u32::from(left) < XLS_MAX_COLS
            {
                st.sheet.view_top_left = CellRef::new(u32::from(top), u32::from(left));
            }
        }
        0x00A0 => read_scl(&mut c, &mut st.sheet),
        0x088B => read_plv(&mut c, &mut st.sheet),
        0x001D => {
            // SELECTION ([MS-XLS] 2.4.248): pnn, rwAct, colAct, irefAct, cref, then RefU entries
            // (2.5.207: rwFirst, rwLast, colFirst, colLast = 6 bytes each).
            // A truncated record is skipped; the sheet keeps its cells.
            let mut parse = || -> Result<(u8, u16, u16), IoError> {
                let (pnn, row, col) = (c.u8()?, c.u16()?, c.u16()?);
                c.skip(2)?; // irefAct.
                let count = c.u16()?; // cref.
                for _ in 0..count {
                    c.skip(6)?;
                }
                Ok((pnn, row, col))
            };
            match parse() {
                Ok((pnn, row, col)) if u32::from(row) < MAX_ROWS && u32::from(col) < XLS_MAX_COLS => {
                    st.selections.push((pnn, CellRef::new(u32::from(row), u32::from(col))));
                }
                Ok(_) => {}
                Err(_) => st.bad_selection = true,
            }
        }
        0x001C => report.warn("Cell comments in .xls files are not imported yet."),
        0x00EC => st.charts.drawing(rec),
        0x005D => st.charts.obj(rec, st.g.biff5.as_ref(), &st.g.links, report),
        0x01B6 => st.charts.txo(rec, report),
        // IMDATA: a BIFF5 picture's image (BIFF8 pictures are in the drawing's blip store).
        0x007F => st.charts.imdata(rec, st.g.biff5.is_some()),
        // Only a BIFF5 polygon waits for its points.
        chart::COORDLIST => st.charts.coordlist(rec, report),
        // DVAL is only a header; import actual DV records regardless of its declared count.
        0x01BE => validation::read(&mut c, st, budget, report)?,
        0x01B0 | 0x0879 => st.cond.header(&c.rest(), rec.id == 0x0879),
        0x01B1 => st.cond.cf(&c.rest(), &st.g.links, &mut st.sheet),
        0x087A => st.cond.cf12(&c.rest(), &st.g.links, &mut st.sheet),
        0x087B => st.cond.cf_ex(&c.rest(), &mut st.sheet),
        0x01B8 => report.warn("Hyperlinks in .xls files are not imported yet."),
        0x009D => autofilter::State::info(&mut c, st, report),
        0x009E => autofilter::State::criterion(&mut c, st, report),
        0x087E => autofilter::State::filter12(&mut c, st, report),
        0x087F => autofilter::State::continue_frt12(&mut c, st),
        // Tables: FeatHdr11 (nothing to import), Feature11/Feature12, ContinueFrt11, List12.
        0x0871 => {}
        0x0872 | 0x0878 => st.tables.feature(c.rest()),
        0x0875 => st.tables.continue_frt(&c.rest()),
        0x0877 => st.tables.list12(&c.rest()),
        id if st.pivots.record(id, &mut c) => {}
        _ => {}
    }
    Ok(())
}

/// A cell's text: an XLUnicodeString, or in BIFF5 a 2-byte count and 8-bit characters.
fn text(c: &mut Cursor<'_, '_>, g: &Globals) -> Result<String, IoError> {
    match &g.biff5 {
        Some(b5) => {
            let cch = usize::from(c.u16()?);
            Ok(b5.text(&c.bytes(cch)?))
        }
        None => c.string(),
    }
}

fn formula_cell(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>, budget: &mut Budget) -> Result<(), IoError> {
    let (r, col, x) = (c.u16()?, c.u16()?, c.u16()?);
    let mut val = [0u8; 8];
    for b in val.iter_mut() {
        *b = c.u8()?;
    }
    let _flags = c.u16()?;
    c.skip(4)?;
    let f = parsed(c)?;
    let [b0, _, b2, _, _, _, e0, e1] = val;
    let mut string_result = false;
    let value = if [e0, e1] == [0xFF, 0xFF] {
        match b0 {
            0x00 => {
                string_result = true;
                Value::Empty
            }
            0x01 => Value::Bool(b2 != 0),
            0x02 => Value::Error(cell_error(b2)),
            0x03 => Value::from(""),
            _ => Value::Empty,
        }
    } else {
        Value::number(f64::from_le_bytes(val))
    };
    let Some(pos) = st.pos(r, col) else { return Ok(()) };
    budget.cell()?;
    let style = st.style(x);
    st.sheet.cells.set(pos, Cell { value, formula: None, style, py: None });
    st.last_formula = Some(pos);
    if string_result {
        st.pending_string = Some((pos, style));
    }
    // PtgTbl: a What-If data table's result cell. The table is read from its TABLE record; the
    // cell keeps only its value, as in `.xlsx`.
    if st.g.biff5.is_none() && f.rgce.len() == 5 && f.rgce.first() == Some(&0x02) {
        st.data_table_cell = true;
        return Ok(());
    }
    // PtgExp: part of a shared or array formula anchored at another cell.
    if f.rgce.len() == 5 && f.rgce.first() == Some(&0x01) {
        let get = |i: usize| f.rgce.get(i).copied().unwrap_or(0);
        let anchor = CellRef::new(u32::from(u16::from_le_bytes([get(1), get(2)])), u32::from(u16::from_le_bytes([get(3), get(4)])));
        st.pending.push(Pending { pos, anchor });
        return Ok(());
    }
    st.attach(pos, &f, Mode::Cell, None);
    Ok(())
}

/// The most What-If data tables kept on one sheet.
const MAX_DATA_TABLES: usize = 65_536;

/// A BIFF8 TABLE record ([MS-XLS] 2.4.319; BIFF5 tables keep only their values): one What-If data table, mapped to the model as the
/// `.xlsx` `<f t="dataTable">` attributes are. A malformed record is skipped with a warning; a
/// second table with the same result range is skipped.
fn data_table(parts: &[&[u8]], st: &mut SheetState<'_>, report: &mut ReadReport) {
    if st.g.biff5.is_some() {
        report.warn("What-if data tables are not supported; their values were kept.");
        return;
    }
    st.data_table_record = true;
    match parse_data_table(parts) {
        Some(t) => {
            if st.data_table_ranges.len() < MAX_DATA_TABLES && st.data_table_ranges.insert(t.range) {
                st.sheet.data_tables.push(t);
            }
        }
        None => report.warn("Malformed .xls What-if data table records were skipped; their values were kept."),
    }
}

fn parse_data_table(parts: &[&[u8]]) -> Option<gridcraft_model::DataTable> {
    let mut c = Cursor::new(parts);
    let (r0, r1) = (c.u16().ok()?, c.u16().ok()?);
    let (c0, c1) = (c.u8().ok()?, c.u8().ok()?);
    let (flags, inp_r1, inp_c1, inp_r2, inp_c2) = (c.u16().ok()?, c.u16().ok()?, c.u16().ok()?, c.u16().ok()?, c.u16().ok()?);
    // Columns are read as one byte, so only the input cells can lie past IV.
    if r1 < r0 || c1 < c0 || u32::from(inp_c1) >= XLS_MAX_COLS || u32::from(inp_c2) >= XLS_MAX_COLS {
        return None;
    }
    let cell = |r: u16, c: u16| CellRef::new(u32::from(r), u32::from(c));
    let two_d = flags & 0x0008 != 0;
    Some(gridcraft_model::DataTable {
        range: RangeRef::new(cell(r0, u16::from(c0)), cell(r1, u16::from(c1))),
        two_d,
        row: flags & 0x0004 != 0,
        r1: Some(cell(inp_r1, inp_c1)),
        r2: two_d.then(|| cell(inp_r2, inp_c2)),
        del1: flags & 0x0010 != 0,
        del2: flags & 0x0020 != 0,
        ca: flags & 0x0001 != 0,
    })
}

/// The range of one worksheet's `_FilterDatabase` built-in name ([MS-XLS] 2.4.150), which is the
/// range its AutoFilter filters. `sheet` is the BoundSheet8 index (the name's sheet scope).
fn filter_database(g: &Globals, sheet: usize) -> Option<RangeRef> {
    if g.biff5.is_some() {
        // BIFF5 name tokens use other layouts; the AutoFilter range isn't read.
        return None;
    }
    let lbl = g.names.iter().find(|l| l.builtin == Some(13) && l.scope == Some(sheet))?;
    autofilter::filter_database(&lbl.rgce, &lbl.rgcb, &g.links)
}

fn defined_names(names: &[Lbl], links: &Links, biff5: Option<&Biff5>, model_index: &[Option<usize>], wb: &mut Workbook, report: &mut ReadReport) {
    // References into other workbooks keep the `.xlsx` text (`[1]Data!$B$2`), as cells do.
    add_defined_names(names, model_index, wb, report, |l| {
        decode_name(&l.rgce, &l.rgcb, links, biff5).map(|e| external_sheet_text(&gridcraft_formula::print(&e)).0)
    });
}

/// Adds the defined names to `wb`: print areas and print titles go to their sheet's page setup,
/// other built-in names and macro names are skipped. `model_index` maps file sheet positions to
/// imported sheets; `decode` turns a name's formula into text.
pub(crate) fn add_defined_names(
    names: &[Lbl],
    model_index: &[Option<usize>],
    wb: &mut Workbook,
    report: &mut ReadReport,
    decode: impl Fn(&Lbl) -> Result<String, Unsupported>,
) {
    for lbl in names {
        let scope = match lbl.scope {
            None => None,
            Some(i) => match model_index.get(i).copied().flatten() {
                Some(m) => Some(m),
                None => continue,
            },
        };
        if lbl.future_function || lbl.skip {
            continue;
        }
        let text = match decode(lbl) {
            Ok(text) => text,
            Err(_) if lbl.rgce.is_empty() => continue,
            Err(Unsupported(why)) => {
                report.warn(format!("Some defined names use {why} and were skipped."));
                continue;
            }
        };
        match lbl.builtin {
            Some(6 | 7) => {
                let Some(si) = scope else { continue };
                let Some(sheet) = wb.sheet_mut(si) else { continue };
                if lbl.builtin == Some(6) {
                    sheet.print.set_print_areas(crate::read::parse_print_areas(&text));
                } else {
                    let (rows, cols) = crate::read::parse_print_titles(&text);
                    sheet.print.title_rows = rows;
                    sheet.print.title_cols = cols;
                }
            }
            Some(_) => {}
            None if lbl.macro_name => report.warn("Macro names were skipped."),
            None => wb.names.push(DefinedName { name: lbl.name.clone(), scope, formula: text, comment: String::new(), hidden: lbl.hidden }),
        }
    }
}

/// Why [`read_xls_with_password`] could not open a workbook.
#[derive(Debug, thiserror::Error)]
pub enum XlsOpenError {
    /// Encrypted with a password other than Excel's default, and no password was given.
    #[error("the workbook is password-protected")]
    PasswordRequired,
    /// The password is wrong, or the encryption can't be read.
    #[error(transparent)]
    Decrypt(#[from] DecryptError),
    #[error(transparent)]
    Io(#[from] IoError),
}

/// [`read_xls`], decrypting a workbook encrypted with `password`. Excel's default password is
/// always tried first, so an unencrypted or default-password workbook opens with any password.
pub fn read_xls_with_password(bytes: &[u8], password: Option<&str>) -> Result<(Workbook, ReadReport), XlsOpenError> {
    read_xls_with_password_using(bytes, password, FunctionInfo::default())
}

/// [`read_xls_with_password`] with the function library's facts (see [`read_xls_with`]).
pub fn read_xls_with_password_using(bytes: &[u8], password: Option<&str>, functions: FunctionInfo) -> Result<(Workbook, ReadReport), XlsOpenError> {
    read_xls_inner(bytes, Budget::default(), Lock::Password(password), functions)
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn a_refused_shared_formula_ends_shared_expansion_for_later_sheets() {
        let mut budget = Budget::with_text_left(10);
        assert!(budget.try_shared_text(8));
        assert!(!budget.try_shared_text(8), "over the allowance");
        // a shorter formula would fit the 2 bytes left, but the cut-off holds
        assert!(!budget.try_shared_text(1));
        assert!(budget.try_text(1), "text the import needs is still charged normally");
    }
}

#[cfg(test)]
mod cell_text_tests {
    use super::*;

    #[test]
    fn cell_text_is_cut_at_the_limit_between_characters() {
        assert_eq!(cut_cell_text(&"a".repeat(CELL_TEXT_MAX)), None);
        assert_eq!(cut_cell_text(&"a".repeat(CELL_TEXT_MAX + 1)).map(str::len), Some(CELL_TEXT_MAX));
        // Multi-byte characters of one UTF-16 unit each: counted as units, not bytes.
        let e = "\u{e9}".repeat(CELL_TEXT_MAX);
        assert_eq!(cut_cell_text(&e), None);
        let pair = format!("{}\u{1F600}", "a".repeat(CELL_TEXT_MAX - 1));
        assert_eq!(cut_cell_text(&pair), Some("a".repeat(CELL_TEXT_MAX - 1).as_str()));
        let (v, cut) = fit_cell_value(Value::number(1.0));
        assert!(!cut && v == Value::number(1.0));
    }
}
