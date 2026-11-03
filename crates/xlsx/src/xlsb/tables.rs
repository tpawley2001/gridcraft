//! XLSB tables (ListObjects) and AutoFilters, from [MS-XLSB] 2.1.7.51 (Table part), 2.1.7.62
//! (Worksheet part) and the records of 2.4. A table part holds `BrtBeginList`, an optional
//! AutoFilter block, `BrtBeginListCols` with one `BrtBeginListCol` per column and a
//! `BrtTableStyleClient`; the worksheet lists its table parts with `BrtListPart` relationship ids.
//! Calculated column (`BrtListCCFmla`) and custom totals (`BrtListTrFmla`) formulas are parsed
//! tokens; every table is scanned ahead ([`scan_lists`]) so they and cell formulas can resolve
//! structured references by table id.

use std::collections::HashSet;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_model::{AutoFilter, FilterCriterion, Sheet, Style, Table, TableColumn, TotalsFn};

use super::rules::icon_set;
use super::{Bytes, Entry, Records, SheetInfo, check_type, invalid, relationships};
use crate::package::{Package, Rel};
use crate::xls::formula::{Links, List, Mode, Site, decode_xlsb_at};
use crate::{IoError, ReadReport, binary_filter};

const DATE_FILTER_WARNING: &str = "XLSB date filters in AutoFilters are not imported.";

const TABLE: &str = "application/vnd.ms-excel.table";
/// Tables read per sheet; the rest are skipped with a warning.
pub(super) const MAX_TABLES: usize = 1024;
const MAX_FILTER_COLUMNS: usize = 16_384;
const MAX_FILTER_VALUES: usize = 10_000;
const MAX_TABLE_PART: u64 = 16 * 1024 * 1024;
const MISSING: &str = "XLSB tables whose part is missing or malformed were skipped.";
/// Tables scanned in the whole workbook for structured references.
const MAX_LISTS: usize = 65_536;

const BEGIN_AFILTER: u16 = 161;
const END_AFILTER: u16 = 162;
const BEGIN_FILTER_COLUMN: u16 = 163;
const END_FILTER_COLUMN: u16 = 164;
const BEGIN_FILTERS: u16 = 165;
const END_FILTERS: u16 = 166;
const FILTER: u16 = 167;
const COLOR_FILTER: u16 = 168;
const ICON_FILTER: u16 = 169;
const TOP10_FILTER: u16 = 170;
const DYNAMIC_FILTER: u16 = 171;
const BEGIN_CUSTOM_FILTERS: u16 = 172;
const END_CUSTOM_FILTERS: u16 = 173;
const CUSTOM_FILTER: u16 = 174;
const DATE_GROUP_ITEM: u16 = 175;
const BEGIN_LIST: u16 = 343;
const END_LIST: u16 = 344;
const BEGIN_LIST_COL: u16 = 347;
const LIST_CC_FMLA: u16 = 351;
const LIST_TR_FMLA: u16 = 352;
const TABLE_STYLE_CLIENT: u16 = 513;
pub(super) const LIST_PART: u16 = 661;

/// The range of an `RfX` / `UncheckedRfX` (rwFirst, rwLast, colFirst, colLast); `None` when it
/// is reversed or leaves the sheet.
fn rfx(b: &mut Bytes<'_>) -> Result<Option<RangeRef>, IoError> {
    let (r0, r1, c0, c1) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
    if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
        return Ok(None);
    }
    Ok(Some(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1))))
}

/// The relationship id of a `BrtListPart` record; `None` for a null id.
pub(super) fn list_part(payload: &[u8]) -> Result<Option<String>, IoError> {
    let mut b = Bytes::new(payload);
    let id = b.string(255, true)?;
    b.finish()?;
    Ok(id.filter(|s| !s.is_empty()))
}

/// The comparison of a `BrtCustomFilter` (`grbitSgn`) as the model's operator text.
fn custom_op(sign: u8) -> Option<&'static str> {
    Some(match sign {
        1 => "<",
        2 => "=",
        3 => "<=",
        4 => ">",
        5 => "<>",
        6 => ">=",
        _ => return None,
    })
}

/// One `BrtCustomFilter`: (operator, value as the `val` text the .xlsx reader keeps).
fn custom_filter(payload: &[u8]) -> Result<(String, String), IoError> {
    let mut b = Bytes::new(payload);
    let vts = b.u8()?;
    let op = custom_op(b.u8()?).ok_or_else(|| invalid("unknown custom filter comparison"))?;
    let union = b.take(8)?;
    let value = match vts {
        0x04 => {
            let n = f64::from_le_bytes(union.try_into().map_err(|_| invalid("truncated number"))?);
            if !n.is_finite() {
                return Err(invalid("nonfinite filter value"));
            }
            n.to_string()
        }
        0x06 => b.text(255)?,
        0x08 => if union.first().copied().unwrap_or(0) != 0 { "TRUE" } else { "FALSE" }.to_string(),
        // Blanks / non-blanks compare with an empty value.
        0x0c => return Ok(("=".into(), String::new())),
        0x0e => return Ok(("<>".into(), String::new())),
        _ => return Err(invalid("unknown custom filter value type")),
    };
    b.finish()?;
    Ok((op.to_string(), value))
}

/// Builds the criteria of one AutoFilter block while its records are read.
struct FilterBuilder<'a> {
    /// The workbook's differential formats, which colour filters refer to.
    dxfs: &'a [Style],
    criteria: Vec<(u32, FilterCriterion)>,
    col: Option<u32>,
    crit: Option<FilterCriterion>,
    /// Inside `BrtBeginCustomFilters`: whether the comparisons are joined by AND, and them.
    custom: Option<(bool, Vec<(String, String)>)>,
    in_filters: bool,
    invalid_date: bool,
}

impl FilterBuilder<'_> {
    fn record(&mut self, id: u16, payload: &[u8], report: &mut ReadReport) -> Result<(), IoError> {
        let mut b = Bytes::new(payload);
        match id {
            BEGIN_FILTER_COLUMN => {
                self.col = Some(b.u32()?);
                self.crit = None;
                self.custom = None;
                self.in_filters = false;
                self.invalid_date = false;
            }
            END_FILTER_COLUMN => {
                if self.invalid_date && matches!(&self.crit, Some(FilterCriterion::Values { values, blanks: false }) if values.is_empty()) {
                    self.crit = None;
                }
                if let (Some(col), Some(crit)) = (self.col.take(), self.crit.take()) {
                    if self.criteria.len() >= MAX_FILTER_COLUMNS {
                        report.warn("XLSB AutoFilter columns past the first 16384 were skipped.");
                    } else if !self.criteria.iter().any(|(c, _)| *c == col) {
                        self.criteria.push((col, crit));
                    }
                }
            }
            BEGIN_FILTERS => {
                self.in_filters = true;
                let blanks = b.u32()? != 0;
                self.crit = Some(FilterCriterion::Values { values: vec![], blanks });
            }
            END_FILTERS => self.in_filters = false,
            FILTER => {
                let value = b.text(255)?;
                if let Some(FilterCriterion::Values { values, .. } | FilterCriterion::DateGroups { values, .. }) = &mut self.crit {
                    if values.len() < MAX_FILTER_VALUES {
                        values.push(value);
                    } else {
                        report.warn("XLSB AutoFilter value lists were cut to 10000 values.");
                    }
                }
            }
            BEGIN_CUSTOM_FILTERS => {
                // fAnd: 0 means AND, 1 means OR ([MS-XLSB] 2.4.40).
                self.custom = Some((b.u32()? == 0, vec![]));
            }
            CUSTOM_FILTER => {
                let c = custom_filter(payload)?;
                if let Some((_, list)) = &mut self.custom
                    && list.len() < 2
                {
                    list.push(c);
                }
            }
            END_CUSTOM_FILTERS => {
                if let Some((and, list)) = self.custom.take() {
                    let mut it = list.into_iter();
                    if let Some(a) = it.next() {
                        self.crit = Some(FilterCriterion::Custom { a, b: it.next(), and });
                    }
                }
            }
            TOP10_FILTER => {
                let flags = b.u8()?;
                let value = b.f64()?;
                self.crit = Some(FilterCriterion::Top10 { bottom: flags & 1 == 0, percent: flags & 2 != 0, count: value.clamp(0.0, 1e9) as u32 });
            }
            DYNAMIC_FILTER => {
                let cft = b.u32()?;
                let applied = b.u8()? & 1 != 0;
                let bounds = (b.f64()?, b.f64()?);
                self.crit = binary_filter::dynamic(cft, applied.then_some(bounds));
                if self.crit.is_none() {
                    report.warn(DATE_FILTER_WARNING);
                }
            }
            DATE_GROUP_ITEM if self.in_filters => {
                let parsed = (|| {
                    let year = b.u16()?;
                    let month = b.u16()?;
                    let day = b.u32()?;
                    let time = [b.u16()?, b.u16()?, b.u16()?];
                    b.take(6)?;
                    let depth = b.u32()?;
                    Ok::<_, IoError>(binary_filter::date_group(year, month, day, time, depth))
                })();
                if let Ok(Some(group)) = parsed {
                    if let Some(crit) = &mut self.crit {
                        let count = match crit {
                            FilterCriterion::DateGroups { groups, .. } => groups.len(),
                            _ => 0,
                        };
                        if count < MAX_FILTER_VALUES {
                            binary_filter::add_group(crit, group);
                        } else {
                            report.warn(DATE_FILTER_WARNING);
                        }
                    }
                } else {
                    report.warn(DATE_FILTER_WARNING);
                    self.invalid_date = true;
                }
            }
            COLOR_FILTER => {
                // BrtColorFilter: dxfId, then fCellColor (fill colour when non-zero, else font
                // colour). A missing format drops the criterion, like the .xlsx reader.
                let dxf = usize::try_from(b.u32()?).ok().and_then(|i| self.dxfs.get(i));
                let cell = b.u32()? != 0;
                self.crit = dxf.map(|d| if cell { FilterCriterion::FillColor(d.fill.fg) } else { FilterCriterion::FontColor(d.font.color) });
            }
            ICON_FILTER => {
                // BrtIconFilter: iIconSet (KPISets), then iIcon; -1 selects cells with no icon.
                let set = b.u32()?;
                let icon = b.u32()?;
                match icon_set(set) {
                    Some(name) => self.crit = Some(FilterCriterion::Icon { set: name.to_string(), id: (icon != u32::MAX).then_some(icon) }),
                    None => {
                        self.crit = None;
                        report.warn("XLSB colour and icon filters in AutoFilters are not imported.");
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Reads an AutoFilter block from its `BrtBeginAFilter` payload through `BrtEndAFilter`. A bad
/// range drops the whole filter and a malformed criterion drops that column, each with a warning.
pub(super) fn autofilter(records: &mut Records<'_>, begin: &[u8], dxfs: &[Style], report: &mut ReadReport) -> Result<Option<AutoFilter>, IoError> {
    let mut b = Bytes::new(begin);
    let range = rfx(&mut b).ok().flatten();
    let mut filter = FilterBuilder { dxfs, criteria: vec![], col: None, crit: None, custom: None, in_filters: false, invalid_date: false };
    loop {
        let (id, payload) = records.next(report)?.ok_or_else(|| invalid("unterminated AutoFilter"))?;
        if id == END_AFILTER {
            break;
        }
        if filter.record(id, payload, report).is_err() {
            report.warn("Malformed XLSB AutoFilter criteria were skipped.");
            filter.crit = None;
            filter.custom = None;
        }
    }
    let Some(range) = range else {
        report.warn("XLSB AutoFilters with an invalid range were skipped.");
        return Ok(None);
    };
    let width = range.width();
    filter.criteria.retain(|(c, _)| *c < width);
    Ok(Some(AutoFilter { range, criteria: filter.criteria }))
}

fn totals_fn(ilta: u32) -> TotalsFn {
    match ilta {
        1 => TotalsFn::Average,
        2 => TotalsFn::Count,
        3 => TotalsFn::CountNums,
        4 => TotalsFn::Max,
        5 => TotalsFn::Min,
        6 => TotalsFn::Sum,
        7 => TotalsFn::StdDev,
        8 => TotalsFn::Var,
        9 => TotalsFn::Custom,
        _ => TotalsFn::None,
    }
}

/// `BrtBeginListCol`: idField, ilta, three DXF ids, idqsif, then stName (unique name), stCaption
/// (the column name), stTotal (totals row label) and three cell style names.
fn list_col(payload: &[u8]) -> Result<TableColumn, IoError> {
    let mut b = Bytes::new(payload);
    b.u32()?;
    let totals = totals_fn(b.u32()?);
    b.take(16)?;
    let unique_name = b.string(255, true)?;
    let caption = b.string(255, true)?;
    let total = b.string(8189, true)?;
    Ok(TableColumn {
        name: caption.or_else(|| unique_name.clone()).unwrap_or_default(),
        totals,
        totals_label: total.filter(|_| totals != TotalsFn::Custom),
        formula: None,
        unique_name,
        xml: None,
        header_row_dxf: None,
        data_dxf: None,
        totals_row_dxf: None,
    })
}

/// A `BrtListCCFmla` / `BrtListTrFmla` formula (fArray and reserved bits, then a
/// `ListParsedFormula`: cce, rgce, cb, rgcb) at `base`, as formula text; `None` when it can't be
/// decoded.
fn list_formula(payload: &[u8], base: CellRef, links: &Links, site: Site<'_>) -> Option<String> {
    let mut b = Bytes::new(payload);
    b.u16().ok()?;
    let cce = usize::try_from(b.u32().ok()?).ok()?;
    let rgce = b.take(cce).ok()?;
    let cb = b.u32().ok().and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let rgcb = b.take(cb).unwrap_or(&[]);
    let e = decode_xlsb_at(rgce, rgcb, base, Mode::Cell, links, site).ok()?;
    Some(gridcraft_formula::print(&e))
}

/// The links and sheet name formulas inside a table part are decoded with.
type Context<'a> = Option<(&'a Links, &'a str)>;

/// A table part and its `idList`. `Err` for a part that can't be decoded; the caller skips the
/// table. Without a `ctx` (the scan ahead) column formulas are left out.
fn table(bytes: &[u8], ctx: Context<'_>, dxfs: &[Style], report: &mut ReadReport) -> Result<(Table, u32), IoError> {
    let mut records = Records::new(bytes);
    let begin = loop {
        match records.next(report)? {
            Some((BEGIN_LIST, payload)) => break payload,
            Some(_) => {}
            None => return Err(invalid("table part has no BrtBeginList")),
        }
    };
    let mut b = Bytes::new(begin);
    let range = rfx(&mut b)?.ok_or_else(|| invalid("table range is out of bounds"))?;
    let list_type = b.u32()?;
    let list_id = b.u32()?; // Ids are renumbered on import, like the .xlsx reader does.
    let header_row = b.u32()? != 0;
    let totals_row = b.u32()? != 0;
    b.take(4 + 6 * 4)?; // Flags and six DXF ids (differential formats aren't imported from XLSB).
    let connection = b.u32()?;
    let name = b.string(255, true)?;
    let display = b.string(255, true)?;
    let xml_table = list_type == 2;
    let mut t = Table {
        id: 0,
        name: display.or(name).unwrap_or_default(),
        range,
        header_row,
        totals_row,
        columns: vec![],
        style: String::new(),
        banded_rows: false,
        banded_cols: false,
        first_col: false,
        last_col: false,
        filter_button: false,
        filter: vec![],
        table_type: xml_table.then(|| "xml".to_string()),
        connection_id: xml_table.then_some(connection),
        header_row_dxf: None,
        data_dxf: None,
        totals_row_dxf: None,
        header_row_border_dxf: None,
        table_border_dxf: None,
        totals_row_border_dxf: None,
    };
    while let Some((id, payload)) = records.next(report)? {
        match id {
            END_LIST => return Ok((t, list_id)),
            BEGIN_AFILTER => {
                if let Some(af) = autofilter(&mut records, payload, dxfs, report)? {
                    t.filter_button = true;
                    t.filter = af.criteria;
                }
            }
            BEGIN_LIST_COL => {
                if t.columns.len() >= MAX_COLS as usize {
                    return Err(invalid("too many table columns"));
                }
                t.columns.push(list_col(payload)?);
            }
            LIST_CC_FMLA | LIST_TR_FMLA => {
                let Some((links, sheet)) = ctx else { continue };
                let Some(i) = t.columns.len().checked_sub(1) else { continue };
                let custom = t.columns.last().is_some_and(|c| c.totals == TotalsFn::Custom);
                if id == LIST_TR_FMLA && !custom {
                    continue;
                }
                // Calculated columns are relative to the column's first data cell, totals
                // formulas to its totals row cell.
                let row = if id == LIST_CC_FMLA { t.range.start.row.saturating_add(u32::from(t.header_row)) } else { t.range.end.row };
                let col = u32::try_from(i).ok().and_then(|i| t.range.start.col.checked_add(i));
                let site = Site { sheet, table: Some(list_id) };
                let text = col.and_then(|col| list_formula(payload, CellRef::new(row, col), links, site));
                let Some(text) = text else {
                    report.warn(if id == LIST_CC_FMLA {
                        "XLSB calculated column formulas that can't be decoded were skipped."
                    } else {
                        "XLSB custom totals row formulas that can't be decoded were skipped."
                    });
                    continue;
                };
                if let Some(c) = t.columns.last_mut() {
                    if id == LIST_CC_FMLA {
                        c.formula = Some(text);
                    } else {
                        c.totals_label = Some(text);
                    }
                }
            }
            TABLE_STYLE_CLIENT => {
                let mut b = Bytes::new(payload);
                let flags = b.u16()?;
                t.first_col = flags & 1 != 0;
                t.last_col = flags & 2 != 0;
                t.banded_rows = flags & 4 != 0;
                t.banded_cols = flags & 8 != 0;
                t.style = b.string(255, true)?.unwrap_or_default();
            }
            _ => {}
        }
    }
    Err(invalid("table part has no BrtEndList"))
}

/// Why a decoded table can't be placed on its sheet, as the warning to give.
fn check_table(t: &Table, sheet: &Sheet, names: &HashSet<String>) -> Result<(), &'static str> {
    let header_rows = u32::from(t.header_row) + u32::from(t.totals_row);
    if t.range.height() <= header_rows || t.columns.len() != t.range.width() as usize {
        return Err("XLSB tables whose range doesn't fit their header, totals row or columns were skipped.");
    }
    if t.name.trim().is_empty() || names.contains(&t.name.to_lowercase()) {
        return Err("XLSB tables with a missing or duplicate name were skipped.");
    }
    if sheet.tables.iter().any(|o| o.range.intersects(&t.range)) {
        return Err("XLSB tables overlapping another table were skipped.");
    }
    Ok(())
}

/// Every worksheet's tables as structured references name them, read ahead of the sheets so
/// formulas anywhere can refer to them. Parts that can't be read are left out; the import of
/// that sheet's tables warns about them.
pub(super) fn scan_lists(package: &mut Package<'_>, types: &[Entry], book_rels: &[Rel], sheets: &[SheetInfo]) -> Vec<List> {
    let mut out = vec![];
    let mut scratch = ReadReport::default();
    for info in sheets {
        let Some(sheet) = info.rel.as_ref().and_then(|id| book_rels.iter().find(|r| &r.id == id && r.kind == "worksheet" && !r.external)) else {
            continue;
        };
        let Ok(rels) = relationships(package, &sheet.target) else { continue };
        for rel in rels.iter().filter(|r| r.kind == "table" && !r.external) {
            if out.len() >= MAX_LISTS {
                return out;
            }
            let bytes = check_type(types, &rel.target, TABLE).and_then(|()| package.read_limited(&rel.target, MAX_TABLE_PART));
            let Some((t, id)) = bytes.ok().flatten().and_then(|bytes| table(&bytes, None, &[], &mut scratch).ok()) else { continue };
            out.push(List { id, name: t.name, columns: t.columns.into_iter().map(|c| c.name).collect(), sheet: info.name.clone(), range: t.range });
        }
    }
    out
}

/// Adds the tables of a worksheet part (`sheet_path`, its `BrtListPart` relationship `ids`) to
/// `sheet`. A table that is missing, malformed or invalid is skipped with one warning per kind;
/// `names` (lower case) keeps table names unique in the workbook and `next_id` numbers them.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_tables(
    package: &mut Package<'_>,
    types: &[Entry],
    sheet_path: &str,
    ids: &[String],
    links: &Links,
    dxfs: &[Style],
    sheet: &mut Sheet,
    names: &mut HashSet<String>,
    next_id: &mut u32,
    report: &mut ReadReport,
) {
    if ids.is_empty() {
        return;
    }
    let Ok(rels) = relationships(package, sheet_path) else {
        report.warn(MISSING);
        return;
    };
    for id in ids {
        let Some(rel) = rels.iter().find(|r| &r.id == id && r.kind == "table" && !r.external) else {
            report.warn(MISSING);
            continue;
        };
        let bytes = check_type(types, &rel.target, TABLE).and_then(|()| package.read_limited(&rel.target, MAX_TABLE_PART));
        let ctx = Some((links, sheet.name.as_str()));
        let Some((mut t, _)) = bytes.ok().flatten().and_then(|bytes| table(&bytes, ctx, dxfs, report).ok()) else {
            report.warn(MISSING);
            continue;
        };
        if let Err(warning) = check_table(&t, sheet, names) {
            report.warn(warning);
            continue;
        }
        names.insert(t.name.to_lowercase());
        t.id = *next_id;
        *next_id = next_id.saturating_add(1);
        sheet.tables.push(t);
    }
}
