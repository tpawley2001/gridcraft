//! Query tables and their data connections in legacy `.xls` (BIFF8) worksheets ([MS-XLS] Qsi,
//! DbQuery, QsiSXTag, DBQueryExt, TxtQry, Qsir, Qsif), kept as `xl/connections.xml` and
//! `xl/queryTables/*.xml` when saved as .xlsx. Every workbook is assembled here in code.
use std::collections::HashMap;
use std::io::Read;

use gridcraft_core::Value;
use gridcraft_model::Workbook;

use super::binary_custom_props_tests::{all_types, custom_part, stream};
use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s, v, xlstr};
use crate::xml::{El, parse};
use crate::{ReadReport, read_xls, read_xlsx, write_xlsx};

const NAME: &str = "ExternalData_1";
const REL_QUERY_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable";
const DROPPED: &str = "data connections could not be kept";

fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Qsi: fTitles, fAsync and fSaveData, AutoFormat 0, number and font formats applied.
fn qsi() -> Vec<u8> {
    rec(0x01AD, &cat(&[&u16s(&[0x0209, 0, 0x0003]), &u32s(&[0]), &xlstr(NAME), &[0, 0]]))
}
/// DbQuery: the data source type and flags, then the SXString counts.
fn db_query(grbit: u16, cst_query: u16, cst_web_post: u16, cst_odbc_conn: u16) -> Vec<u8> {
    rec(0x00DC, &u16s(&[grbit, 0, cst_query, cst_web_post, 0, cst_odbc_conn]))
}
fn sx_string(s: &str) -> Vec<u8> {
    rec(0x00CD, &cat(&[&(s.len() as u16).to_le_bytes(), &[0], s.as_bytes()]))
}
/// QsiSXTag for the query table: user formatting preserved and columns auto-fitted.
fn qsi_sx_tag() -> Vec<u8> {
    rec(0x0802, &cat(&[&u16s(&[0x0802, 0, 0, 0]), &u32s(&[0x03]), &[0, 0, 0x10, 0], &xlstr(NAME), &[0, 0]]))
}
/// DBQueryExt: refreshed with and refreshable by data functionality level 3.
fn db_query_ext(dbt: u16, grbit_dbt: u16, txt_wiz: bool, html_fmt: u16) -> Vec<u8> {
    rec(0x0803, &cat(&[&u16s(&[0x0803, 0, dbt, 0, grbit_dbt, u16::from(txt_wiz)]), &[3, 3, 3, 0], &u16s(&[0, 0, 0, 0, html_fmt, 0])]))
}
/// Qsir with the given field names, then one Qsif per field.
fn qsir(fields: &[&str]) -> Vec<u8> {
    let flags: u32 = 0x07 | (1 << 25);
    let head = cat(&[
        &u16s(&[0x0806, 0x0001, 0, 2, 0, 1]),
        &u16s(&[24, 12]),
        &u32s(&[flags, 0, fields.len() as u32, 0, fields.len() as u32 + 1]),
        &u16s(&[0, 0]),
    ]);
    let mut out = rec(0x0806, &head);
    for (i, name) in fields.iter().enumerate() {
        out.extend(rec(0x0807, &cat(&[&u16s(&[0x0807, 0]), &u32s(&[0, i as u32 + 1]), &xlstr(name)])));
    }
    out
}

/// An ODBC query table: connection string and SQL command in SXString records.
fn odbc_records(ext: Vec<u8>) -> Vec<u8> {
    cat(&[
        &qsi(),
        &db_query(0x0001 | 0x0008 | 0x0010, 1, 0, 1),
        &sx_string("SELECT Region, Sales FROM Orders"),
        &sx_string("DSN=Sales;UID=reader"),
        &qsi_sx_tag(),
        &ext,
        &qsir(&["Region", "Sales"]),
    ])
}
fn odbc() -> Vec<u8> {
    odbc_records(db_query_ext(1, 0, false, 0))
}
/// A comma-delimited text import of a DOS file from its second row, with two fields.
fn text_import() -> Vec<u8> {
    // fFile, fDelimited, iCpid = MS-DOS.
    let flags: u16 = 0x0001 | 0x0002 | (2 << 2);
    let txt = rec(
        0x0805,
        &cat(&[
            &u16s(&[0x0805, 0, flags, 0]),
            &u32s(&[2]),
            &[0x04],
            &u16s(&[0]),
            &[0],
            &u32s(&[2]),
            b".,",
            &u32s(&[0, 0, 1, 6]),
            &xlstr("C:\\data\\orders.csv"),
        ]),
    );
    cat(&[&qsi(), &db_query(6, 0, 0, 0), &qsi_sx_tag(), &db_query_ext(6, 0, true, 0), &txt, &qsir(&["Region", "Sales"])])
}
/// A web query: its URL is the command string.
fn web_query() -> Vec<u8> {
    cat(&[
        &qsi(),
        &db_query(0x0004 | 0x0040, 1, 0, 0),
        &sx_string("https://example.com/rates?a=1&b=2"),
        &qsi_sx_tag(),
        &db_query_ext(4, 0x0001, false, 1),
        &qsir(&["Rate"]),
    ])
}

/// The Workbook stream: one worksheet "Sheet1" holding A1:B3 and `query` records, and the
/// query table's hidden `ExternalData_1` name local to it (= Sheet1!$A$1:$B$3).
fn workbook_stream(query: &[u8]) -> Vec<u8> {
    let area = cat(&[&[0x3B], &0u16.to_le_bytes(), &u16s(&[0, 2, 0, 1])]);
    let lbl =
        rec(0x0018, &cat(&[&u16s(&[0x0001]), &[0, NAME.len() as u8], &u16s(&[area.len() as u16, 0, 1]), &[0; 4], &[0], NAME.as_bytes(), &area]));
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
            &lbl,
            &eof(),
        ])
    };
    let mut sheet = bof(0x0010);
    for r in 0..3u16 {
        for c in 0..2u16 {
            sheet.extend(rec(0x0203, &cat(&[&cell(r, c, 0), &f64::from(r * 10 + c).to_le_bytes()])));
        }
    }
    sheet.extend(query);
    sheet.extend(eof());
    cat(&[&globals(globals(0).len() as u32), &sheet])
}
fn book(query: &[u8]) -> Vec<u8> {
    compound("Workbook", &workbook_stream(query))
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
fn dropped(report: &ReadReport) -> usize {
    report.warnings.iter().filter(|w| w.contains(DROPPED)).count()
}
/// Reads the .xls and saves it as .xlsx: the workbook, the report and the saved parts.
fn save(xls: &[u8]) -> (Workbook, ReadReport, HashMap<String, Vec<u8>>) {
    let (wb, report) = read_xls(xls).unwrap();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    (wb, report, parts)
}
/// The only connection of the saved `xl/connections.xml`.
fn connection(parts: &HashMap<String, Vec<u8>>) -> El {
    let root = parse(parts.get("xl/connections.xml").expect("xl/connections.xml written")).unwrap();
    let all: Vec<_> = root.kids("connection").cloned().collect();
    assert_eq!(all.len(), 1);
    all.into_iter().next().unwrap()
}
/// The query table part Sheet1 links to.
fn query_table(parts: &HashMap<String, Vec<u8>>) -> El {
    let rels = parse(&parts["xl/worksheets/_rels/sheet1.xml.rels"]).unwrap();
    let rel = rels.kids("Relationship").find(|r| r.attr("Type") == Some(REL_QUERY_TABLE)).expect("queryTable relationship");
    let target = rel.attr("Target").unwrap().trim_start_matches('/').replace("../", "xl/");
    parse(parts.get(&target).unwrap_or_else(|| panic!("{target} written"))).unwrap()
}

#[test]
fn odbc_query_table_is_kept_with_its_connection() {
    let (wb, report, parts) = save(&book(&odbc()));
    assert_eq!(dropped(&report), 0, "{:?}", report.warnings);
    let c = connection(&parts);
    assert_eq!(c.attr("id"), Some("1"));
    assert_eq!(c.attr("type"), Some("1"));
    assert_eq!(c.attr("name"), Some(NAME));
    assert_eq!(c.attr("refreshedVersion"), Some("3"));
    let db = c.child("dbPr").expect("dbPr");
    assert_eq!(db.attr("connection"), Some("DSN=Sales;UID=reader"));
    assert_eq!(db.attr("command"), Some("SELECT Region, Sales FROM Orders"));

    let q = query_table(&parts);
    assert_eq!(q.attr("connectionId"), c.attr("id"));
    assert_eq!(q.attr("name"), Some(NAME));
    assert!(q.flag("headers", true));
    assert!(q.flag("backgroundRefresh", true));
    assert!(!q.flag("removeDataOnSave", false));
    let fields = q.child("queryTableRefresh").and_then(|r| r.child("queryTableFields")).expect("queryTableFields");
    assert_eq!(fields.attr("count"), Some("2"));
    let names: Vec<_> = fields.kids("queryTableField").map(|f| f.attr("name").unwrap()).collect();
    assert_eq!(names, ["Region", "Sales"]);

    // The query table's range: its hidden defined name, saved with the workbook.
    let name = wb.names.iter().find(|n| n.name == NAME).expect("query table name kept");
    assert!(name.hidden);
    assert!(name.formula.contains("$A$1:$B$3"), "{}", name.formula);
    let book = String::from_utf8(parts["xl/workbook.xml"].clone()).unwrap();
    assert!(book.contains(&format!(r#"name="{NAME}""#)), "{book}");
    assert_eq!(v(&wb, 0, "B3"), Value::number(21.0));
}

#[test]
fn text_import_keeps_its_text_properties() {
    let (_, report, parts) = save(&book(&text_import()));
    assert_eq!(dropped(&report), 0, "{:?}", report.warnings);
    let c = connection(&parts);
    assert_eq!(c.attr("type"), Some("6"));
    let t = c.child("textPr").expect("textPr");
    assert_eq!(t.attr("sourceFile"), Some("C:\\data\\orders.csv"));
    assert_eq!(t.attr("fileType"), Some("dos"));
    assert_eq!(t.attr("firstRow"), Some("2"));
    assert!(t.flag("delimited", true));
    assert!(t.flag("comma", false));
    assert!(!t.flag("tab", true));
    assert!(!t.flag("prompt", true));
    let fields = t.child("textFields").expect("textFields");
    assert_eq!(fields.attr("count"), Some("2"));
    let types: Vec<_> = fields.kids("textField").map(|f| (f.attr("type"), f.attr("position"))).collect();
    assert_eq!(types, [(None, None), (Some("text"), Some("6"))]);
    assert_eq!(query_table(&parts).attr("connectionId"), Some("1"));
}

#[test]
fn web_query_keeps_its_url() {
    let (_, report, parts) = save(&book(&web_query()));
    assert_eq!(dropped(&report), 0, "{:?}", report.warnings);
    let c = connection(&parts);
    assert_eq!(c.attr("type"), Some("4"));
    let w = c.child("webPr").expect("webPr");
    assert_eq!(w.attr("url"), Some("https://example.com/rates?a=1&b=2"));
    assert!(w.flag("parsePre", false));
    assert_eq!(w.attr("htmlFormat"), None);
}

#[test]
fn custom_properties_and_connections_are_both_kept() {
    let doc = stream(Some(&all_types()));
    let xls = crate::testing::compound_file(&[("Workbook", &workbook_stream(&odbc())), ("\u{5}DocumentSummaryInformation", &doc)]);
    let (wb, report, parts) = save(&xls);
    assert_eq!(dropped(&report), 0, "{:?}", report.warnings);
    assert!(custom_part(&wb).is_some());
    assert!(parts.contains_key("docProps/custom.xml"));
    assert_eq!(connection(&parts).attr("type"), Some("1"));
}

#[test]
fn malformed_query_drops_the_connection_and_keeps_the_cells() {
    let full = db_query_ext(1, 0, false, 0);
    // DBQueryExt cut short after its data source type.
    let truncated = cat(&[&0x0803u16.to_le_bytes(), &8u16.to_le_bytes(), &full[4..12]]);
    // A parameter query (cparams = 1): parameters are not mapped.
    let params = rec(0x00DC, &u16s(&[0x0019, 1, 1, 0, 0, 1]));
    let parameters = cat(&[&qsi(), &params, &sx_string("a"), &sx_string("b"), &qsi_sx_tag(), &full, &qsir(&["A"])]);
    for query in [odbc_records(truncated), parameters] {
        let (wb, report, parts) = save(&book(&query));
        assert!(!parts.contains_key("xl/connections.xml"));
        assert!(!parts.keys().any(|k| k.contains("queryTable")));
        assert_eq!(dropped(&report), 1, "{:?}", report.warnings);
        assert_eq!(v(&wb, 0, "B3"), Value::number(21.0));
    }
}

#[test]
fn hostile_query_records_never_panic() {
    let full = odbc();
    for cut in 0..full.len() {
        let _ = read_xls(&book(&full[..cut]));
    }
}

#[test]
fn kept_query_table_survives_an_xlsx_round_trip() {
    let (_, _, parts) = save(&book(&odbc()));
    let saved = write_xlsx(&read_xls(&book(&odbc())).unwrap().0).unwrap();
    let (again, _) = read_xlsx(&saved).unwrap();
    let resaved = unzip(&write_xlsx(&again).unwrap());
    let c = connection(&resaved);
    assert_eq!(c.child("dbPr").and_then(|d| d.attr("connection")), Some("DSN=Sales;UID=reader"));
    let q = query_table(&resaved);
    assert_eq!(q.attr("connectionId"), Some("1"));
    assert_eq!(query_table(&parts).attr("name"), q.attr("name"));
    assert!(again.names.iter().any(|n| n.name == NAME));
}
