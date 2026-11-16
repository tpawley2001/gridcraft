//! Query tables of Excel 97-2003 worksheets and their data connections, kept so that an .xls saved
//! as .xlsx can still be refreshed in Excel. Implemented from the public [MS-XLS] specification:
//! the QUERYTABLE production of the Worksheet Substream and its records, `Qsi`,
//! `DbOrParamQry` (a `DbQuery`) with its `SXString` segments, `QsiSXTag`, `DBQueryExt`
//! with its `OleDbConn`, `ExtString` and `TxtQry` records, and `Qsir` with its `Qsif` fields.
//!
//! Excel 97-2003 stores one connection per query table, inside the sheet. Each becomes one
//! ECMA-376 Part 1 §18.13 `connection` in the workbook's `xl/connections.xml` (ids 1..n in sheet
//! order, named after the query table) and one §18.12 `queryTable` part on its sheet, written by
//! the same code the XLSB reader uses (`passthrough::connections`). The query table's range is
//! its sheet's hidden `ExternalData_N` defined name, imported with the other names. Connections
//! are never opened, followed or refreshed.
//!
//! Kept: ODBC and OLE DB queries (`dbPr`), web queries (`webPr`) and text imports (`textPr` with
//! their fields), and the query table's options, fields and deleted fields. A query table holding
//! anything else (parameters, a list of tables to import, a saved sort, an OLAP cube, a DAO or
//! ADO source, a query table bound to a list…) is dropped with its connection, with one warning
//! per workbook: a half-translated connection makes Excel repair the file. Excel 2007 additions
//! (`SXAddl` records) are not read. Out of scope: the connections of PivotTable caches (`DConn`,
//! `SXDB`) and OLAP.

use std::collections::HashSet;
use std::sync::Arc;

use gridcraft_model::{Preserved, Workbook};

use super::records::{Cursor, Record};
use crate::passthrough::connections::{
    Connection, DbPr, QueryTable, QueryTableField, QueryTableRefresh, TextPr, WebPr, add_query_table_part, connections_part, is_xml_char,
    text_field_type,
};
use crate::passthrough::merge_preserved;
use crate::{IoError, ReadReport};

const DROPPED: &str =
    "Some .xls data connections could not be kept and were dropped with their query tables; the rest are kept when saving as .xlsx.";

/// Query tables kept for one sheet, and connections for one workbook, at most.
const MAX_QUERY_TABLES: usize = 1024;
const MAX_CONNECTIONS: usize = 4096;
/// Records in one query table block at most.
const MAX_BLOCK_RECORDS: usize = 65_536;

// [MS-XLS] 2.3 record types.
const QSI: u16 = 0x01AD;
const DB_OR_PARAM_QRY: u16 = 0x00DC;
const SX_STRING: u16 = 0x00CD;
const QSI_SX_TAG: u16 = 0x0802;
const DB_QUERY_EXT: u16 = 0x0803;
const EXT_STRING: u16 = 0x0804;
const TXT_QRY: u16 = 0x0805;
const QSIR: u16 = 0x0806;
const QSIF: u16 = 0x0807;
const OLE_DB_CONN: u16 = 0x080A;
const SX_ADDL: u16 = 0x0864;
const SORT_DATA: u16 = 0x0895;

// DataSourceType, the same numbers as the `type` attribute.
const DBT_ODBC: u16 = 1;
const DBT_WEB: u16 = 4;
const DBT_OLEDB: u16 = 5;
const DBT_TXT: u16 = 6;

/// A query table that is not kept: malformed, or holding something not mapped.
struct Skip;
impl From<IoError> for Skip {
    fn from(_: IoError) -> Self {
        Skip
    }
}
type Res<T> = Result<T, Skip>;

/// One record of a query table block, with the payloads of its `Continue` records.
type Rec = (u16, Vec<Vec<u8>>);

#[derive(Default)]
struct Block {
    records: Vec<Rec>,
    overflow: bool,
}

/// The query table blocks of one worksheet, gathered as their records while it is read.
#[derive(Default)]
pub(super) struct Blocks {
    blocks: Vec<Block>,
    open: bool,
    too_many: bool,
}

impl Blocks {
    /// Sees every record of the worksheet: a `Qsi` opens a block, the records of the QUERYTABLE
    /// production extend it, and anything else closes it.
    pub(super) fn record(&mut self, rec: &Record<'_>) {
        match rec.id {
            QSI => {
                if self.blocks.len() >= MAX_QUERY_TABLES {
                    self.too_many = true;
                    self.open = false;
                    return;
                }
                self.blocks.push(Block::default());
                self.open = true;
            }
            DB_OR_PARAM_QRY | SX_STRING | QSI_SX_TAG | DB_QUERY_EXT | EXT_STRING | TXT_QRY | OLE_DB_CONN | QSIR | QSIF | SX_ADDL | SORT_DATA
                if self.open => {}
            _ => {
                self.open = false;
                return;
            }
        }
        let Some(block) = self.blocks.last_mut() else { return };
        if block.records.len() >= MAX_BLOCK_RECORDS {
            block.overflow = true;
            return;
        }
        block.records.push((rec.id, rec.parts.iter().map(|p| p.to_vec()).collect()));
    }
}

/// The connections kept so far for the workbook.
#[derive(Default)]
pub(super) struct Kept {
    connections: Vec<String>,
    query_tables: u32,
    dropped: bool,
}

impl Kept {
    /// Translates one worksheet's query tables: their parts go to the sheet's pass-through
    /// content, their connections to the workbook's.
    pub(super) fn sheet(&mut self, blocks: Blocks, preserved: &mut Option<Arc<Preserved>>) {
        self.dropped |= blocks.too_many;
        let mut p = Preserved::default();
        for block in &blocks.blocks {
            let id = self.connections.len().saturating_add(1);
            let Some(id) = u32::try_from(id).ok().filter(|_| id <= MAX_CONNECTIONS) else {
                self.dropped = true;
                continue;
            };
            let Ok((connection, table)) = query_table(block, id) else {
                self.dropped = true;
                continue;
            };
            self.connections.push(connection.xml());
            self.query_tables = self.query_tables.saturating_add(1);
            add_query_table_part(&mut p, format!("rIdQueryTable{}", self.query_tables), self.query_tables, table.xml());
        }
        if !p.is_empty() {
            merge_preserved(preserved, Some(Arc::new(p)));
        }
    }

    /// Adds `xl/connections.xml` to the workbook's pass-through content.
    pub(super) fn finish(self, wb: &mut Workbook, report: &mut ReadReport) {
        merge_preserved(&mut wb.preserved, connections_part(&self.connections));
        if self.dropped {
            report.warn(DROPPED);
        }
    }
}

/// Reads a record's payload; `rt` is the record type an FrtHeaderOld or FrtRefHeaderU repeats.
fn cursor<'a, 'b>(parts: &'b [&'a [u8]], rt: Option<u16>) -> Res<Cursor<'a, 'b>> {
    let mut c = Cursor::new(parts);
    if let Some(rt) = rt
        && c.u16()? != rt
    {
        return Err(Skip);
    }
    if rt.is_some() {
        c.u16()?; // grbitFrt
    }
    Ok(c)
}

fn slices(parts: &[Vec<u8>]) -> Vec<&[u8]> {
    parts.iter().map(Vec::as_slice).collect()
}

/// Text refused when XML can't carry it unchanged.
fn checked(s: String) -> Res<String> {
    if s.chars().all(is_xml_char) { Ok(s) } else { Err(Skip) }
}

/// An XLUnicodeString.
fn string(c: &mut Cursor<'_, '_>) -> Res<String> {
    checked(c.string()?)
}

/// The records of a block, read in order.
struct Reader<'a> {
    records: std::iter::Peekable<std::slice::Iter<'a, Rec>>,
}

impl<'a> Reader<'a> {
    fn next(&mut self, id: u16) -> Res<&'a [Vec<u8>]> {
        match self.records.next() {
            Some((got, parts)) if *got == id => Ok(parts),
            _ => Err(Skip),
        }
    }
    fn peek(&mut self) -> Option<u16> {
        self.records.peek().map(|(id, _)| *id)
    }

    /// The string held by `count` SXString segments ([MS-XLS] 2.4.304).
    fn sx_strings(&mut self, count: i16) -> Res<String> {
        let mut out = String::new();
        for _ in 0..u16::try_from(count).map_err(|_| Skip)? {
            let parts = slices(self.next(SX_STRING)?);
            let mut c = cursor(&parts, None)?;
            let cch = c.u16()?;
            if cch != 0xFFFF {
                let high = c.u8()? & 1 != 0;
                out.push_str(&checked(c.chars(usize::from(cch), high)?)?);
            }
        }
        Ok(out)
    }

    /// The string held by `count` ExtString records ([MS-XLS] 2.4.108).
    fn ext_strings(&mut self, count: u16) -> Res<String> {
        let mut out = String::new();
        for _ in 0..count {
            let parts = slices(self.next(EXT_STRING)?);
            let mut c = cursor(&parts, Some(EXT_STRING))?;
            out.push_str(&string(&mut c)?);
        }
        Ok(out)
    }
}

/// The query table block's connection (with id `id`) and query table.
fn query_table(block: &Block, id: u32) -> Res<(Connection, QueryTable)> {
    if block.overflow {
        return Err(Skip);
    }
    let mut r = Reader { records: block.records.iter().peekable() };

    // Qsi ([MS-XLS] 2.4.208).
    let parts = slices(r.next(QSI)?);
    let mut c = cursor(&parts, None)?;
    let flags = c.u16()?;
    let auto_format = c.u16()?;
    let atr = c.u16()?;
    c.u32()?;
    let name = string(&mut c)?;
    let qbit = |n: u16| flags & (1 << n) != 0;
    let (shrink, overwrite) = (qbit(6), qbit(13));
    if name.is_empty() || name.chars().count() >= 255 || auto_format > 0x14 || (shrink && overwrite) {
        return Err(Skip);
    }

    // DbQuery and its SXString strings.
    let parts = slices(r.next(DB_OR_PARAM_QRY)?);
    let mut c = cursor(&parts, None)?;
    let grbit = c.u16()?;
    let params = c.u16()? as i16;
    let cst_query = c.u16()? as i16;
    let cst_web_post = c.u16()? as i16;
    c.u16()?; // cstSQLSav: the server-based fields' query, not mapped
    let cst_odbc_conn = c.u16()? as i16;
    let dbt = grbit & 0x07;
    let dbit = |n: u16| grbit & (1 << n) != 0;
    let (odbc_conn, sql, sql_sav, web, save_pwd, tables_only_html) = (dbit(3), dbit(4), dbit(5), dbit(6), dbit(7), dbit(8));
    // Parameters and the server-based fields' query are not mapped.
    if params != 0 || sql_sav || (sql && web) {
        return Err(Skip);
    }
    let command = if sql { Some(r.sx_strings(cst_query)?) } else { None };
    let connection_string = if odbc_conn { Some(r.sx_strings(cst_odbc_conn)?) } else { None };
    let (url, post) = if web {
        let url = r.sx_strings(cst_query)?;
        let post = r.sx_strings(cst_web_post)?;
        (Some(url), (cst_web_post > 0).then_some(post))
    } else {
        (None, None)
    };

    // QsiSXTag ([MS-XLS] 2.4.211) for this query table.
    let parts = slices(r.next(QSI_SX_TAG)?);
    let mut c = cursor(&parts, Some(QSI_SX_TAG))?;
    let fsx = c.u16()?;
    let tag_flags = c.u16()?;
    // DwQsiFuture.
    let future = c.u32()?;
    c.u32()?;
    let tag_name = string(&mut c)?;
    // A query table bound to a list (fExtDataList, fCreateQTList, fDummyList) is out of scope.
    if fsx != 0 || tag_name != name || future & 0xD0 != 0 {
        return Err(Skip);
    }
    let enable_refresh = tag_flags & 0x01 != 0;

    // DBQueryExt ([MS-XLS] 2.4.81).
    let parts = slices(r.next(DB_QUERY_EXT)?);
    let mut c = cursor(&parts, Some(DB_QUERY_EXT))?;
    let ext_dbt = c.u16()?;
    let ext_flags = c.u16()?;
    let grbit_dbt = c.u16()?;
    let ext_flags2 = c.u16()?;
    let _edited = c.u8()?;
    let refreshed = c.u8()?;
    let refreshable_min = c.u8()?;
    c.u8()?;
    c.u16()?;
    let coledb = c.u16()?;
    let _cst_future = c.u16()?;
    let interval = c.u16()?;
    let html_fmt = c.u16()?;
    let param_flags = c.u16()?;
    let ebit = |n: u16| ext_flags & (1 << n) != 0;
    let (maintain, new_query, import_xml_source, sp_list, src_is_xml) = (ebit(0), ebit(1), ebit(2), ebit(3), ebit(7));
    let (txt_wiz, table_names) = (ext_flags2 & 0x01 != 0, ext_flags2 & 0x02 != 0);
    // A list of tables to import and Web-based lists are not mapped.
    if ext_dbt != dbt || table_names || sp_list || param_flags != 0 || coledb > 4 || interval >= 32_768 || txt_wiz != (dbt == DBT_TXT) {
        return Err(Skip);
    }
    let mut ole_db: Vec<(bool, String)> = vec![];
    for _ in 0..coledb {
        // OleDbConn ([MS-XLS] 2.4.186) and its ExtString records.
        let parts = slices(r.next(OLE_DB_CONN)?);
        let mut c = cursor(&parts, Some(OLE_DB_CONN))?;
        let local = c.u16()? & 0x02 != 0;
        let cst = c.u16()?;
        if cst == 0 {
            return Err(Skip);
        }
        ole_db.push((local, r.ext_strings(cst)?));
    }
    let text = if txt_wiz { Some(text_query(&mut r)?) } else { None };

    // QSIR and anything after it.
    let mut refresh = None;
    while let Some(id) = r.peek() {
        match id {
            QSIR if refresh.is_none() => refresh = Some(query_refresh(&mut r)?),
            // Excel 2007 additions: not read.
            SX_ADDL => {
                r.records.next();
            }
            // A saved sort, and records out of order: not mapped.
            _ => return Err(Skip),
        }
    }

    let db_like = matches!(dbt, DBT_ODBC | DBT_OLEDB);
    let mut children = vec![];
    match dbt {
        DBT_ODBC => {
            let connection = connection_string.ok_or(Skip)?;
            if coledb != 0 || web {
                return Err(Skip);
            }
            children.push(DbPr { connection, command, server_command: None, command_type: None }.xml());
        }
        DBT_OLEDB => {
            // ConnGrbitDbtOledb: the command type and which string is used.
            let command_type = u32::from(grbit_dbt & 0x07);
            let use_local = grbit_dbt & 0x08 != 0;
            // An OLAP cube is out of scope.
            if command_type == 1 || command_type > 5 || odbc_conn || web {
                return Err(Skip);
            }
            let connection = ole_db.into_iter().find(|(local, _)| *local == use_local).map(|(_, s)| s).ok_or(Skip)?;
            let command_type = (command_type != 0 && command_type != 2).then_some(command_type);
            children.push(DbPr { connection, command, server_command: None, command_type }.xml());
        }
        DBT_WEB => {
            if !web || odbc_conn || coledb != 0 {
                return Err(Skip);
            }
            let html_format = match html_fmt {
                0 | 1 => None,
                2 => Some("rtf"),
                3 => Some("all"),
                _ => return Err(Skip),
            };
            // ConnGrbitDbtWeb.
            let wbit = |n: u16| grbit_dbt & (1 << n) != 0;
            let w = WebPr {
                xml: src_is_xml,
                source_data: import_xml_source,
                parse_pre: wbit(0),
                consecutive: wbit(1),
                first_row: wbit(2),
                xl97: wbit(3),
                text_dates: wbit(4),
                xl2000: wbit(5),
                url,
                post,
                html_tables: tables_only_html,
                html_format,
                ..WebPr::default()
            };
            children.push(w.xml());
        }
        DBT_TXT => {
            if web || odbc_conn || sql || coledb != 0 {
                return Err(Skip);
            }
            children.push(text.ok_or(Skip)?.xml());
        }
        // DAO and ADO sources keep their strings in forms not mapped.
        _ => return Err(Skip),
    }

    let (asynchronous, auto_refresh, save_data) = (qbit(3), qbit(5), qbit(9));
    let connection = Connection {
        id,
        name: name.clone(),
        ty: u32::from(dbt),
        keep_alive: db_like && maintain,
        interval,
        // The refreshed versions are meaningless before the first refresh.
        refreshed_version: if new_query { 0 } else { refreshed },
        min_refreshable_version: if new_query { 0 } else { refreshable_min },
        save_password: dbt == DBT_ODBC && save_pwd,
        new: new_query,
        background: asynchronous,
        refresh_on_load: auto_refresh,
        save_data,
        children,
        ..Connection::default()
    };
    let table = QueryTable {
        name,
        headers: qbit(0),
        row_numbers: qbit(1),
        disable_refresh: qbit(2) && !enable_refresh,
        background_refresh: asynchronous,
        first_background_refresh: qbit(4),
        refresh_on_load: auto_refresh,
        grow_shrink: match (shrink, overwrite) {
            (true, _) => None,
            (false, true) => Some("overwriteClear"),
            (false, false) => Some("insertClear"),
        },
        fill_formulas: qbit(7),
        remove_data_on_save: !save_data,
        disable_edit: qbit(10),
        preserve_formatting: future & 0x01 != 0,
        adjust_column_width: future & 0x02 != 0,
        intermediate: false,
        connection_id: id,
        auto_format_id: auto_format,
        apply_number: atr & 0x01 != 0,
        apply_border: atr & 0x08 != 0,
        apply_font: atr & 0x02 != 0,
        apply_pattern: atr & 0x10 != 0,
        apply_alignment: atr & 0x04 != 0,
        apply_width_height: None,
        refresh,
    };
    Ok((connection, table))
}

/// TxtQry ([MS-XLS] 2.4.330) and the ExtString records that follow it, as `<textPr>`.
fn text_query(r: &mut Reader<'_>) -> Res<TextPr> {
    let parts = slices(r.next(TXT_QRY)?);
    let mut c = cursor(&parts, Some(TXT_QRY))?;
    let flags = c.u16()?;
    c.u16()?;
    let start = c.u32()? as i32;
    let delims = c.u8()?;
    let custom = c.u16()?;
    c.u8()?;
    let count = c.u32()? as i32;
    let decimal = c.u8()?;
    let thousands = c.u8()?;
    if !(1..=256).contains(&count) || !(1..100_000_000).contains(&start) {
        return Err(Skip);
    }
    let mut fields = vec![];
    for _ in 0..count {
        // TxtWf.
        let ty = c.u32()?;
        let position = c.u32()?;
        fields.push((text_field_type(ty).ok_or(Skip)?, position));
    }
    let file = string(&mut c)?;
    // The connection string ExtStrings of a text query: the file name above is what is kept.
    while r.peek() == Some(EXT_STRING) {
        r.next(EXT_STRING)?;
    }
    let fbit = |n: u16| flags & (1 << n) != 0;
    // fFile must be set.
    if !fbit(0) {
        return Err(Skip);
    }
    // With fUseNewiCpid set and iCpidNew > 2, iCpid is unused and iCpidNew is only an optional
    // optimisation hint ([MS-XLS] 2.4.330): no code page is written, so textPr's defaults apply.
    let code_page = (flags >> 2) & 0x03;
    let new_code_page = (flags >> 5) & 0x3FF;
    let file_type = match code_page {
        _ if fbit(15) && new_code_page > 2 => None,
        0 => Some("mac"),
        1 => None,
        2 => Some("dos"),
        _ => return Err(Skip),
    };
    let dbit = |n: u8| delims & (1 << n) != 0;
    // The separators are ANSI characters (Windows-1252, whose 0x80-0x9F range is not mapped here).
    let separator = |byte: u8, default: char| -> Res<Option<char>> {
        match byte {
            0 => Ok(None),
            0x80..=0x9F => Err(Skip),
            _ => {
                let c = char::from(byte);
                if !is_xml_char(c) {
                    return Err(Skip);
                }
                Ok((c != default).then_some(c))
            }
        }
    };
    let delimiter = match dbit(4) {
        true => Some(char::from_u32(u32::from(custom)).filter(|c| is_xml_char(*c)).ok_or(Skip)?),
        false => None,
    };
    let qualifier = match (delims >> 6) & 0x03 {
        0 => None,
        1 => Some("singleQuote"),
        2 => Some("none"),
        _ => return Err(Skip),
    };
    Ok(TextPr {
        prompt: fbit(4),
        file_type,
        first_row: start as u32,
        source_file: file,
        delimited: fbit(1),
        decimal: separator(decimal, '.')?,
        thousands: separator(thousands, ',')?,
        tab: dbit(0),
        space: dbit(1),
        comma: dbit(2),
        semicolon: dbit(3),
        consecutive: dbit(5),
        qualifier,
        delimiter,
        fields,
    })
}

/// Reads `count` XLUnicodeStrings after `skip` bytes of `parts`; `None` unless they end the record.
fn titles_after(parts: &[&[u8]], skip: usize, count: u32) -> Option<Vec<String>> {
    let mut c = Cursor::new(parts);
    c.skip(skip).ok()?;
    let mut out = vec![];
    for _ in 0..count {
        out.push(checked(c.string().ok()?).ok()?);
    }
    (c.remaining() == 0).then_some(out)
}

/// Strings that may follow an optional 4-byte idList (present in files saved by later
/// versions): whichever reading ends the record exactly.
fn titles(parts: &[&[u8]], fixed: usize, count: u32) -> Res<Vec<String>> {
    titles_after(parts, fixed, count).or_else(|| titles_after(parts, fixed.saturating_add(4), count)).ok_or(Skip)
}

/// Qsir ([MS-XLS] 2.4.210) and its Qsif records ([MS-XLS] 2.4.209).
fn query_refresh(r: &mut Reader<'_>) -> Res<QueryTableRefresh> {
    let parts = slices(r.next(QSIR)?);
    let mut c = cursor(&parts, Some(QSIR))?;
    c.skip(8)?; // ref8
    c.u16()?; // cbQsirSaved
    c.u16()?; // cbQsifSaved
    let flags = c.u32()?;
    c.u32()?; // iSortCustom
    let count = c.u32()?;
    let deleted_count = c.u32()?;
    let next_id = c.u32()?;
    let left = c.u16()?;
    let right = c.u16()?;
    // frtRefHeaderU, the two sizes and the fixed fields.
    let deleted = titles(&parts, 40, deleted_count)?;
    let bit = |n: u32| flags & (1 << n) != 0;
    // A saved sort is not mapped.
    if bit(20) || !(1..=0xFFFF).contains(&next_id) || left >= 16_384 || right >= 16_384 || count > 0xFFFF {
        return Err(Skip);
    }
    let mut fields = vec![];
    let mut ids = HashSet::new();
    for _ in 0..count {
        let parts = slices(r.next(QSIF)?);
        let mut c = cursor(&parts, Some(QSIF))?;
        let f = c.u32()?;
        let id = c.u32()?;
        let mut name = titles(&parts, 12, 1)?;
        // Sort keys (iSortKey, fSorted) are not mapped.
        if f & 0x27FC != 0 || !(1..=0xFFFF).contains(&id) || !ids.insert(id) {
            return Err(Skip);
        }
        let name = name.pop().filter(|s| !s.is_empty());
        fields.push(QueryTableField {
            id,
            name,
            data_bound: f & 0x01 == 0,
            row_numbers: f & 0x0800 != 0,
            fill_formulas: f & 0x02 != 0,
            clipped: f & 0x1_0000 != 0,
            table_column_id: 0,
        });
    }
    Ok(QueryTableRefresh {
        next_id,
        preserve_sort_filter_layout: bit(0),
        field_id_wrapped: bit(23),
        headers_in_last_refresh: bit(25),
        minimum_version: ((flags >> 26) & 0x1F) as u16,
        unbound_columns_left: left,
        unbound_columns_right: right,
        fields,
        deleted,
    })
}
