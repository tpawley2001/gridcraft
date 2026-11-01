//! XLSB tables (ListObjects) and AutoFilters, from [MS-XLSB] 2.1.7.51 (Table part), 2.1.7.62
//! (Worksheet part) and the records of 2.4. A table part holds `BrtBeginList`, an optional
//! AutoFilter block, `BrtBeginListCols` with one `BrtBeginListCol` per column and a
//! `BrtTableStyleClient`; the worksheet lists its table parts with `BrtListPart` relationship ids.
//! Formulas stored only as parsed tokens (calculated columns, custom totals) are not imported.

use std::collections::HashSet;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_model::{AutoFilter, FilterCriterion, Sheet, Table, TableColumn, TotalsFn};

use super::{Bytes, Entry, Records, check_type, invalid, relationships};
use crate::package::Package;
use crate::{IoError, ReadReport};

const TABLE: &str = "application/vnd.ms-excel.table";
/// Tables read per sheet; the rest are skipped with a warning.
pub(super) const MAX_TABLES: usize = 1024;
const MAX_FILTER_COLUMNS: usize = 16_384;
const MAX_FILTER_VALUES: usize = 10_000;
const MAX_TABLE_PART: u64 = 16 * 1024 * 1024;
const MISSING: &str = "XLSB tables whose part is missing or malformed were skipped.";

const BEGIN_AFILTER: u16 = 161;
const END_AFILTER: u16 = 162;
const BEGIN_FILTER_COLUMN: u16 = 163;
const END_FILTER_COLUMN: u16 = 164;
const BEGIN_FILTERS: u16 = 165;
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
#[derive(Default)]
struct FilterBuilder {
    criteria: Vec<(u32, FilterCriterion)>,
    col: Option<u32>,
    crit: Option<FilterCriterion>,
    /// Inside `BrtBeginCustomFilters`: whether the comparisons are joined by AND, and them.
    custom: Option<(bool, Vec<(String, String)>)>,
}

impl FilterBuilder {
    fn record(&mut self, id: u16, payload: &[u8], report: &mut ReadReport) -> Result<(), IoError> {
        let mut b = Bytes::new(payload);
        match id {
            BEGIN_FILTER_COLUMN => {
                self.col = Some(b.u32()?);
                self.crit = None;
                self.custom = None;
            }
            END_FILTER_COLUMN => {
                if let (Some(col), Some(crit)) = (self.col.take(), self.crit.take()) {
                    if self.criteria.len() >= MAX_FILTER_COLUMNS {
                        report.warn("XLSB AutoFilter columns past the first 16384 were skipped.");
                    } else if !self.criteria.iter().any(|(c, _)| *c == col) {
                        self.criteria.push((col, crit));
                    }
                }
            }
            BEGIN_FILTERS => {
                let blanks = b.u32()? != 0;
                self.crit = Some(FilterCriterion::Values { values: vec![], blanks });
            }
            FILTER => {
                let value = b.text(255)?;
                if let Some(FilterCriterion::Values { values, .. }) = &mut self.crit {
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
            DYNAMIC_FILTER => match b.u32()? {
                1 => self.crit = Some(FilterCriterion::AboveAverage(true)),
                2 => self.crit = Some(FilterCriterion::AboveAverage(false)),
                _ => report.warn("XLSB date filters in AutoFilters are not imported."),
            },
            DATE_GROUP_ITEM => report.warn("XLSB date filters in AutoFilters are not imported."),
            COLOR_FILTER | ICON_FILTER => report.warn("XLSB colour and icon filters in AutoFilters are not imported."),
            _ => {}
        }
        Ok(())
    }
}

/// Reads an AutoFilter block from its `BrtBeginAFilter` payload through `BrtEndAFilter`. A bad
/// range drops the whole filter and a malformed criterion drops that column, each with a warning.
pub(super) fn autofilter(records: &mut Records<'_>, begin: &[u8], report: &mut ReadReport) -> Result<Option<AutoFilter>, IoError> {
    let mut b = Bytes::new(begin);
    let range = rfx(&mut b).ok().flatten();
    let mut filter = FilterBuilder::default();
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

/// A table part. `Err` for a part that can't be decoded; the caller skips the table.
fn table(bytes: &[u8], report: &mut ReadReport) -> Result<Table, IoError> {
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
    b.u32()?; // idList: ids are renumbered on import, like the .xlsx reader does.
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
            END_LIST => return Ok(t),
            BEGIN_AFILTER => {
                if let Some(af) = autofilter(&mut records, payload, report)? {
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
            LIST_CC_FMLA => report.warn("XLSB calculated column formulas in tables are not imported yet."),
            LIST_TR_FMLA => {
                if t.columns.last().is_some_and(|c| c.totals == TotalsFn::Custom) {
                    report.warn("XLSB custom totals row formulas in tables are not imported yet.");
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

/// Adds the tables of a worksheet part (`sheet_path`, its `BrtListPart` relationship `ids`) to
/// `sheet`. A table that is missing, malformed or invalid is skipped with one warning per kind;
/// `names` (lower case) keeps table names unique in the workbook and `next_id` numbers them.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_tables(
    package: &mut Package<'_>,
    types: &[Entry],
    sheet_path: &str,
    ids: &[String],
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
        let Some(mut t) = bytes.ok().flatten().and_then(|bytes| table(&bytes, report).ok()) else {
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
