//! Without a cube (OLAP) connection, Excel keeps the value saved in the file for every cell whose
//! formula calls a cube function, through any recalculation, and calculates the cells that read them
//! from those values. Measured in Microsoft 365 Excel: three cube workbooks, 1,759 cube formula
//! cells, all unchanged after a full recalculation without a connection.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellError, CellRef, Value};
use gridcraft_engine::io;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// Sheet1 as Excel saved it with a live cube connection: A1 `CUBEMEMBER` ("Profit"), B1 `CUBEVALUE`
/// (339610.8964), C1 `CUBEMEMBERPROPERTY` (#N/A), D1 `=B1*2` (stale cached 0), A2:A3 a shared
/// `CUBEVALUE` formula (12.5, 99).
fn package() -> Vec<u8> {
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1" t="str"><f>CUBEMEMBER("Sales","[Measures].[Profit]")</f><v>Profit</v></c><c r="B1"><f>CUBEVALUE("Sales",A1)</f><v>339610.8964</v></c><c r="C1" t="e"><f>CUBEMEMBERPROPERTY("Sales","[Product].&amp;[26]","Category")</f><v>#N/A</v></c><c r="D1"><f>B1*2</f><v>0</v></c></row><row r="2"><c r="A2"><f t="shared" ref="A2:A3" si="0">CUBEVALUE("Sales",$A$1,"[Store].&amp;["&amp;ROW()&amp;"]")</f><v>12.5</v></c></row><row r="3"><c r="A3"><f t="shared" si="0"/><v>99</v></c></row></sheetData></worksheet>"#
    );
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/></Types>"#
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            ),
        ),
        (
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn open() -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "cube.xlsx", "base64": io::base64_encode(&package())})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    s
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(cell))
}

fn set(s: &mut Session, cell: &str, input: &str) {
    s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
}

fn expected() -> Vec<(&'static str, Value)> {
    vec![
        ("A1", Value::from("Profit")),
        ("B1", Value::Number(339610.8964)),
        ("C1", Value::Error(CellError::NA)),
        ("D1", Value::Number(339610.8964 * 2.0)),
        ("A2", Value::Number(12.5)),
        ("A3", Value::Number(99.0)),
    ]
}

fn assert_kept(s: &Session, when: &str) {
    for (cell, v) in expected() {
        assert_eq!(value(s, cell), v, "{cell} {when}");
    }
}

#[test]
fn cube_formulas_keep_their_saved_values_through_recalculation() {
    let mut s = open();
    assert_kept(&s, "after open and Calculate Now");
    s.execute("formulas.calculateNow", json!({})).unwrap();
    assert_kept(&s, "after a second Calculate Now");
    // An unrelated edit recalculates; the cube cells still show what was saved.
    set(&mut s, "Z9", "1");
    assert_kept(&s, "after an unrelated edit");
}

#[test]
fn saving_writes_the_kept_cube_values() {
    let s = open();
    let bytes = io::save_bytes(&s.doc().unwrap().wb, "cube.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut xml).unwrap();
    let cell = |r: &str| {
        let start = xml.find(&format!(r#"<c r="{r}""#)).unwrap_or_else(|| panic!("{r} saved: {xml}"));
        let end = start + xml[start..].find("</c>").expect("cell closed");
        xml[start..end].to_string()
    };
    let a1 = cell("A1");
    assert!(a1.contains(r#"t="str""#) && a1.contains("<v>Profit</v>"), "{a1}");
    let b1 = cell("B1");
    assert!(b1.contains("<v>339610.8964</v>"), "{b1}");
}

#[test]
fn a_cube_formula_typed_without_a_connection_is_na() {
    let mut s = Session::new();
    s.new_workbook();
    set(&mut s, "B2", "=CUBEVALUE(\"Sales\",\"[Measures].[Profit]\")");
    assert_eq!(value(&s, "B2"), Value::Error(CellError::NA));
    set(&mut s, "C2", "=CUBEMEMBER(\"Sales\",\"[Measures].[Profit]\")");
    assert_eq!(value(&s, "C2"), Value::Error(CellError::NA));
}

#[test]
fn a_cube_formula_typed_over_a_kept_one_does_not_reuse_its_value() {
    let mut s = open();
    set(&mut s, "B1", "=CUBEVALUE(\"Sales\",\"[Measures].[Store Cost]\")");
    assert_eq!(value(&s, "B1"), Value::Error(CellError::NA));
    // A cell that held a number before gets #N/A for a new cube formula too.
    set(&mut s, "F5", "5");
    set(&mut s, "F5", "=CUBEVALUE(\"Sales\",A1)");
    assert_eq!(value(&s, "F5"), Value::Error(CellError::NA));
}
