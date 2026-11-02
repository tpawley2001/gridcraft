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
//! tables), text imports (`textPr` with their fields), connection parameters given as a prompt or
//! a value, and the query tables on a sheet that use a kept connection, with their fields and
//! deleted fields. The query table's range is the hidden defined name the workbook part already
//! holds. A connection or query table holding anything else (OLAP properties, parameters bound to
//! a cell, a text import's code page given as a number, a query table's sort state…) is dropped
//! whole with a warning: a half-translated connection makes Excel repair the file. Future-record
//! extensions (Excel 2010 and later additions) are dropped. Out of scope: the spreadsheet data
//! model (Power Pivot connections and `xl/model/item.data`), connections kept in custom XML, and
//! query tables bound to a table rather than a range.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_model::{Preserved, PreservedPart, PreservedRel};

use super::{Bytes, Entry, Records, Rel, check_type};
use crate::package::Package;
use crate::write::XML_DECL;
use crate::xml::esc_attr;
use crate::{IoError, ReadReport};

const CT_CONNECTIONS_BIN: &str = "application/vnd.ms-excel.connections";
const CT_QUERY_TABLE_BIN: &str = "application/vnd.ms-excel.queryTable";
const CT_CONNECTIONS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml";
const CT_QUERY_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml";
const REL_CONNECTIONS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections";
const REL_QUERY_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

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

/// Reads `xl/connections.bin` (the workbook relationship of type `connections`).
pub(super) fn read_connections(package: &mut Package<'_>, types: &[Entry], rels: &[Rel], report: &mut ReadReport) -> Kept {
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
    let (items, dropped) = connections(&bytes, report);
    if dropped {
        report.warn(DROPPED);
    }
    if items.is_empty() {
        return kept;
    }
    let mut xml = format!("{XML_DECL}<connections xmlns=\"{NS_MAIN}\">");
    for (id, item) in items {
        kept.ids.insert(id);
        xml.push_str(&item);
    }
    xml.push_str("</connections>");
    let name = "xl/connections.xml".to_string();
    kept.preserved = Some(Arc::new(Preserved {
        rels: vec![PreservedRel { id: "rIdConnections".into(), rel_type: REL_CONNECTIONS.into(), target: name.clone(), external: false }],
        parts: vec![PreservedPart { name, content_type: CT_CONNECTIONS.into(), data: xml.into_bytes().into(), rels: vec![] }],
        ..Preserved::default()
    }));
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
        let name = format!("xl/queryTables/queryTable{}.xml", kept.query_tables);
        p.rels.push(PreservedRel { id: rel.id.clone(), rel_type: REL_QUERY_TABLE.into(), target: name.clone(), external: false });
        p.parts.push(PreservedPart { name, content_type: CT_QUERY_TABLE.into(), data: xml.into_bytes().into(), rels: vec![] });
    }
    (!p.is_empty()).then(|| Arc::new(p))
}

// ---------------------------------------------------------------- connections

/// The `<connection>` elements of the part with their ids, and whether any was dropped.
fn connections(bytes: &[u8], report: &mut ReadReport) -> (Vec<(u32, String)>, bool) {
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
                match connection(payload, &body) {
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

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn attr(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, " {name}=\"{}\"", esc_attr(value));
}
fn flag(out: &mut String, name: &str, on: bool) {
    let _ = write!(out, " {name}=\"{}\"", u8::from(on));
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
fn connection(payload: &[u8], body: &[(u16, &[u8])]) -> Res<(u32, String)> {
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
            BEGIN_PARAMS if params.is_none() && (db.is_some() || web.is_some()) => params = Some(parameters(p, &mut it)?),
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

    let mut x = format!("<connection id=\"{id}\"");
    if let Some(s) = &data_file {
        attr(&mut x, "sourceFile", s);
    }
    if let Some(s) = &conn_file {
        attr(&mut x, "odcFile", s);
    }
    if ty == DB_OLEDB && f1 & 0x01 != 0 {
        flag(&mut x, "keepAlive", true);
    }
    if interval != 0 {
        let _ = write!(x, " interval=\"{interval}\"");
    }
    attr(&mut x, "name", &name);
    if let Some(s) = &desc {
        attr(&mut x, "description", s);
    }
    let _ = write!(x, " type=\"{ty}\"");
    let always_use_file = db_like && f1 & 0x08 != 0;
    if ty == DB_OLEDB && !always_use_file {
        match recon {
            1 => {}
            2 | 3 => {
                let _ = write!(x, " reconnectionMethod=\"{recon}\"");
            }
            _ => return Err(Skip),
        }
    }
    let _ = write!(x, " refreshedVersion=\"{refreshed}\"");
    if min_refreshable != 0 {
        let _ = write!(x, " minRefreshableVersion=\"{min_refreshable}\"");
    }
    if db_like && pc == 0x01 {
        flag(&mut x, "savePassword", true);
    }
    for (bit, name) in [(0x02, "new"), (0x04, "deleted")] {
        if f1 & bit != 0 {
            flag(&mut x, name, true);
        }
    }
    if always_use_file {
        flag(&mut x, "onlyUseConnectionFile", true);
    }
    for (bit, name) in [(0x10, "background"), (0x20, "refreshOnLoad"), (0x40, "saveData")] {
        if f1 & bit != 0 {
            flag(&mut x, name, true);
        }
    }
    if db_like {
        match cred {
            0 => {}
            1 => attr(&mut x, "credentials", "none"),
            2 => attr(&mut x, "credentials", "stored"),
            _ => return Err(Skip),
        }
    }
    if let Some(s) = &sso {
        attr(&mut x, "singleSignOnId", s);
    }
    x.push('>');
    for child in [db, web, txt, params].into_iter().flatten() {
        x.push_str(&child);
    }
    x.push_str("</connection>");
    Ok((id, x))
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
    let mut x = String::from("<dbPr");
    attr(&mut x, "connection", &conn);
    if let Some(s) = &cmd {
        attr(&mut x, "command", s);
    }
    if let Some(s) = &cmd_svr {
        attr(&mut x, "serverCommand", s);
    }
    // CMDNULL (no command) has no XML form; SQL is the default.
    if cmd_type != 0 && cmd_type != 2 {
        let _ = write!(x, " commandType=\"{cmd_type}\"");
    }
    x.push_str("/>");
    Ok(x)
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
    let mut x = String::from("<webPr");
    for (bit, name) in
        [(8, "xml"), (9, "sourceData"), (10, "parsePre"), (11, "consecutive"), (12, "firstRow"), (13, "xl97"), (14, "textDates"), (15, "xl2000")]
    {
        if flags & (1 << bit) != 0 {
            flag(&mut x, name, true);
        }
    }
    if let Some(s) = &url {
        attr(&mut x, "url", s);
    }
    if let Some(s) = &post {
        attr(&mut x, "post", s);
    }
    if flags & (1 << 16) != 0 {
        flag(&mut x, "htmlTables", true);
    }
    match flags & 0xFF {
        0 => {}
        1 => attr(&mut x, "htmlFormat", "rtf"),
        2 => attr(&mut x, "htmlFormat", "all"),
        _ => return Err(Skip),
    }
    if let Some(s) = &edit {
        attr(&mut x, "editPage", s);
    }
    match it.next() {
        Some(&(END_WEB_PROPS, [])) => {
            x.push_str("/>");
            Ok(x)
        }
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
            let _ = write!(x, "><tables count=\"{count}\">{tables}</tables></webPr>");
            Ok(x)
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
    let mut x = String::from("<textPr");
    if !bit(21) {
        flag(&mut x, "prompt", false);
    }
    match flags & 0x03 {
        0 => attr(&mut x, "fileType", "mac"),
        1 => {}
        2 => attr(&mut x, "fileType", "dos"),
        _ => return Err(Skip),
    }
    if first_row != 1 {
        let _ = write!(x, " firstRow=\"{first_row}\"");
    }
    if !file.is_empty() {
        attr(&mut x, "sourceFile", &file);
    }
    if !bit(12) {
        flag(&mut x, "delimited", false);
    }
    // ISO-8859-1 bytes are the first 256 code points.
    for (byte, name, default) in [(decimal, "decimal", '.'), (thousands, "thousands", ',')] {
        let c = char::from(byte);
        if byte != 0 && c != default {
            if !is_xml_char(c) {
                return Err(Skip);
            }
            attr(&mut x, name, &c.to_string());
        }
    }
    if !bit(13) {
        flag(&mut x, "tab", false);
    }
    for (n, name) in [(14, "space"), (15, "comma"), (16, "semicolon"), (17, "consecutive")] {
        if bit(n) {
            flag(&mut x, name, true);
        }
    }
    match (flags >> 18) & 0x03 {
        0 => {}
        1 => attr(&mut x, "qualifier", "singleQuote"),
        _ => attr(&mut x, "qualifier", "none"),
    }
    if bit(22) {
        let c = char::from_u32(u32::from(custom)).filter(|c| is_xml_char(*c)).ok_or(Skip)?;
        attr(&mut x, "delimiter", &c.to_string());
    }
    // ECTWFLDINFOLST: the field count, then one BrtBeginECTwFldInfo per field.
    let mut b = Bytes::new(expect(it, BEGIN_FLD_INFO_LST)?);
    let count = b.u32()?;
    b.finish()?;
    let mut fields = String::new();
    let mut n = 0u32;
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
                const TYPES: [&str; 10] = ["general", "text", "MDY", "DMY", "YMD", "MYD", "DYM", "YDM", "skip", "EMD"];
                let name = usize::try_from(ty).ok().and_then(|i| TYPES.get(i)).ok_or(Skip)?;
                fields.push_str("<textField");
                if ty != 0 {
                    attr(&mut fields, "type", name);
                }
                if start != 0 {
                    let _ = write!(fields, " position=\"{start}\"");
                }
                fields.push_str("/>");
                n = n.saturating_add(1);
            }
            _ => return Err(Skip),
        }
    }
    if n != count || n == 0 {
        return Err(Skip);
    }
    expect_empty(it, END_TXT_WIZ)?;
    let _ = write!(x, "><textFields count=\"{count}\">{fields}</textFields></textPr>");
    Ok(x)
}

/// BrtBeginECParams ([MS-XLSB] 2.4.64) and its parameters, as `<parameters>`.
fn parameters(payload: &[u8], it: &mut std::slice::Iter<'_, (u16, &[u8])>) -> Res<String> {
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
                x.push_str(&parameter(p)?);
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

/// BrtBeginECParam ([MS-XLSB] 2.4.63) as `<parameter>`. Parameters read from a cell are not kept.
fn parameter(payload: &[u8]) -> Res<String> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    let sql_type = b.u16()? as i16;
    let kind = flags & 0x07;
    let (data_type, load_prompt) = match kind {
        0 => (0, b.u32()?),
        1 => (b.u32()?, 0),
        _ => return Err(Skip),
    };
    let name = text(&mut b, 255)?;
    let mut x = String::from("<parameter");
    attr(&mut x, "name", &name);
    if sql_type != 0 {
        let _ = write!(x, " sqlType=\"{sql_type}\"");
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
    let mut x = format!("{XML_DECL}<queryTable xmlns=\"{NS_MAIN}\"");
    attr(&mut x, "name", &name);
    for (n, name, default) in [
        (0, "headers", true),
        (1, "rowNumbers", false),
        (2, "disableRefresh", false),
        (3, "backgroundRefresh", true),
        (4, "firstBackgroundRefresh", false),
        (5, "refreshOnLoad", false),
    ] {
        if bit(n) != default {
            flag(&mut x, name, bit(n));
        }
    }
    let grow = match (bit(6), bit(7)) {
        (false, true) => Some("overwriteClear"),
        (false, false) => Some("insertClear"),
        (true, false) => None,
        (true, true) => return Err(Skip),
    };
    if let Some(g) = grow {
        attr(&mut x, "growShrinkType", g);
    }
    if bit(8) {
        flag(&mut x, "fillFormulas", true);
    }
    if !bit(9) {
        flag(&mut x, "removeDataOnSave", true);
    }
    if bit(10) {
        flag(&mut x, "disableEdit", true);
    }
    if !bit(11) {
        flag(&mut x, "preserveFormatting", false);
    }
    if !bit(12) {
        flag(&mut x, "adjustColumnWidth", false);
    }
    if bit(13) {
        flag(&mut x, "intermediate", true);
    }
    let _ = write!(x, " connectionId=\"{connection}\" autoFormatId=\"{auto_format}\"");
    for (n, name) in [
        (14, "applyNumberFormats"),
        (17, "applyBorderFormats"),
        (15, "applyFontFormats"),
        (18, "applyPatternFormats"),
        (16, "applyAlignmentFormats"),
        (19, "applyWidthHeightFormats"),
    ] {
        flag(&mut x, name, bit(n));
    }
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
    match refresh {
        Some(r) => {
            x.push('>');
            x.push_str(&r);
            x.push_str("</queryTable>");
        }
        None => x.push_str("/>"),
    }
    Ok((connection, x))
}

/// BrtBeginQSIR ([MS-XLSB] 2.4.191) and its fields and deleted fields, as `<queryTableRefresh>`.
fn query_refresh(payload: &[u8], it: &mut std::slice::Iter<'_, (u16, &[u8])>) -> Res<String> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    let next_id = b.u32()?;
    let left = b.u16()?;
    let right = b.u16()?;
    b.finish()?;
    if next_id > 65_535 || left >= 16_384 || right >= 16_384 {
        return Err(Skip);
    }
    let mut x = format!("<queryTableRefresh nextId=\"{next_id}\"");
    if flags & 0x01 == 0 {
        flag(&mut x, "preserveSortFilterLayout", false);
    }
    if flags & 0x08 != 0 {
        flag(&mut x, "fieldIdWrapped", true);
    }
    if flags & 0x10 == 0 {
        flag(&mut x, "headersInLastRefresh", false);
    }
    let min_version = (flags >> 5) & 0x1F;
    if min_version != 0 {
        let _ = write!(x, " minimumVersion=\"{min_version}\"");
    }
    if left != 0 {
        let _ = write!(x, " unboundColumnsLeft=\"{left}\"");
    }
    if right != 0 {
        let _ = write!(x, " unboundColumnsRight=\"{right}\"");
    }
    x.push('>');
    // QSIFS: the column count, then BrtBeginQSIF / BrtEndQSIF per field.
    let mut b = Bytes::new(expect(it, BEGIN_QSIFS)?);
    let count = b.u32()?;
    b.finish()?;
    let mut fields = String::new();
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
                let _ = write!(fields, "<queryTableField id=\"{id}\"");
                if let Some(s) = &name {
                    attr(&mut fields, "name", s);
                }
                if f & 0x01 != 0 {
                    flag(&mut fields, "dataBound", false);
                }
                for (bit, name) in [(0x04, "rowNumbers"), (0x02, "fillFormulas"), (0x08, "clipped")] {
                    if f & bit != 0 {
                        flag(&mut fields, name, true);
                    }
                }
                if table_column != 0 {
                    let _ = write!(fields, " tableColumnId=\"{table_column}\"");
                }
                fields.push_str("/>");
            }
            _ => return Err(Skip),
        }
    }
    if usize::try_from(count).ok() != Some(ids.len()) {
        return Err(Skip);
    }
    let _ = write!(x, "<queryTableFields count=\"{count}\">{fields}</queryTableFields>");
    loop {
        let Some(&(rid, p)) = it.next() else { return Err(Skip) };
        match rid {
            END_QSIR if p.is_empty() => break,
            BEGIN_DELETED_NAMES if p.is_empty() => {
                let mut deleted = String::new();
                let mut n = 0u32;
                loop {
                    let Some(&(rid, p)) = it.next() else { return Err(Skip) };
                    match rid {
                        END_DELETED_NAMES if p.is_empty() => break,
                        BEGIN_DELETED_NAME => {
                            let mut b = Bytes::new(p);
                            let name = text(&mut b, 255)?;
                            b.finish()?;
                            expect_empty(it, END_DELETED_NAME)?;
                            deleted.push_str("<deletedField");
                            attr(&mut deleted, "name", &name);
                            deleted.push_str("/>");
                            n = n.saturating_add(1);
                        }
                        _ => return Err(Skip),
                    }
                }
                if n == 0 {
                    return Err(Skip);
                }
                let _ = write!(x, "<queryTableDeletedFields count=\"{n}\">{deleted}</queryTableDeletedFields>");
            }
            // A sort state and anything else: not mapped.
            _ => return Err(Skip),
        }
    }
    x.push_str("</queryTableRefresh>");
    Ok(x)
}
