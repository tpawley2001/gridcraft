//! XLSB data connections (`xl/connections.bin`) and query tables (`xl/queryTables/*.bin`), from
//! records assembled in Rust per [MS-XLSB], kept as their SpreadsheetML forms when saved as .xlsx.
use std::collections::HashMap;
use std::io::Read;

use gridcraft_model::Workbook;

use super::make_zip;
use crate::xml::{El, parse};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}

// ---------------------------------------------------------------- connections.bin

/// BrtBeginExtConnection: refreshed version 6, the given flags (fNewQuery…), `idbtype`, id and
/// name, optional data file and description.
fn ext_connection(id: u32, ty: u32, name: &str, flags: u16, data_file: Option<&str>) -> Vec<u8> {
    let mut d = vec![6, 0, 0, 0];
    d.extend(0u16.to_le_bytes()); // wInterval
    d.extend(flags.to_le_bytes());
    // fLoadSourceDataFile, reserved3 (always 1).
    d.extend((0x08u16 | u16::from(data_file.is_some())).to_le_bytes());
    d.extend(ty.to_le_bytes());
    d.extend(1u32.to_le_bytes()); // irecontype
    d.extend(id.to_le_bytes());
    d.push(0); // iCredMethod
    if let Some(f) = data_file {
        d.extend(wide(f));
    }
    d.extend(wide(name));
    record(201, &d)
}
/// BrtBeginECDbProps + BrtEndECDbProps.
fn db_props(cmd_type: u32, conn: &str, cmd: &str) -> Vec<u8> {
    let mut d = cmd_type.to_le_bytes().to_vec();
    d.push(0x02); // fLoadCmd
    d.extend(wide(conn));
    d.extend(wide(cmd));
    [record(203, &d), record(204, &[])].concat()
}
/// BrtBeginECWebProps (URL only) + BrtEndECWebProps.
fn web_props(url: &str) -> Vec<u8> {
    let mut d = (1u32 << 16).to_le_bytes().to_vec(); // fTablesOnlyHTML
    d.push(0x04); // fLoadURL
    d.extend(wide(url));
    [record(261, &d), record(262, &[])].concat()
}
/// BrtBeginECTxtWiz (Windows, delimited by commas, from row 1) with `fields` text fields.
fn text_props(file: &str, fields: u32) -> Vec<u8> {
    let flags: u32 = 1 | (1 << 12) | (1 << 15) | (1 << 20);
    let mut d = flags.to_le_bytes().to_vec();
    d.extend(0u16.to_le_bytes()); // chCustom
    d.extend(1u32.to_le_bytes()); // rowStartAt
    d.extend(*b".,");
    d.extend(wide(file));
    let mut out = record(538, &d);
    out.extend(record(540, &fields.to_le_bytes()));
    for i in 0..fields {
        let mut f = (if i == 0 { 1u32 } else { 0 }).to_le_bytes().to_vec(); // text, general
        f.extend(0u32.to_le_bytes());
        out.extend(record(542, &f));
    }
    out.extend(record(541, &[]));
    out.extend(record(539, &[]));
    out
}
fn connections_part(conns: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(429, &[]);
    for c in conns {
        out.extend(c);
        out.extend(record(202, &[]));
    }
    out.extend(record(430, &[]));
    out
}
fn odbc() -> Vec<u8> {
    [ext_connection(1, 1, "Query from Sales", 0x02 | 0x40, None), db_props(2, "DSN=Sales;", "SELECT * FROM t")].concat()
}
fn text_import() -> Vec<u8> {
    [ext_connection(2, 6, "a", 0x40, None), text_props("C:\\data\\a.csv", 2)].concat()
}
fn web_query() -> Vec<u8> {
    [ext_connection(3, 4, "Web <q> & \"t\"", 0, None), web_props("https://example.com/t")].concat()
}

// ---------------------------------------------------------------- queryTable.bin

/// A query table bound to `conn`: titles, background refresh, refresh on load, insert-delete,
/// keep data, preserve formatting, autofit; `fields` named fields.
fn query_table_part(conn: u32, name: &str, fields: &[&str]) -> Vec<u8> {
    let flags: u32 = 1 | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 9) | (1 << 11) | (1 << 12);
    let mut d = flags.to_le_bytes().to_vec();
    d.extend(16u16.to_le_bytes()); // itblAutoFmt
    d.extend(conn.to_le_bytes());
    d.extend(wide(name));
    let mut out = record(447, &d);
    let mut r = (0x01u16 | 0x02 | 0x04 | 0x10).to_le_bytes().to_vec();
    r.extend((fields.len() as u32 + 1).to_le_bytes());
    r.extend([0; 4]);
    out.extend(record(449, &r));
    out.extend(record(455, &(fields.len() as u32).to_le_bytes()));
    for (i, f) in fields.iter().enumerate() {
        let mut q = 0x10u32.to_le_bytes().to_vec(); // fIrstName
        q.extend((i as u32 + 1).to_le_bytes());
        q.extend(0u32.to_le_bytes());
        q.extend(wide(f));
        out.extend(record(457, &q));
        out.extend(record(458, &[]));
    }
    out.extend(record(456, &[]));
    out.extend(record(451, &[]));
    out.extend(record(453, &wide("Old")));
    out.extend(record(454, &[]));
    out.extend(record(452, &[]));
    out.extend(record(450, &[]));
    out.extend(record(448, &[]));
    out
}

// ---------------------------------------------------------------- package

fn worksheet() -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(145, &[]));
    let mut row = 0u32.to_le_bytes().to_vec();
    row.extend([0; 13]);
    out.extend(record(0, &row));
    let mut cell = 0u32.to_le_bytes().to_vec();
    cell.extend([0; 4]);
    cell.extend(1.0f64.to_le_bytes());
    out.extend(record(5, &cell));
    out.extend(record(146, &[]));
    out.extend(record(130, &[]));
    out
}

/// One sheet named `sheet_name`; the hidden sheet-scoped name `Sales_1` = Data!$A$1:$C$5 when
/// `name`.
fn book_sheet(name: bool, sheet_name: &str) -> Vec<u8> {
    let mut out = record(131, &[]);
    out.extend(record(143, &[]));
    let mut sheet = 0u32.to_le_bytes().to_vec();
    sheet.extend(1u32.to_le_bytes());
    sheet.extend(wide("s0"));
    sheet.extend(wide(sheet_name));
    out.extend(record(156, &sheet));
    out.extend(record(144, &[]));
    // EXTERNALS: BrtSupSelf, then one XTI for Data.
    out.extend(record(353, &[]));
    out.extend(record(357, &[]));
    let mut xti = 1u32.to_le_bytes().to_vec();
    xti.extend([0u8; 12]);
    out.extend(record(362, &xti));
    out.extend(record(354, &[]));
    if name {
        // BrtName: fHidden, scope Data, PtgArea3d over A1:C5.
        let mut rgce = vec![0x3B];
        rgce.extend(0u16.to_le_bytes());
        rgce.extend(0u32.to_le_bytes());
        rgce.extend(4u32.to_le_bytes());
        rgce.extend(0u16.to_le_bytes());
        rgce.extend(2u16.to_le_bytes());
        let mut d = 1u32.to_le_bytes().to_vec();
        d.push(0);
        d.extend(0u32.to_le_bytes());
        d.extend(wide("Sales_1"));
        d.extend((rgce.len() as u32).to_le_bytes());
        d.extend(rgce);
        d.extend(0u32.to_le_bytes());
        d.extend(u32::MAX.to_le_bytes());
        out.extend(record(39, &d));
    }
    out.extend(record(132, &[]));
    out
}

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// An .xlsb package with optional connections and query table parts.
fn package(connections: Option<&[u8]>, query_table: Option<&[u8]>) -> Vec<u8> {
    package_with(connections, query_table, None)
}

const CUSTOM_PROPS: &str = "docProps/custom.xml";

/// An .xlsb package with optional connections, query table and custom document properties parts.
fn package_with(connections: Option<&[u8]>, query_table: Option<&[u8]>, custom: Option<&[u8]>) -> Vec<u8> {
    package_sheet(connections, query_table, custom, "Data")
}

/// [`package_with`] with the one sheet named `sheet_name`.
fn package_sheet(connections: Option<&[u8]>, query_table: Option<&[u8]>, custom: Option<&[u8]>, sheet_name: &str) -> Vec<u8> {
    let custom_rel =
        if custom.is_some() { format!(r#"<Relationship Id="cust" Type="{REL}/custom-properties" Target="{CUSTOM_PROPS}"/>"#) } else { String::new() };
    let root = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="{REL}/officeDocument" Target="xl/workbook.bin"/>{custom_rel}</Relationships>"#
    );
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet0.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/connections.bin" ContentType="application/vnd.ms-excel.connections"/><Override PartName="/xl/queryTables/queryTable1.bin" ContentType="application/vnd.ms-excel.queryTable"/>"#,
    );
    if custom.is_some() {
        types.push_str(&format!(
            r#"<Override PartName="/{CUSTOM_PROPS}" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>"#
        ));
    }
    types.push_str("</Types>");
    let mut rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s0" Type="{REL}/worksheet" Target="worksheets/sheet0.bin"/>"#
    );
    if connections.is_some() {
        rels.push_str(&format!(r#"<Relationship Id="rId9" Type="{REL}/connections" Target="connections.bin"/>"#));
    }
    rels.push_str("</Relationships>");
    let sheet_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/queryTable" Target="../queryTables/queryTable1.bin"/></Relationships>"#
    );
    let (book, sheet) = (book_sheet(query_table.is_some(), sheet_name), worksheet());
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/worksheets/sheet0.bin", &sheet),
    ];
    if let Some(c) = connections {
        parts.push(("xl/connections.bin", c));
    }
    if let Some(q) = query_table {
        parts.push(("xl/worksheets/_rels/sheet0.bin.rels", sheet_rels.as_bytes()));
        parts.push(("xl/queryTables/queryTable1.bin", q));
    }
    if let Some(c) = custom {
        parts.push((CUSTOM_PROPS, c));
    }
    make_zip(&parts)
}

fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut data = vec![];
        f.read_to_end(&mut data).unwrap();
        out.insert(f.name().to_string(), data);
    }
    out
}

fn connection_warnings(report: &ReadReport) -> Vec<&String> {
    report.warnings.iter().filter(|w| w.contains("data connections") || w.contains("query tables")).collect()
}

/// Reads the .xlsb, saves it as .xlsx and reads that back: the saved package's parts.
fn round_trip(xlsb: &[u8]) -> (Workbook, ReadReport, HashMap<String, Vec<u8>>) {
    let (wb, report) = read_xlsb(xlsb).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let (again, _) = read_xlsx(&saved).unwrap();
    // The re-read workbook keeps them too: a second save writes the same parts.
    let resaved = unzip(&write_xlsx(&again).unwrap());
    let parts = unzip(&saved);
    for name in parts.keys().filter(|n| n.contains("connections") || n.contains("queryTable")) {
        assert_eq!(resaved.get(name), parts.get(name), "{name}");
    }
    (wb, report, parts)
}

fn connections_xml(parts: &HashMap<String, Vec<u8>>) -> El {
    parse(parts.get("xl/connections.xml").expect("xl/connections.xml written")).unwrap()
}
fn by_id(root: &El, id: &str) -> El {
    root.kids("connection").find(|c| c.attr("id") == Some(id)).cloned().unwrap_or_else(|| panic!("connection {id}"))
}

#[test]
fn odbc_connection_is_kept_with_its_attributes() {
    let (_, report, parts) = round_trip(&package(Some(&connections_part(&[odbc()])), None));
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let root = connections_xml(&parts);
    assert_eq!(root.name, "connections");
    assert_eq!(root.kids("connection").count(), 1);
    let c = by_id(&root, "1");
    assert_eq!(c.attr("name"), Some("Query from Sales"));
    assert_eq!(c.attr("type"), Some("1"));
    assert_eq!(c.attr("refreshedVersion"), Some("6"));
    assert_eq!(c.attr("new"), Some("1"));
    assert_eq!(c.attr("saveData"), Some("1"));
    let db = c.child("dbPr").unwrap();
    assert_eq!(db.attr("connection"), Some("DSN=Sales;"));
    assert_eq!(db.attr("command"), Some("SELECT * FROM t"));
    // commandType 2 (SQL) is the schema default.
    assert_eq!(db.attr_u32("commandType").unwrap_or(2), 2);
    // The workbook relationship and content type Excel expects.
    let rels = String::from_utf8(parts["xl/_rels/workbook.xml.rels"].clone()).unwrap();
    assert!(rels.contains(&format!(r#"Type="{REL}/connections""#)), "{rels}");
    let types = String::from_utf8(parts["[Content_Types].xml"].clone()).unwrap();
    assert!(types.contains("application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml"), "{types}");
}

#[test]
fn text_import_connection_keeps_text_properties_and_fields() {
    let (_, report, parts) = round_trip(&package(Some(&connections_part(&[text_import()])), None));
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let c = by_id(&connections_xml(&parts), "2");
    assert_eq!(c.attr("type"), Some("6"));
    let t = c.child("textPr").unwrap();
    assert_eq!(t.attr("sourceFile"), Some("C:\\data\\a.csv"));
    assert!(t.flag("delimited", true));
    assert!(t.flag("comma", false));
    assert!(!t.flag("tab", true));
    // fPromptForFile is off.
    assert!(!t.flag("prompt", true));
    let fields = t.child("textFields").unwrap();
    assert_eq!(fields.attr("count"), Some("2"));
    let kinds: Vec<_> = fields.kids("textField").map(|f| f.attr("type").unwrap_or("general")).collect();
    assert_eq!(kinds, ["text", "general"]);
}

#[test]
fn web_query_keeps_its_url_and_escapes_names() {
    let (_, report, parts) = round_trip(&package(Some(&connections_part(&[web_query()])), None));
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let c = by_id(&connections_xml(&parts), "3");
    assert_eq!(c.attr("type"), Some("4"));
    assert_eq!(c.attr("name"), Some("Web <q> & \"t\""));
    let w = c.child("webPr").unwrap();
    assert_eq!(w.attr("url"), Some("https://example.com/t"));
    assert!(w.flag("htmlTables", false));
}

#[test]
fn query_table_is_kept_on_its_sheet_with_its_connection_and_name() {
    let fields = ["Region", "Product", "Sales"];
    let xlsb = package(Some(&connections_part(&[odbc()])), Some(&query_table_part(1, "Sales_1", &fields)));
    let (wb, report, parts) = round_trip(&xlsb);
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    // The query table's range: the hidden defined name.
    let name = wb.names.iter().find(|n| n.name == "Sales_1").expect("query table name kept");
    assert!(name.hidden);
    assert!(name.formula.contains("$A$1:$C$5"), "{}", name.formula);
    let book = String::from_utf8(parts["xl/workbook.xml"].clone()).unwrap();
    assert!(book.contains(r#"name="Sales_1""#), "{book}");

    let rels = parse(&parts["xl/worksheets/_rels/sheet1.xml.rels"]).unwrap();
    let rel = rels.kids("Relationship").find(|r| r.attr("Type") == Some(&format!("{REL}/queryTable"))).expect("queryTable relationship");
    let target = rel.attr("Target").unwrap().trim_start_matches('/').replace("../", "xl/");
    let q = parse(parts.get(&target).unwrap_or_else(|| panic!("{target} written"))).unwrap();
    assert_eq!(q.name, "queryTable");
    assert_eq!(q.attr("connectionId"), Some("1"));
    assert_eq!(q.attr("name"), Some("Sales_1"));
    assert!(q.flag("refreshOnLoad", false));
    assert!(q.flag("headers", true));
    let refresh = q.child("queryTableRefresh").unwrap();
    assert_eq!(refresh.attr("nextId"), Some("4"));
    let f = refresh.child("queryTableFields").unwrap();
    assert_eq!(f.attr("count"), Some("3"));
    let names: Vec<_> = f.kids("queryTableField").map(|f| f.attr("name").unwrap()).collect();
    assert_eq!(names, fields);
    let deleted = refresh.child("queryTableDeletedFields").unwrap();
    assert_eq!(deleted.kids("deletedField").map(|d| d.attr("name").unwrap()).collect::<Vec<_>>(), ["Old"]);
    let types = String::from_utf8(parts["[Content_Types].xml"].clone()).unwrap();
    assert!(types.contains("application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml"), "{types}");
}

#[test]
fn query_table_of_a_dropped_connection_is_dropped() {
    let xlsb = package(Some(&connections_part(&[odbc()])), Some(&query_table_part(7, "Sales_1", &["A"])));
    let (wb, report) = read_xlsb(&xlsb).unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("query tables could not be kept")), "{:?}", report.warnings);
    assert!(wb.sheets[0].preserved.is_none());
}

#[test]
fn unsupported_connection_is_skipped_and_the_others_kept() {
    // An OLAP connection (BrtBeginECOlapProps follows its database properties): not mapped.
    let olap = [ext_connection(4, 5, "Cube", 0, None), db_props(1, "Provider=MSOLAP;", "Sales"), record(205, &[0; 8]), record(206, &[])].concat();
    // An unknown record inside a connection.
    let unknown = [ext_connection(5, 1, "Odd", 0, None), db_props(2, "DSN=x;", "SELECT 1"), record(999, &[1, 2, 3])].concat();
    let xlsb = package(Some(&connections_part(&[odbc(), olap, unknown, web_query()])), None);
    let (_, report, parts) = round_trip(&xlsb);
    assert!(report.warnings.iter().any(|w| w.contains("Some XLSB data connections could not be kept")), "{:?}", report.warnings);
    let root = connections_xml(&parts);
    let ids: Vec<_> = root.kids("connection").map(|c| c.attr("id").unwrap()).collect();
    assert_eq!(ids, ["1", "3"]);
}

#[test]
fn truncated_parts_never_panic() {
    let conns = connections_part(&[odbc(), text_import(), web_query()]);
    let qt = query_table_part(1, "Sales_1", &["A", "B"]);
    for cut in (0..conns.len()).step_by(3) {
        let xlsb = package(Some(&conns[..cut]), Some(&qt));
        let (wb, _) = read_xlsb(&xlsb).unwrap();
        write_xlsx(&wb).unwrap();
    }
    for cut in 0..qt.len() {
        let xlsb = package(Some(&conns), Some(&qt[..cut]));
        let (wb, report) = read_xlsb(&xlsb).unwrap();
        assert!(wb.sheets[0].preserved.is_none(), "cut {cut}");
        assert!(!connection_warnings(&report).is_empty(), "cut {cut}");
        write_xlsx(&wb).unwrap();
    }
    // Damaged payloads inside complete records: each record cut short.
    let full = odbc();
    for cut in 1..full.len() {
        let mut bytes = full.clone();
        bytes.truncate(cut);
        let _ = read_xlsb(&package(Some(&connections_part(&[bytes, web_query()])), None)).unwrap();
    }
}

#[test]
fn workbook_without_connections_writes_none() {
    let (wb, report) = read_xlsb(&package(None, None)).unwrap();
    assert!(connection_warnings(&report).is_empty());
    assert!(wb.preserved.is_none());
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(!parts.keys().any(|n| n.contains("connections") || n.contains("queryTable")), "{:?}", parts.keys());
}

#[test]
fn custom_properties_and_connections_are_both_kept() {
    let custom = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Client"><vt:lpwstr>Acme</vt:lpwstr></property></Properties>"#;
    let fields = ["Region", "Product", "Sales"];
    let xlsb = package_with(Some(&connections_part(&[odbc()])), Some(&query_table_part(1, "Sales_1", &fields)), Some(custom));
    let (_, report, parts) = round_trip(&xlsb);
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    assert_eq!(parts.get(CUSTOM_PROPS).map(Vec::as_slice), Some(&custom[..]));
    let root_rels = String::from_utf8(parts["_rels/.rels"].clone()).unwrap();
    assert!(root_rels.contains(&format!(r#"Type="{REL}/custom-properties""#)), "{root_rels}");
    assert_eq!(by_id(&connections_xml(&parts), "1").attr("name"), Some("Query from Sales"));
    assert!(parts.keys().any(|n| n.starts_with("xl/queryTables/") && n.ends_with(".xml")), "{:?}", parts.keys());
    // Both survive a second read and save.
    let (again, _) = read_xlsx(&write_xlsx(&read_xlsb(&xlsb).unwrap().0).unwrap()).unwrap();
    let resaved = unzip(&write_xlsx(&again).unwrap());
    assert_eq!(resaved.get(CUSTOM_PROPS), parts.get(CUSTOM_PROPS));
    assert!(resaved.contains_key("xl/connections.xml"));
}

// ---------------------------------------------------------------- cell parameters

/// BrtBeginECParams + BrtEndECParams around the given BrtBeginECParam payloads.
fn parameters_block(params: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(265, &(params.len() as u32).to_le_bytes());
    for p in params {
        out.extend(record(267, p));
        out.extend(record(268, &[]));
    }
    out.extend(record(266, &[]));
    out
}
/// A BrtBeginECParam payload: `pbt` 2 (a cell), its flags, SQL type, name and formula.
fn cell_param(flags: u16, sql_type: u16, name: &str, rgce: &[u8]) -> Vec<u8> {
    let mut d = (flags | 2).to_le_bytes().to_vec();
    d.extend(sql_type.to_le_bytes());
    d.extend(0u32.to_le_bytes()); // wTypeSql data type: present, not used for a cell
    d.extend(wide(name));
    d.extend((rgce.len() as u32).to_le_bytes());
    d.extend(rgce);
    d.extend(0u32.to_le_bytes()); // cb
    d
}
/// A BrtBeginECParam payload: `pbt` 0 (a prompt).
fn prompt_param(name: &str, prompt: &str) -> Vec<u8> {
    let mut d = 0u16.to_le_bytes().to_vec();
    d.extend(0u16.to_le_bytes());
    d.extend(1u32.to_le_bytes()); // fLoadPrompt
    d.extend(wide(name));
    d.extend(wide(prompt));
    d
}
/// PtgRef3d: ixti, then RgceLoc (row, column).
fn ref3d(ixti: u16, row: u32, col: u16) -> Vec<u8> {
    let mut d = vec![0x3A];
    d.extend(ixti.to_le_bytes());
    d.extend(row.to_le_bytes());
    d.extend(col.to_le_bytes());
    d
}
/// PtgArea3d: ixti, then RgceArea (first and last row, first and last column).
fn area3d(ixti: u16, r0: u32, r1: u32, c0: u16, c1: u16) -> Vec<u8> {
    let mut d = vec![0x3B];
    d.extend(ixti.to_le_bytes());
    d.extend(r0.to_le_bytes());
    d.extend(r1.to_le_bytes());
    d.extend(c0.to_le_bytes());
    d.extend(c1.to_le_bytes());
    d
}
fn cell_connection(id: u32, ty: u32, params: Vec<u8>) -> Vec<u8> {
    let props = if ty == 4 { web_props("https://example.com/t") } else { db_props(2, "DSN=Sales;", "SELECT * FROM t WHERE id=?") };
    [ext_connection(id, ty, "Query", 0x40, None), props, params].concat()
}

#[test]
fn cell_parameter_keeps_its_connection_and_siblings() {
    let params = parameters_block(&[prompt_param("Region", "Which region?"), cell_param(0x0A, 4, "Customer", &ref3d(0, 1, 0))]);
    let xlsb = package(Some(&connections_part(&[cell_connection(1, 1, params)])), None);
    let (_, report, parts) = round_trip(&xlsb);
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let c = by_id(&connections_xml(&parts), "1");
    let params = c.child("parameters").expect("parameters");
    assert_eq!(params.attr("count"), Some("2"));
    let p = params.kids("parameter").next().expect("prompt parameter");
    assert_eq!(p.attr("name"), Some("Region"));
    assert_eq!(p.attr("prompt"), Some("Which region?"));
    let p = params.kids("parameter").nth(1).expect("cell parameter");
    assert_eq!(p.attr("parameterType"), Some("cell"));
    assert_eq!(p.attr("cell"), Some("Data!$A$2"));
    assert_eq!(p.attr("sqlType"), Some("4"));
    assert!(p.flag("refreshOnChange", false));
}

#[test]
fn web_query_cell_parameter_takes_a_single_row_or_column() {
    let params = parameters_block(&[cell_param(0, 0, "Row", &area3d(0, 2, 2, 1, 3)), cell_param(0, 0, "Col", &area3d(0, 0, 4, 2, 2))]);
    let xlsb = package(Some(&connections_part(&[cell_connection(3, 4, params)])), None);
    let (_, report, parts) = round_trip(&xlsb);
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let c = by_id(&connections_xml(&parts), "3");
    let cells: Vec<_> = c.path(&["parameters"]).map(|p| p.kids("parameter").filter_map(|p| p.attr("cell")).collect()).unwrap_or_default();
    assert_eq!(cells, ["Data!$B$3:$D$3", "Data!$C$1:$C$5"]);
}

#[test]
fn cell_parameter_quotes_a_sheet_name_only_when_required() {
    let params = parameters_block(&[cell_param(0, 0, "P", &ref3d(0, 1, 1))]);
    let xlsb = package_sheet(Some(&connections_part(&[cell_connection(1, 1, params)])), None, None, "My Data");
    let (_, report, parts) = round_trip(&xlsb);
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let p = by_id(&connections_xml(&parts), "1").path(&["parameters", "parameter"]).cloned().expect("parameter");
    assert_eq!(p.attr("cell"), Some("'My Data'!$B$2"));
}

#[test]
fn one_cell_area_is_a_cell_for_odbc() {
    let params = parameters_block(&[cell_param(0, 0, "P", &area3d(0, 3, 3, 2, 2))]);
    let xlsb = package(Some(&connections_part(&[cell_connection(1, 1, params)])), None);
    let (_, report, parts) = round_trip(&xlsb);
    assert!(connection_warnings(&report).is_empty(), "{:?}", report.warnings);
    let p = by_id(&connections_xml(&parts), "1").path(&["parameters", "parameter"]).cloned().expect("parameter");
    assert_eq!(p.attr("cell"), Some("Data!$C$4"));
}

#[test]
fn unsupported_cell_parameters_drop_only_their_connection() {
    // ODBC takes one cell: a range is refused.
    let odbc_range = cell_connection(1, 1, parameters_block(&[cell_param(0, 0, "P", &area3d(0, 0, 1, 0, 1))]));
    // A reference to another workbook (a second supporting link that is not this one).
    let external = cell_connection(2, 1, parameters_block(&[cell_param(0, 0, "P", &ref3d(1, 0, 0))]));
    // A formula rather than a reference.
    let formula = cell_connection(3, 1, parameters_block(&[cell_param(0, 0, "P", &[0x1E, 1, 0, 0x1E, 2, 0, 0x03])]));
    // A truncated formula: cce says 9 bytes, 3 follow.
    let truncated = {
        let mut d = 2u16.to_le_bytes().to_vec();
        d.extend(0u16.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend(wide("P"));
        d.extend(9u32.to_le_bytes());
        d.extend([0x3A, 0, 0, 0]);
        cell_connection(4, 1, [record(265, &1u32.to_le_bytes()), record(267, &d), record(268, &[]), record(266, &[])].concat())
    };
    // A malformed cce: zero, and past the 16,384-byte limit.
    let zero = cell_connection(5, 1, parameters_block(&[cell_param(0, 0, "P", &[])]));
    let huge = {
        let mut d = 2u16.to_le_bytes().to_vec();
        d.extend(0u16.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend(wide("P"));
        d.extend(16_385u32.to_le_bytes());
        cell_connection(6, 1, [record(265, &1u32.to_le_bytes()), record(267, &d), record(268, &[]), record(266, &[])].concat())
    };
    let good = cell_connection(7, 1, parameters_block(&[cell_param(0, 0, "P", &ref3d(0, 0, 0))]));
    let xlsb = package(Some(&connections_part(&[odbc_range, external, formula, truncated, zero, huge, good])), None);
    let (_, report, parts) = round_trip(&xlsb);
    assert_eq!(connection_warnings(&report).len(), 1, "{:?}", report.warnings);
    let root = connections_xml(&parts);
    let ids: Vec<_> = root.kids("connection").filter_map(|c| c.attr("id")).collect();
    assert_eq!(ids, ["7"]);
    assert_eq!(by_id(&root, "7").path(&["parameters", "parameter"]).and_then(|p| p.attr("cell")), Some("Data!$A$1"));
}
