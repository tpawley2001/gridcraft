//! With iterative calculation off, Excel keeps the value a formula on a circular reference shows
//! (its cached value from the file, or 0 for a formula just entered), lists the loop, and
//! calculates the formulas that read it from that value. Expected values follow Excel's own
//! answers for a file whose only loop is `G2 =G2+1` with cached 16.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_engine::io;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// Sheet1: C4 `=G3` (210), D1 `=G2*2` (stale cached 0), G2 `=G2+1` (16),
/// G3 `=IF(ISODD(G2),-60,210)` (210). No calcPr: iteration off.
fn package() -> Vec<u8> {
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="D1"><f>G2*2</f><v>0</v></c></row><row r="2"><c r="G2"><f>G2+1</f><v>16</v></c></row><row r="3"><c r="G3"><f>IF(ISODD(G2),-60,210)</f><v>210</v></c></row><row r="4"><c r="C4"><f>G3</f><v>210</v></c></row></sheetData></worksheet>"#
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
    s.execute("file.open", json!({"name": "loop.xlsx", "base64": io::base64_encode(&package())})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    s
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(cell))
}

fn set(s: &mut Session, cell: &str, input: &str) {
    s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
}

fn listed(s: &Session, cell: &str) -> bool {
    s.doc().unwrap().calc.circular.contains(&(0, at(cell)))
}

fn blank() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

#[test]
fn loop_cell_keeps_its_cached_value_and_dependents_use_it() {
    let mut s = open();
    let expected = [("G2", num(16.0)), ("G3", num(210.0)), ("C4", num(210.0)), ("D1", num(32.0))];
    for (cell, v) in &expected {
        assert_eq!(value(&s, cell), *v, "{cell} after the first calculation");
    }
    // Recalculating does not advance the loop.
    s.execute("formulas.calculateNow", json!({})).unwrap();
    for (cell, v) in &expected {
        assert_eq!(value(&s, cell), *v, "{cell} after a second calculation");
    }
    assert!(listed(&s, "G2"), "{:?}", s.doc().unwrap().calc.circular);
}

#[test]
fn saving_keeps_the_loop_cell_value() {
    let s = open();
    let bytes = io::save_bytes(&s.doc().unwrap().wb, "loop.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut xml).unwrap();
    let start = xml.find(r#"<c r="G2""#).expect("G2 saved");
    let end = start + xml[start..].find("</c>").expect("G2 closed");
    assert!(xml[start..end].contains("<v>16</v>"), "{}", &xml[start..end]);
}

#[test]
fn a_new_loop_shows_zero_and_stays_listed_through_undo() {
    let mut s = blank();
    set(&mut s, "H1", "=H1+1");
    set(&mut s, "H2", "=H1+5");
    assert_eq!(value(&s, "H1"), num(0.0));
    assert_eq!(value(&s, "H2"), num(5.0));
    assert!(listed(&s, "H1"), "{:?}", s.doc().unwrap().calc.circular);
    // Undo rebuilds the calculation state from the workbook: the loop is still reported.
    set(&mut s, "B9", "1");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(value(&s, "H1"), num(0.0));
    assert!(listed(&s, "H1"), "after undo: {:?}", s.doc().unwrap().calc.circular);
}

#[test]
fn a_two_cell_loop_shows_numbers_that_do_not_change() {
    let mut s = blank();
    set(&mut s, "J1", "=J2+1");
    set(&mut s, "J2", "=J1+1");
    let (j1, j2) = (value(&s, "J1"), value(&s, "J2"));
    assert!(matches!(j1, Value::Number(_)), "J1 = {j1:?}");
    assert!(matches!(j2, Value::Number(_)), "J2 = {j2:?}");
    assert!(listed(&s, "J1") || listed(&s, "J2"), "{:?}", s.doc().unwrap().calc.circular);
    s.execute("formulas.calculateNow", json!({})).unwrap();
    assert_eq!((value(&s, "J1"), value(&s, "J2")), (j1, j2));
}

#[test]
fn switching_iteration_off_keeps_the_iterated_values() {
    let mut s = blank();
    set(&mut s, "A1", "=(A2+10)/2");
    set(&mut s, "A2", "=A1");
    s.execute("formulas.calculationOptions", json!({"iterative": true})).unwrap();
    s.execute("formulas.calculationOptions", json!({"iterative": false})).unwrap();
    for cell in ["A1", "A2"] {
        let got = value(&s, cell).as_f64().unwrap_or(f64::NAN);
        assert!((got - 10.0).abs() < 0.002, "{cell} = {:?}", value(&s, cell));
    }
    assert!(listed(&s, "A1") || listed(&s, "A2"), "{:?}", s.doc().unwrap().calc.circular);
}
