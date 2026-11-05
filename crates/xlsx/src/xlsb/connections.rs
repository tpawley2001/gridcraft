//! XLSB data connections and query tables, kept so that an .xlsb saved as .xlsx can still be
//! refreshed in Excel. Implemented from the public [MS-XLSB] specification: the External Data
//! Connections part (section 2.1.7.24, `xl/connections.bin`) and the Query Table part (section
//! 2.1.7.42, `xl/queryTables/queryTableN.bin`).
//!
//! Nothing is modelled: each part is translated to its SpreadsheetML equivalent (ECMA-376 Part 1
//! §18.13 `connections`, §18.12 `queryTable`) and handed to the same pass-through store the .xlsx
//! reader fills, so the writer emits them unchanged. Connections are never opened, followed or
//! refreshed.
//!
//! Kept: ODBC, OLE DB, DAO and ADO connections (`dbPr`), web queries (`webPr` with their HTML
//! tables), text imports (`textPr` with their fields), connection parameters given as a prompt, a
//! value or a cell, and the query tables on a sheet that use a kept connection, with their fields
//! and deleted fields. The query table's range is the hidden defined name the workbook part
//! already holds. A cell parameter's formula is decoded with the workbook's BIFF12 formula
//! decoder: a single cell of this workbook (ODBC and OLE DB) or a single row or column of it
//! (web queries) becomes the `cell` attribute. A connection or query table holding anything else
//! (OLAP properties, a cell parameter bound to a range, another workbook or a formula, a text
//! import's code page given as a number, a query table's sort state…) is dropped whole with a
//! warning: a half-translated connection makes Excel repair the file. Future-record extensions
//! (Excel 2010 and later additions) are dropped. Out of scope: the spreadsheet data model (Power
//! Pivot connections and `xl/model/item.data`), connections kept in custom XML, and query tables
//! bound to a table rather than a range.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::{CellRef, col_to_letters, quote_sheet};
use gridcraft_formula::{Expr, RefKind, SheetSel};
use gridcraft_model::Preserved;

use super::{Bytes, Entry, Records, Rel, check_type};
use crate::package::Package;
use crate::passthrough::connections::{
    Connection, DbPr, QueryTable, QueryTableField, QueryTableRefresh, TextPr, WebPr, add_query_table_part, attr, connections_part, flag, is_xml_char,
    text_field_type,
};
use crate::xls::formula::{Links, Mode, decode_xlsb};
use crate::{IoError, ReadReport};

const CT_CONNECTIONS_BIN: &str = "application/vnd.ms-excel.connections";
const CT_QUERY_TABLE_BIN: &str = "application/vnd.ms-excel.queryTable";

const DROPPED: &str = "Some XLSB data connections could not be kept and were dropped; the rest are kept when saving as .xlsx.";
const DROPPED_QUERY: &str = "Some XLSB query tables could not be kept and were dropped; the rest are kept when saving as .xlsx.";

/// Largest connections or query table part read.
const MAX_PART: u64 = 16 * 1024 * 1024;
/// Most connections kept for one workbook, and query tables for one sheet.
const MAX_CONNECTIONS: usize = 4096;
const MAX_QUERY_TABLES: usize = 1024;
/// Most child records (fields, parameters, tables) in one connection or query table.
const MAX_ITEMS: usize = 65_536;

// [MS-XLSB] 2.3 record types.
const BEGIN_EXT_CONNECTIONS: u16 = 429;
const END_EXT_CONNECTIONS: u16 = 430;
const BEGIN_EXT_CONNECTION: u16 = 201;
const END_EXT_CONNECTION: u16 = 202;
const BEGIN_DB_PROPS: u16 = 203;
const END_DB_PROPS: u16 = 204;
const BEGIN_WEB_PROPS: u16 = 261;
const END_WEB_PROPS: u16 = 262;
const BEGIN_WP_TABLES: u16 = 263;
const END_WP_TABLES: u16 = 264;
const BEGIN_PARAMS: u16 = 265;
const END_PARAMS: u16 = 266;
const BEGIN_PARAM: u16 = 267;
const END_PARAM: u16 = 268;
const BEGIN_TXT_WIZ: u16 = 538;
const END_TXT_WIZ: u16 = 539;
const BEGIN_FLD_INFO_LST: u16 = 540;
const END_FLD_INFO_LST: u16 = 541;
const BEGIN_FLD_INFO: u16 = 542;
const PCDI_MISSING: u16 = 20;
const PCDI_STRING: u16 = 24;
const PCDI_INDEX: u16 = 26;
const BEGIN_QSI: u16 = 447;
const END_QSI: u16 = 448;
const BEGIN_QSIR: u16 = 449;
const END_QSIR: u16 = 450;
const BEGIN_DELETED_NAMES: u16 = 451;
const END_DELETED_NAMES: u16 = 452;
const BEGIN_DELETED_NAME: u16 = 453;
const END_DELETED_NAME: u16 = 454;
const BEGIN_QSIFS: u16 = 455;
const END_QSIFS: u16 = 456;
const BEGIN_QSIF: u16 = 457;
const END_QSIF: u16 = 458;
const QSI15: u16 = 2067;

// DBType ([MS-XLSB] 2.5.31), the same numbers as the `type` attribute.
const DB_ODBC: u32 = 1;
const DB_WEB: u32 = 4;
const DB_OLEDB: u32 = 5;
const DB_TEXT: u32 = 6;

/// A connection or query table that is not kept: malformed, or holding something not mapped.
struct Skip;
impl From<IoError> for Skip {
    fn from(_: IoError) -> Self {
        Skip
    }
}
type Res<T> = Result<T, Skip>;

/// The workbook's data connections: the kept `xl/connections.xml` (as the workbook's pass-through
/// content) and the ids of the connections in it.
pub(super) struct Kept {
    pub preserved: Option<Arc<Preserved>>,
    pub ids: HashSet<u32>,
    /// Query table parts named so far (`queryTableN.xml`).
    query_tables: u32,
}

/// Reads `xl/connections.bin` (the workbook relationship of type `connections`). `links` are the
/// workbook's sheet and link tables, which a cell parameter's formula resolves through.
pub(super) fn read_connections(package: &mut Package<'_>, types: &[Entry], rels: &[Rel], links: &Links, report: &mut ReadReport) -> Kept {
    let mut kept = Kept { preserved: None, ids: HashSet::new(), query_tables: 0 };
    let Some(rel) = rels.iter().find(|r| r.kind == "connections") else { return kept };
    let bytes = if rel.external || check_type(types, &rel.target, CT_CONNECTIONS_BIN).is_err() {
        None
    } else {
        package.read_limited(&rel.target, MAX_PART).ok().flatten()
    };
    let Some(bytes) = bytes else {
        report.warn(DROPPED);
        return kept;
    };
    let (items, dropped) = connections(&bytes, links, report);
    if dropped {
        report.warn(DROPPED);
    }
    kept.ids = items.iter().map(|(id, _)| *id).collect();
    let xml: Vec<&str> = items.iter().map(|(_, item)| item.as_str()).collect();
    kept.preserved = connections_part(&xml);
    kept
}

/// Reads the query tables of a worksheet (`sheet_rels`) that use a kept connection.
pub(super) fn read_query_tables(
    package: &mut Package<'_>,
    types: &[Entry],
    sheet_rels: &[Rel],
    kept: &mut Kept,
    report: &mut ReadReport,
) -> Option<Arc<Preserved>> {
    let mut p = Preserved::default();
    for rel in sheet_rels.iter().filter(|r| r.kind == "queryTable") {
        if p.parts.len() >= MAX_QUERY_TABLES {
            report.warn(DROPPED_QUERY);
            break;
        }
        let bytes = if rel.external || check_type(types, &rel.target, CT_QUERY_TABLE_BIN).is_err() {
            None
        } else {
            package.read_limited(&rel.target, MAX_PART).ok().flatten()
        };
        let table = bytes.and_then(|b| query_table(&b, report).ok());
        // A query table whose connection was dropped would make Excel repair the file.
        let Some((_, xml)) = table.filter(|(c, _)| kept.ids.contains(c)) else {
            report.warn(DROPPED_QUERY);
            continue;
        };
        kept.query_tables = kept.query_tables.saturating_add(1);
        add_query_table_part(&mut p, rel.id.clone(), kept.query_tables, xml);
    }
    (!p.is_empty()).then(|| Arc::new(p))
}

// ---------------------------------------------------------------- connections

/// The `<connection>` elements of the part with their ids, and whether any was dropped.
fn connections(bytes: &[u8], links: &Links, report: &mut ReadReport) -> (Vec<(u32, String)>, bool) {
    let mut out: Vec<(u32, String)> = vec![];
    let mut dropped = false;
    let mut records = Records::new(bytes);
    if records.first(BEGIN_EXT_CONNECTIONS, report).is_err() {
        return (out, true);
    }
    loop {
        let next = match records.next(report) {
            Ok(Some(r)) => r,
            Ok(None) | Err(_) => {
                // A truncated part: what was read completely is kept.
                return (out, true);
            }
        };
        match next {
            (END_EXT_CONNECTIONS, _) => break,
            (BEGIN_EXT_CONNECTION, payload) => {
                let mut body = vec![];
                loop {
                    match records.next(report) {
                        Ok(Some((END_EXT_CONNECTION, _))) => break,
                        Ok(Some(r)) if body.len() < MAX_ITEMS => body.push(r),
                        Ok(Some(_)) => dropped = true,
                        Ok(None) | Err(_) => return (out, true),
                    }
                }
                if out.len() >= MAX_CONNECTIONS {
                    dropped = true;
                    continue;
                }
                match connection(payload, &body, links) {
                    Ok((id, xml)) if !out.iter().any(|(i, _)| *i == id) => out.push((id, xml)),
                    _ => dropped = true,
                }
            }
            _ => dropped = true,
        }
    }
    (out, dropped)
}

/// Text from a record, refused when XML can't carry it unchanged.
fn text(b: &mut Bytes<'_>, max: u32) -> Res<String> {
    let s = b.text(max)?;
    if !s.chars().all(is_xml_char) {
        return Err(Skip);
    }
    Ok(s)
}

/// The next record, which must be `id`.
fn expect<'a>(it: &mut std::slice::Iter<'_, (u16, &'a [u8])>, id: u16) -> Res<&'a [u8]> {
    match it.next() {
        Some(&(got, payload)) if got == id => Ok(payload),
        _ => Err(Skip),
    }
}
fn expect_empty(it: &mut std::slice::Iter<'_, (u16, &[u8])>, id: u16) -> Res<()> {
    if expect(it, id)?.is_empty() { Ok(()) } else { Err(Skip) }
}

/// One BrtBeginExtConnection ([MS-XLSB] 2.4.80) and its records, as a `<connection>`.
fn connection(payload: &[u8], body: &[(u16, &[u8])], links: &Links) -> Res<(u32, String)> {
    let mut b = Bytes::new(payload);
    let refreshed = b.u8()?;
    let min_refreshable = b.u8()?;
    let pc = b.u8()?;
    b.u8()?;
    let interval = b.u16()?;
    let f1 = b.u16()?;
    let f2 = b.u16()?;
    let ty = b.u32()?;
    let recon = b.u32()?;
    let id = b.u32()?;
    let cred = b.u8()?;
    let data_file = if f2 & 0x01 != 0 { Some(text(&mut b, 255)?) } else { None };
    let conn_file = if f2 & 0x02 != 0 { Some(text(&mut b, 255)?) } else { None };
    let desc = if f2 & 0x04 != 0 { Some(text(&mut b, 255)?) } else { None };
    let name = text(&mut b, 255)?;
    let sso = if f2 & 0x10 != 0 { Some(text(&mut b, 255)?) } else { None };
    b.finish()?;
    // The spreadsheet data model's types (100 and up) are out of scope.
    if id == 0 || name.is_empty() || !matches!(ty, 1 | 2 | 4 | 5 | 6 | 7) || interval >= 32_768 {
        return Err(Skip);
    }
    let db_like = matches!(ty, DB_ODBC | DB_OLEDB);

    let (mut db, mut web, mut txt, mut params) = (None, None, None, None);
    let mut it = body.iter();
    while let Some(&(rid, p)) = it.next() {
        match rid {
            BEGIN_DB_PROPS if db.is_none() && web.is_none() && txt.is_none() => {
                db = Some(db_props(p)?);
                expect_empty(&mut it, END_DB_PROPS)?;
            }
            BEGIN_WEB_PROPS if db.is_none() && web.is_none() && txt.is_none() => web = Some(web_props(p, &mut it)?),
            BEGIN_TXT_WIZ if db.is_none() && web.is_none() && txt.is_none() => txt = Some(text_props(p, &mut it)?),
            BEGIN_PARAMS if params.is_none() && (db.is_some() || web.is_some()) => params = Some(parameters(p, &mut it, ty, links)?),
            _ => return Err(Skip),
        }
    }
    let consistent = match ty {
        DB_ODBC | DB_OLEDB => db.is_some(),
        DB_WEB => web.is_some(),
        DB_TEXT => txt.is_some(),
        _ => web.is_none() && txt.is_none(),
    };
    if !consistent {
        return Err(Skip);
    }

    let always_use_file = db_like && f1 & 0x08 != 0;
    let reconnection_method = match recon {
        _ if ty != DB_OLEDB || always_use_file => None,
        1 => None,
        2 | 3 => Some(recon),
        _ => return Err(Skip),
    };
    let credentials = match cred {
        _ if !db_like => None,
        0 => None,
        1 => Some("none"),
        2 => Some("stored"),
        _ => return Err(Skip),
    };
    let c = Connection {
        id,
        name,
        ty,
        description: desc,
        source_file: data_file,
        odc_file: conn_file,
        keep_alive: ty == DB_OLEDB && f1 & 0x01 != 0,
        interval,
        reconnection_method,
        refreshed_version: refreshed,
        min_refreshable_version: min_refreshable,
        save_password: db_like && pc == 0x01,
        new: f1 & 0x02 != 0,
        deleted: f1 & 0x04 != 0,
        only_use_connection_file: always_use_file,
        background: f1 & 0x10 != 0,
        refresh_on_load: f1 & 0x20 != 0,
        save_data: f1 & 0x40 != 0,
        credentials,
        single_sign_on_id: sso,
        children: [db, web, txt, params].into_iter().flatten().collect(),
    };
    Ok((id, c.xml()))
}

/// BrtBeginECDbProps ([MS-XLSB] 2.4.61) as `<dbPr>`.
fn db_props(payload: &[u8]) -> Res<String> {
    let mut b = Bytes::new(payload);
    let cmd_type = b.u32()?;
    let flags = b.u8()?;
    let conn = text(&mut b, 65_535)?;
    let cmd = if flags & 0x02 != 0 { Some(text(&mut b, u32::MAX >> 1)?) } else { None };
    let cmd_svr = if flags & 0x01 != 0 { Some(text(&mut b, u32::MAX >> 1)?) } else { None };
    b.finish()?;
    if cmd_type > 5 {
        return Err(Skip);
    }
    // CMDNULL (no command) has no XML form; SQL is the default.
    let command_type = (cmd_type != 0 && cmd_type != 2).then_some(cmd_type);
    Ok(DbPr { connection: conn, command: cmd, server_command: cmd_svr, command_type }.xml())
}

/// BrtBeginECWebProps ([MS-XLSB] 2.4.71), its HTML tables and BrtEndECWebProps, as `<webPr>`.
fn web_props(payload: &[u8], it: &mut std::slice::Iter<'_, (u16, &[u8])>) -> Res<String> {
    let mut b = Bytes::new(payload);
    let flags = b.u32()?;
    let load = b.u8()?;
    let url = if load & 0x04 != 0 { Some(text(&mut b, u32::MAX >> 1)?) } else { None };
    let post = if load & 0x01 != 0 { Some(text(&mut b, u32::MAX >> 1)?) } else { None };
    let edit = if load & 0x02 != 0 { Some(text(&mut b, u32::MAX >> 1)?) } else { None };
    b.finish()?;
    let html_format = match flags & 0xFF {
        0 => None,
        1 => Some("rtf"),
        2 => Some("all"),
        _ => return Err(Skip),
    };
    let bit = |n: u32| flags & (1 << n) != 0;
    let mut web = WebPr {
        xml: bit(8),
        source_data: bit(9),
        parse_pre: bit(10),
        consecutive: bit(11),
        first_row: bit(12),
        xl97: bit(13),
        text_dates: bit(14),
        xl2000: bit(15),
        url,
        post,
        html_tables: bit(16),
        html_format,
        edit_page: edit,
        tables: None,
    };
    match it.next() {
        Some(&(END_WEB_PROPS, [])) => Ok(web.xml()),
        Some(&(BEGIN_WP_TABLES, p)) => {
            let mut b = Bytes::new(p);
            let count = b.u32()?;
            b.finish()?;
            let mut tables = String::new();
            let mut n = 0u32;
            loop {
                let Some(&(rid, p)) = it.next() else { return Err(Skip) };
                match rid {
                    END_WP_TABLES if p.is_empty() => break,
                    PCDI_MISSING if p.is_empty() => tables.push_str("<m/>"),
                    PCDI_STRING => {
                        let mut b = Bytes::new(p);
                        let s = text(&mut b, 255)?;
                        b.finish()?;
                        tables.push_str("<s");
                        attr(&mut tables, "v", &s);
                        tables.push_str("/>");
                    }
                    PCDI_INDEX => {
                        let mut b = Bytes::new(p);
                        let v = b.u32()?;
                        b.finish()?;
                        let _ = write!(tables, "<x v=\"{v}\"/>");
                    }
                    _ => return Err(Skip),
                }
                n = n.saturating_add(1);
            }
            if n != count {
                return Err(Skip);
            }
            expect_empty(it, END_WEB_PROPS)?;
            web.tables = Some((count, tables));
            Ok(web.xml())
        }
        _ => Err(Skip),
    }
}

/// BrtBeginECTxtWiz ([MS-XLSB] 2.4.69) with its field list and BrtEndECTxtWiz, as `<textPr>`.
fn text_props(payload: &[u8], it: &mut std::slice::Iter<'_, (u16, &[u8])>) -> Res<String> {
    let mut b = Bytes::new(payload);
    // ECTxtWizData ([MS-XLSB] 2.5.41).
    let flags = b.u32()?;
    let custom = b.u16()?;
    let first_row = b.u32()?;
    let decimal = b.u8()?;
    let thousands = b.u8()?;
    let file = text(&mut b, 218)?;
    b.finish()?;
    // A code page given by number has no documented mapping to the `codePage` attribute.
    if (flags >> 2) & 0x3FF >= 3 || first_row == 0 || first_row >= 100_000_000 {
        return Err(Skip);
    }
    let bit = |n: u32| flags & (1 << n) != 0;
    let file_type = match flags & 0x03 {
        0 => Some("mac"),
        1 => None,
        2 => Some("dos"),
        _ => return Err(Skip),
    };
    // ISO-8859-1 bytes are the first 256 code points.
    let separator = |byte: u8, default: char| -> Res<Option<char>> {
        let c = char::from(byte);
        match byte != 0 && c != default {
            true if !is_xml_char(c) => Err(Skip),
            on => Ok(on.then_some(c)),
        }
    };
    let delimiter = match bit(22) {
        true => Some(char::from_u32(u32::from(custom)).filter(|c| is_xml_char(*c)).ok_or(Skip)?),
        false => None,
    };
    let qualifier = match (flags >> 18) & 0x03 {
        0 => None,
        1 => Some("singleQuote"),
        _ => Some("none"),
    };
    // ECTWFLDINFOLST: the field count, then one BrtBeginECTwFldInfo per field.
    let mut b = Bytes::new(expect(it, BEGIN_FLD_INFO_LST)?);
    let count = b.u32()?;
    b.finish()?;
    let mut fields = vec![];
    loop {
        let Some(&(rid, p)) = it.next() else { return Err(Skip) };
        match rid {
            END_FLD_INFO_LST if p.is_empty() => break,
            BEGIN_FLD_INFO => {
                // ECTwFldInfoData ([MS-XLSB] 2.5.40).
                let mut b = Bytes::new(p);
                let ty = b.u32()?;
                let start = b.u32()?;
                b.finish()?;
                fields.push((text_field_type(ty).ok_or(Skip)?, start));
            }
            _ => return Err(Skip),
        }
    }
    if usize::try_from(count).ok() != Some(fields.len()) || fields.is_empty() {
        return Err(Skip);
    }
    expect_empty(it, END_TXT_WIZ)?;
    let t = TextPr {
        prompt: bit(21),
        file_type,
        first_row,
        source_file: file,
        delimited: bit(12),
        decimal: separator(decimal, '.')?,
        thousands: separator(thousands, ',')?,
        tab: bit(13),
        space: bit(14),
        comma: bit(15),
        semicolon: bit(16),
        consecutive: bit(17),
        qualifier,
        delimiter,
        fields,
    };
    Ok(t.xml())
}

/// BrtBeginECParams ([MS-XLSB] 2.4.64) and its parameters, as `<parameters>`. `ty` is the
/// connection's DBType, which decides the shape a cell parameter may have.
fn parameters(payload: &[u8], it: &mut std::slice::Iter<'_, (u16, &[u8])>, ty: u32, links: &Links) -> Res<String> {
    let mut b = Bytes::new(payload);
    let count = b.u32()?;
    b.finish()?;
    let mut x = String::new();
    let mut n = 0u32;
    loop {
        let Some(&(rid, p)) = it.next() else { return Err(Skip) };
        match rid {
            END_PARAMS if p.is_empty() => break,
            BEGIN_PARAM => {
                x.push_str(&parameter(p, ty, links)?);
                expect_empty(it, END_PARAM)?;
                n = n.saturating_add(1);
            }
            _ => return Err(Skip),
        }
    }
    if n != count || n == 0 {
        return Err(Skip);
    }
    Ok(format!("<parameters count=\"{count}\">{x}</parameters>"))
}

/// BrtBeginECParam ([MS-XLSB] 2.4.63) as `<parameter>`: a prompt (`pbt` 0), a value (`pbt` 1) or
/// a cell (`pbt` 2).
fn parameter(payload: &[u8], ty: u32, links: &Links) -> Res<String> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    let sql_type = b.u16()? as i16;
    let kind = flags & 0x07;
    let (data_type, load_prompt) = match kind {
        0 => (0, b.u32()?),
        1 => (b.u32()?, 0),
        // The data type is present but not used: the cell's value is read at refresh.
        2 => (b.u32()?, 0),
        _ => return Err(Skip),
    };
    let name = text(&mut b, 255)?;
    let mut x = String::from("<parameter");
    attr(&mut x, "name", &name);
    if sql_type != 0 {
        let _ = write!(x, " sqlType=\"{sql_type}\"");
    }
    if kind == 2 {
        let cce = b.u32()?;
        if !(1..=16_384).contains(&cce) {
            return Err(Skip);
        }
        let rgce = b.take(cce as usize)?;
        let cb = b.u32()?;
        let rgcb = b.take(usize::try_from(cb).map_err(|_| Skip)?)?;
        b.finish()?;
        let cell = cell_reference(rgce, rgcb, ty, links)?;
        attr(&mut x, "parameterType", "cell");
        attr(&mut x, "cell", &cell);
        flag(&mut x, "refreshOnChange", flags & 0x08 != 0);
        x.push_str("/>");
        return Ok(x);
    }
    if kind == 0 {
        if load_prompt == 1 {
            let prompt = text(&mut b, 65_535)?;
            attr(&mut x, "prompt", &prompt);
        }
    } else {
        attr(&mut x, "parameterType", "value");
        match data_type {
            0x01 => {
                let _ = write!(x, " double=\"{}\"", b.f64()?);
            }
            0x02 => {
                let s = text(&mut b, 255)?;
                attr(&mut x, "string", &s);
            }
            0x04 => {
                let v = b.u8()?;
                flag(&mut x, "boolean", v != 0);
            }
            0x800 | 0x8000 => {
                let v = b.f64()?;
                if v.fract() != 0.0 || v < f64::from(i32::MIN) || v > f64::from(i32::MAX) {
                    return Err(Skip);
                }
                let _ = write!(x, " integer=\"{}\"", v as i32);
            }
            _ => return Err(Skip),
        }
    }
    b.finish()?;
    x.push_str("/>");
    Ok(x)
}

/// A cell parameter's ParameterParsedFormula ([MS-XLSB] 2.5.98.14) as the `cell` attribute
/// (ECMA-376 Part 1 §18.13.6): a reference to this workbook without the leading `=`. ODBC and
/// OLE DB take one cell; a web query takes one row or one column. Anything else (another
/// workbook, a deleted sheet, a formula) refuses the whole connection.
fn cell_reference(rgce: &[u8], rgcb: &[u8], ty: u32, links: &Links) -> Res<String> {
    let Expr::Ref(r) = decode_xlsb(rgce, rgcb, CellRef::new(0, 0), Mode::Cell, links).map_err(|_| Skip)? else {
        return Err(Skip);
    };
    let SheetSel::Named(sheet) = &r.sheet else { return Err(Skip) };
    let cell = |a: &gridcraft_formula::Anchor| format!("${}${}", col_to_letters(a.col), a.row as u64 + 1);
    let body = match &r.kind {
        RefKind::Cell(a) => cell(a),
        // A one-cell area is a single cell; a web query also takes one row or one column.
        RefKind::Range(a, b) if a.row == b.row && a.col == b.col => cell(a),
        RefKind::Range(a, b) if ty == DB_WEB && (a.row == b.row || a.col == b.col) => format!("{}:{}", cell(a), cell(b)),
        RefKind::Cols(a, _, b, _) if ty == DB_WEB && a == b => format!("${}:${}", col_to_letters(*a), col_to_letters(*b)),
        RefKind::Rows(a, _, b, _) if ty == DB_WEB && a == b => format!("${}:${}", a + 1, b + 1),
        _ => return Err(Skip),
    };
    Ok(format!("{}!{body}", quote_sheet(sheet)))
}

// ---------------------------------------------------------------- query tables

/// A Query Table part as `(connection id, <queryTable> XML)`.
fn query_table(bytes: &[u8], report: &mut ReadReport) -> Res<(u32, String)> {
    let mut records = Records::new(bytes);
    let mut body = vec![];
    while let Some(r) = records.next(report)? {
        if body.len() >= MAX_ITEMS {
            return Err(Skip);
        }
        body.push(r);
    }
    let mut it = body.iter();
    let qsi = expect(&mut it, BEGIN_QSI)?;
    let mut b = Bytes::new(qsi);
    let flags = b.u32()?;
    let auto_format = b.u16()?;
    let connection = b.u32()?;
    let name = text(&mut b, 255)?;
    b.finish()?;
    if connection == 0 || name.is_empty() || auto_format > 0x14 {
        return Err(Skip);
    }
    let bit = |n: u32| flags & (1 << n) != 0;
    let grow_shrink = match (bit(6), bit(7)) {
        (false, true) => Some("overwriteClear"),
        (false, false) => Some("insertClear"),
        (true, false) => None,
        (true, true) => return Err(Skip),
    };
    let mut refresh = None;
    let mut ended = false;
    while let Some(&(rid, p)) = it.next() {
        match rid {
            BEGIN_QSIR if refresh.is_none() && !ended => refresh = Some(query_refresh(p, &mut it)?),
            // Excel 2013 additions (an extension element): not kept.
            QSI15 if !ended => {}
            END_QSI if p.is_empty() && !ended => ended = true,
            _ => return Err(Skip),
        }
    }
    if !ended {
        return Err(Skip);
    }
    let q = QueryTable {
        name,
        headers: bit(0),
        row_numbers: bit(1),
        disable_refresh: bit(2),
        background_refresh: bit(3),
        first_background_refresh: bit(4),
        refresh_on_load: bit(5),
        grow_shrink,
        fill_formulas: bit(8),
        remove_data_on_save: !bit(9),
        disable_edit: bit(10),
        preserve_formatting: bit(11),
        adjust_column_width: bit(12),
        intermediate: bit(13),
        connection_id: connection,
        auto_format_id: auto_format,
        apply_number: bit(14),
        apply_border: bit(17),
        apply_font: bit(15),
        apply_pattern: bit(18),
        apply_alignment: bit(16),
        apply_width_height: Some(bit(19)),
        refresh,
    };
    Ok((connection, q.xml()))
}

/// BrtBeginQSIR ([MS-XLSB] 2.4.191) and its fields and deleted fields.
fn query_refresh(payload: &[u8], it: &mut std::slice::Iter<'_, (u16, &[u8])>) -> Res<QueryTableRefresh> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    let next_id = b.u32()?;
    let left = b.u16()?;
    let right = b.u16()?;
    b.finish()?;
    if next_id > 65_535 || left >= 16_384 || right >= 16_384 {
        return Err(Skip);
    }
    // QSIFS: the column count, then BrtBeginQSIF / BrtEndQSIF per field.
    let mut b = Bytes::new(expect(it, BEGIN_QSIFS)?);
    let count = b.u32()?;
    b.finish()?;
    let mut fields: Vec<QueryTableField> = vec![];
    let mut ids = HashSet::new();
    loop {
        let Some(&(rid, p)) = it.next() else { return Err(Skip) };
        match rid {
            END_QSIFS if p.is_empty() => break,
            BEGIN_QSIF => {
                let mut b = Bytes::new(p);
                let f = b.u32()?;
                let id = b.u32()?;
                let table_column = b.u32()?;
                let name = if f & 0x10 != 0 { Some(text(&mut b, 255)?) } else { None };
                b.finish()?;
                expect_empty(it, END_QSIF)?;
                if !ids.insert(id) {
                    return Err(Skip);
                }
                fields.push(QueryTableField {
                    id,
                    name,
                    data_bound: f & 0x01 == 0,
                    row_numbers: f & 0x04 != 0,
                    fill_formulas: f & 0x02 != 0,
                    clipped: f & 0x08 != 0,
                    table_column_id: table_column,
                });
            }
            _ => return Err(Skip),
        }
    }
    if usize::try_from(count).ok() != Some(fields.len()) {
        return Err(Skip);
    }
    let mut deleted = vec![];
    loop {
        let Some(&(rid, p)) = it.next() else { return Err(Skip) };
        match rid {
            END_QSIR if p.is_empty() => break,
            BEGIN_DELETED_NAMES if p.is_empty() && deleted.is_empty() => {
                loop {
                    let Some(&(rid, p)) = it.next() else { return Err(Skip) };
                    match rid {
                        END_DELETED_NAMES if p.is_empty() => break,
                        BEGIN_DELETED_NAME => {
                            let mut b = Bytes::new(p);
                            let name = text(&mut b, 255)?;
                            b.finish()?;
                            expect_empty(it, END_DELETED_NAME)?;
                            deleted.push(name);
                        }
                        _ => return Err(Skip),
                    }
                }
                if deleted.is_empty() {
                    return Err(Skip);
                }
            }
            // A sort state and anything else: not mapped.
            _ => return Err(Skip),
        }
    }
    Ok(QueryTableRefresh {
        next_id,
        preserve_sort_filter_layout: flags & 0x01 != 0,
        field_id_wrapped: flags & 0x08 != 0,
        headers_in_last_refresh: flags & 0x10 != 0,
        minimum_version: (flags >> 5) & 0x1F,
        unbound_columns_left: left,
        unbound_columns_right: right,
        fields,
        deleted,
    })
}
