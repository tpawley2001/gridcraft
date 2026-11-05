//! Worksheet tables (Excel 2003 lists and Excel 2007 tables) from the shared-feature records of
//! [MS-XLS]: Feature11 (2.4.114) and Feature12 holding a TableFeatureType (2.5.266) with its
//! Feat11FieldDataItem columns (2.5.113), continued by ContinueFrt11 records, and the List12
//! records (2.4.157) carrying each table's style (List12TableStyleClientInfo) and display name
//! (List12DisplayName). FeatHdr11 holds nothing to import.
//!
//! A table that can't be read is skipped, with one warning per sheet; its cells open as usual.

use std::collections::HashSet;
use std::sync::Arc;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_model::{Sheet, Table, TableColumn, TotalsFn, Workbook};

use super::{Budget, Cursor, invalid};
use crate::sheet_write::{table_columns, table_name};
use crate::{IoError, ReadReport};

/// Tables kept per sheet at most.
const MAX_TABLES: usize = 4096;
/// Bytes gathered for one Feature11/12 record and its ContinueFrt11 records at most.
const MAX_FEATURE_BYTES: usize = 4 << 20;
/// SharedFeatureType ISFLIST.
const ISF_LIST: u16 = 5;

/// TableFeatureType flag bits ([MS-XLS] 2.5.266).
const F_AUTO_FILTER: u32 = 1 << 1;
const F_SINGLE_CELL: u32 = 1 << 9;
const F_LOAD_CSP_NAME: u32 = 1 << 14;
const F_LOAD_ENTRY_ID: u32 = 1 << 20;

/// Feat11FieldDataItem flag bits ([MS-XLS] 2.5.113).
const FD_LOAD_XMAPI: u32 = 1 << 2;
const FD_LOAD_FMLA: u32 = 1 << 3;
const FD_LOAD_TOTAL_FMLA: u32 = 1 << 6;
const FD_SAVE_STYLE_NAME: u32 = 1 << 8;
const FD_LOAD_TOTAL_STR: u32 = 1 << 9;

/// One table read from its Feature record, before its range is checked against the sheet.
struct Raw {
    range: (u16, u16, u16, u16),
    id: u32,
    name: String,
    display_name: Option<String>,
    header_row: bool,
    totals_row: bool,
    auto_filter: bool,
    /// The columns read from the field data; columns past the last readable one take their
    /// names from the header row.
    columns: Vec<TableColumn>,
    style: Option<Style>,
}

struct Style {
    name: String,
    first_col: bool,
    last_col: bool,
    banded_rows: bool,
    banded_cols: bool,
}

/// The tables of the sheet being read.
#[derive(Default)]
pub(super) struct SheetTables {
    /// The Feature11/12 payload being gathered (ContinueFrt11 records may follow it).
    pending: Option<Vec<u8>>,
    tables: Vec<Raw>,
    /// Whether a table or List12 record was skipped (warned once per sheet).
    skipped: bool,
}

impl SheetTables {
    /// A Feature11 or Feature12 record.
    pub(super) fn feature(&mut self, payload: Vec<u8>) {
        self.flush();
        self.pending = Some(payload);
    }

    /// A ContinueFrt11 record: its FrtHeaderOld (rt, grbitFrt) then more of the feature data.
    pub(super) fn continue_frt(&mut self, payload: &[u8]) {
        let Some(buf) = &mut self.pending else { return };
        let more = payload.get(4..).unwrap_or(&[]);
        if buf.len().saturating_add(more.len()) > MAX_FEATURE_BYTES {
            self.pending = None;
            self.skipped = true;
            return;
        }
        buf.extend_from_slice(more);
    }

    /// A List12 record: FrtHeader (12 bytes), lsd, idList, then the structure `lsd` names.
    pub(super) fn list12(&mut self, payload: &[u8]) {
        self.flush();
        let parts = [payload];
        let mut c = Cursor::new(&parts);
        let head = (|| -> Result<(u16, u32), IoError> {
            c.skip(12)?;
            Ok((c.u16()?, c.u32()?))
        })();
        let Ok((lsd, id)) = head else {
            self.skipped = true;
            return;
        };
        // A List12 for a table that isn't there is ignored.
        let Some(t) = self.tables.iter_mut().rev().find(|t| t.id == id) else { return };
        let read = match lsd {
            // List12TableStyleClientInfo: flags (2 bytes), stListStyleName.
            1 => (|| -> Result<(), IoError> {
                let flags = c.u16()?;
                let name = c.string()?;
                t.style = Some(Style {
                    name,
                    first_col: flags & 1 != 0,
                    last_col: flags & 2 != 0,
                    banded_rows: flags & 4 != 0,
                    banded_cols: flags & 8 != 0,
                });
                Ok(())
            })(),
            // List12DisplayName: stListName (XLNameUnicodeString), stListComment.
            2 => c.string().map(|name| t.display_name = Some(name).filter(|n| !n.is_empty())),
            // List12BlockLevel: block-level formatting, not imported.
            _ => Ok(()),
        };
        self.skipped |= read.is_err();
    }

    fn flush(&mut self) {
        let Some(buf) = self.pending.take() else { return };
        if self.tables.len() >= MAX_TABLES {
            self.skipped = true;
            return;
        }
        match parse_feature(&buf) {
            Ok(t) => self.tables.push(t),
            Err(_) => self.skipped = true,
        }
    }

    /// Checks the tables against the sheet and adds the valid ones to it. Names and ids are made
    /// unique across the workbook later, by [`finish_workbook`].
    pub(super) fn finish(mut self, sheet: &mut Sheet, budget: &mut Budget, report: &mut ReadReport) -> Result<(), IoError> {
        self.flush();
        for raw in std::mem::take(&mut self.tables) {
            let Some(t) = table(raw, sheet, budget)? else {
                self.skipped = true;
                continue;
            };
            sheet.tables.push(t);
        }
        if self.skipped {
            report.warn(format!("Some tables on sheet '{}' could not be read and were kept as plain cells.", sheet.name));
        }
        Ok(())
    }
}

/// Builds the model table, or `None` when its range doesn't fit the sheet.
fn table(raw: Raw, sheet: &Sheet, budget: &mut Budget) -> Result<Option<Table>, IoError> {
    let (r0, r1, c0, c1) = raw.range;
    let (r0, r1, c0, c1) = (u32::from(r0), u32::from(r1), u32::from(c0), u32::from(c1));
    if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
        return Ok(None);
    }
    let range = RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1));
    // At least one data row besides the header and totals rows.
    if range.height() < 1 + u32::from(raw.header_row) + u32::from(raw.totals_row) {
        return Ok(None);
    }
    if sheet.tables.iter().any(|t| t.range.intersects(&range)) || sheet.merges.iter().any(|m| m.intersects(&range)) {
        return Ok(None);
    }
    let mut columns = raw.columns;
    columns.truncate(range.width() as usize);
    for i in columns.len() as u32..range.width() {
        let header =
            raw.header_row.then(|| sheet.cell(CellRef::new(r0, c0 + i))).flatten().map(|c| c.value.display()).filter(|s| !s.trim().is_empty());
        columns.push(column(header.unwrap_or_else(|| format!("Column{}", i + 1))));
    }
    for c in &columns {
        budget.text(c.name.len().saturating_add(c.totals_label.as_ref().map_or(0, String::len)))?;
    }
    let name = raw.display_name.unwrap_or(raw.name);
    budget.text(name.len())?;
    let style = raw.style;
    let mut t = Table {
        id: raw.id,
        name,
        range,
        header_row: raw.header_row,
        totals_row: raw.totals_row,
        columns,
        style: style.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
        banded_rows: style.as_ref().is_some_and(|s| s.banded_rows),
        banded_cols: style.as_ref().is_some_and(|s| s.banded_cols),
        first_col: style.as_ref().is_some_and(|s| s.first_col),
        last_col: style.as_ref().is_some_and(|s| s.last_col),
        filter_button: raw.auto_filter && raw.header_row,
        filter: vec![],
        table_type: None,
        connection_id: None,
        header_row_dxf: None,
        data_dxf: None,
        totals_row_dxf: None,
        header_row_border_dxf: None,
        table_border_dxf: None,
        totals_row_border_dxf: None,
        sort: None,
    };
    // Column names one per column, non-empty and unique, as they will be saved.
    let names = table_columns(&t);
    for (c, name) in t.columns.iter_mut().zip(names) {
        c.name = name;
    }
    Ok(Some(t))
}

/// Makes the imported tables' names valid and unique in the workbook (as saving would), and
/// gives each a unique id, keeping the file's where it can.
pub(super) fn finish_workbook(wb: &mut Workbook) {
    let mut used_ids = HashSet::new();
    let mut used_names = HashSet::new();
    let mut n = 0u32;
    for sheet in &mut wb.sheets {
        if sheet.tables.is_empty() {
            continue;
        }
        for t in &mut Arc::make_mut(sheet).tables {
            n = n.saturating_add(1);
            t.name = table_name(&t.name, &mut used_names, n);
            if t.id == 0 || !used_ids.insert(t.id) {
                t.id = 0;
            }
        }
    }
    let mut next = 1u32;
    for sheet in &mut wb.sheets {
        if !sheet.tables.iter().any(|t| t.id == 0) {
            continue;
        }
        for t in Arc::make_mut(sheet).tables.iter_mut().filter(|t| t.id == 0) {
            while used_ids.contains(&next) {
                next = next.saturating_add(1);
            }
            t.id = next;
            used_ids.insert(next);
        }
    }
}

fn column(name: String) -> TableColumn {
    TableColumn {
        name,
        totals: TotalsFn::None,
        totals_label: None,
        formula: None,
        unique_name: None,
        xml: None,
        header_row_dxf: None,
        data_dxf: None,
        totals_row_dxf: None,
    }
}

/// Skips `n` bytes after checking they are there, so a hostile size costs no time.
fn skip(c: &mut Cursor<'_, '_>, n: u32) -> Result<(), IoError> {
    let n = n as usize;
    if n > c.remaining() {
        return Err(invalid("table data runs past the record"));
    }
    c.skip(n)
}

/// Feature11/Feature12 ([MS-XLS] 2.4.114): the header, `refs2`, then TableFeatureType.
fn parse_feature(buf: &[u8]) -> Result<Raw, IoError> {
    let parts = [buf];
    let mut c = Cursor::new(&parts);
    // FrtRefHeaderU: rt, grbitFrt, ref8 (Ref8U: rwFirst, rwLast, colFirst, colLast).
    c.skip(4)?;
    let mut range = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
    if c.u16()? != ISF_LIST {
        return Err(invalid("shared feature is not a table"));
    }
    c.skip(5)?;
    let cref2 = c.u16()?;
    // cbFeatData (rgbFeat is read up to its own end) and reserved3.
    c.skip(6)?;
    if cref2 > 0 {
        range = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
        skip(&mut c, (u32::from(cref2) - 1) * 8)?;
    }
    // TableFeatureType ([MS-XLS] 2.5.266); only tables on a worksheet range (lt = 0) are kept.
    if c.u32()? != 0 {
        return Err(invalid("table bound to an external data source"));
    }
    let id = c.u32()?;
    let header_row = c.u32()? != 0;
    let totals_row = c.u32()? != 0;
    // idFieldNext, cbFSData, rupBuild, unused1.
    c.skip(12)?;
    let flags = c.u32()?;
    // lPosStmCache, cbStmCache, cchStmCache, lem, rgbHashParam.
    c.skip(32)?;
    let name = c.string()?;
    let count = c.u16()?;
    if flags & F_LOAD_CSP_NAME != 0 {
        c.string()?;
    }
    if flags & F_LOAD_ENTRY_ID != 0 {
        c.string()?;
    }
    let auto_filter = flags & F_AUTO_FILTER != 0;
    let single_cell = flags & F_SINGLE_CELL != 0;
    let mut columns = Vec::with_capacity(usize::from(count).min(c.remaining() / 40));
    for _ in 0..count {
        // A column part that isn't imported may hide the rest: those columns are named from the
        // header row instead.
        match field(&mut c, header_row, auto_filter, single_cell) {
            Ok((col, more)) => {
                columns.push(col);
                if !more {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    Ok(Raw { range, id, name, display_name: None, header_row, totals_row, auto_filter, columns, style: None })
}

/// Feat11FieldDataItem ([MS-XLS] 2.5.113) of a worksheet-range table. Returns the column and
/// whether the items after it can still be found.
fn field(c: &mut Cursor<'_, '_>, header_row: bool, auto_filter: bool, single_cell: bool) -> Result<(TableColumn, bool), IoError> {
    // idField, lfdt, lfxidt.
    c.skip(12)?;
    let ilta = c.u32()?;
    let cb_fmt_agg = c.u32()?;
    c.skip(4)?;
    let flags = c.u32()?;
    let cb_fmt_insert_row = c.u32()?;
    c.skip(4)?;
    let field_name = c.string()?;
    // strCaption is the header cell's text, so it is the column's name when present.
    let caption = if single_cell { None } else { Some(c.string()?) };
    let mut col = column(caption.filter(|s| !s.is_empty()).unwrap_or(field_name));
    col.totals = match ilta {
        1 => TotalsFn::Average,
        2 => TotalsFn::Count,
        3 => TotalsFn::CountNums,
        4 => TotalsFn::Max,
        5 => TotalsFn::Min,
        6 => TotalsFn::Sum,
        7 => TotalsFn::StdDev,
        8 => TotalsFn::Var,
        _ => TotalsFn::None,
    };
    skip(c, cb_fmt_agg)?;
    skip(c, cb_fmt_insert_row)?;
    if auto_filter {
        // Feat11FdaAutoFilter: cbAutoFilter, unused (2 bytes), recAutoFilter.
        let cb = c.u32()?;
        c.skip(2)?;
        skip(c, cb)?;
    }
    // The XML map, list formula and totals formula aren't imported, and their sizes are only
    // known by decoding them.
    if flags & (FD_LOAD_XMAPI | FD_LOAD_FMLA | FD_LOAD_TOTAL_FMLA) != 0 {
        return Ok((col, false));
    }
    if flags & FD_LOAD_TOTAL_STR != 0 {
        col.totals_label = Some(c.string()?);
    }
    // dskHdrCache (CachedDiskHeader) when the table has no header row.
    if !header_row && !single_cell {
        let cb = c.u32()?;
        skip(c, cb)?;
        if flags & FD_SAVE_STYLE_NAME != 0 {
            c.string()?;
        }
    }
    Ok((col, true))
}
