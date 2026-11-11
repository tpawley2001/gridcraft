//! ODS array (matrix) formulas: a `table:formula` cell with `table:number-matrix-columns-spanned` /
//! `table:number-matrix-rows-spanned` (ODF 1.2 Part 1, 19.685/19.686) opens as a legacy array formula
//! over that range and calculates like Excel instead of spilling into the cached neighbours
//! (`#SPILL!`). Expected values were measured in Excel opening such a file and recalculating.
//! The ODS packages are built in code; every cached value is 0 so the checks see recalculated values.

use std::io::Write;

use gridcraft_core::{CellError, CellRef, RangeRef, Value};
use gridcraft_engine::{DocState, Session, io};
use serde_json::json;

const MIME: &str = "application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &str = r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;

/// An ODS package with one sheet "Data" whose rows are the given `<table:table-row>` bodies.
fn ods(rows: &[&str]) -> Vec<u8> {
    let rows: String = rows.iter().map(|r| format!("<table:table-row>{r}</table:table-row>")).collect();
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2"><office:body><office:spreadsheet><table:table table:name="Data">{rows}</table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in [("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_str())] {
        z.start_file(name, stored).unwrap();
        z.write_all(data.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn empty(n: u32) -> String {
    if n == 0 { String::new() } else { format!(r#"<table:table-cell table:number-columns-repeated="{n}"/>"#) }
}

fn num(v: f64) -> String {
    format!(r#"<table:table-cell office:value-type="float" office:value="{v}"/>"#)
}

/// A formula cell cached as 0; `matrix` = (columns, rows) spanned, if any.
fn formula(f: &str, matrix: Option<(u32, u32)>) -> String {
    let span = matrix.map_or(String::new(), |(c, r)| format!(r#" table:number-matrix-columns-spanned="{c}" table:number-matrix-rows-spanned="{r}""#));
    format!(r#"<table:table-cell table:formula="{f}"{span} office:value-type="float" office:value="0"/>"#)
}

/// The test sheet (A1-based):
/// row 1: E1 =SIN({-1;0;1;2}) 4x1, F1:H1 cached 0
/// row 2: B2 ={2;3}*{4|5} 2x2, C2 cached 0
/// row 3: B3:C3 cached 0, E3 =MAX(SIN({-1;0;1;2})) 1x1
/// row 5: A5=2 B5=3, D5 =[.A5:.B5]*[.A5:.A6] 2x2, E5 cached 0
/// row 6: A6=4 B6=5, D6:E6 cached 0
/// row 9: A9 =SIN({1;2}) 3x1, B9:C9 cached 0
fn sheet() -> Vec<u8> {
    let zero = num(0.0);
    let r1 = format!("{}{}{zero}{zero}{zero}", empty(4), formula("of:=SIN({-1;0;1;2})", Some((4, 1))));
    let r2 = format!("{}{}{zero}", empty(1), formula("of:={2;3}*{4|5}", Some((2, 2))));
    let r3 = format!("{}{zero}{zero}{}{}", empty(1), empty(1), formula("of:=MAX(SIN({-1;0;1;2}))", Some((1, 1))));
    let r5 = format!("{}{}{}{}{zero}", num(2.0), num(3.0), empty(1), formula("of:=[.A5:.B5]*[.A5:.A6]", Some((2, 2))));
    let r6 = format!("{}{}{}{zero}{zero}", num(4.0), num(5.0), empty(1));
    let r9 = format!("{}{zero}{zero}", formula("of:=SIN({1;2})", Some((3, 1))));
    let blank = empty(1);
    ods(&[&r1, &r2, &r3, &blank, &r5, &r6, &blank, &blank, &r9])
}

fn calculated(bytes: &[u8]) -> Session {
    let (wb, _) = io::open_bytes("matrix.ods", bytes).expect("open ods");
    let mut s = Session::new();
    s.add_document(DocState::new(wb, None, "matrix".into(), gridcraft_engine::Trust::FromFile));
    s.execute("formulas.calculateNow", json!({})).expect("recalc");
    s
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

fn value(s: &Session, a1: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(a1))
}

fn assert_num(s: &Session, a1: &str, want: f64) {
    match value(s, a1) {
        Value::Number(n) => assert!((n - want).abs() < 1e-12, "{a1}: got {n}, Excel {want}"),
        other => panic!("{a1}: got {other:?}, Excel {want}"),
    }
}

#[test]
fn matrix_formulas_calculate_over_their_range_like_excel() {
    let s = calculated(&sheet());
    // E1:H1 = SIN({-1,0,1,2})
    assert_num(&s, "E1", -0.8414709848078965);
    assert_num(&s, "F1", 0.0);
    assert_num(&s, "G1", 0.8414709848078965);
    assert_num(&s, "H1", 0.9092974268256817);
    // B2:C3 = {2,3}*{4;5}
    assert_num(&s, "B2", 8.0);
    assert_num(&s, "C2", 12.0);
    assert_num(&s, "B3", 10.0);
    assert_num(&s, "C3", 15.0);
    // 1x1 matrix aggregates the whole array
    assert_num(&s, "E3", 0.9092974268256817);
    // D5:E6 = A5:B5*A5:A6
    assert_num(&s, "D5", 4.0);
    assert_num(&s, "E5", 6.0);
    assert_num(&s, "D6", 8.0);
    assert_num(&s, "E6", 12.0);
}

#[test]
fn a_matrix_range_wider_than_the_result_shows_na_in_the_extra_cells() {
    let s = calculated(&sheet());
    assert_num(&s, "A9", 0.8414709848078965);
    assert_num(&s, "B9", 0.9092974268256817);
    assert_eq!(value(&s, "C9"), Value::Error(CellError::NA));
}

#[test]
fn the_anchor_holds_an_array_formula_over_the_spanned_range() {
    let (wb, _) = io::open_bytes("matrix.ods", &sheet()).expect("open ods");
    let sh = wb.sheet(0).unwrap();
    for (anchor, range) in [("E1", "E1:H1"), ("B2", "B2:C3"), ("E3", "E3:E3"), ("D5", "D5:E6"), ("A9", "A9:C9")] {
        let f = sh.cell(at(anchor)).and_then(|c| c.formula.clone()).unwrap_or_else(|| panic!("{anchor}: no formula"));
        assert_eq!(f.array, Some(RangeRef::parse(range).unwrap()), "{anchor}");
    }
    // Covered cells carry no formula of their own.
    for covered in ["F1", "H1", "C2", "B3", "C3", "E6", "C9"] {
        assert!(sh.cell(at(covered)).and_then(|c| c.formula.as_ref()).is_none(), "{covered} has a formula");
    }
}

#[test]
fn saved_as_xlsx_the_formula_is_an_array_formula() {
    let (wb, _) = io::open_bytes("matrix.ods", &sheet()).expect("open ods");
    let bytes = io::save_bytes(&wb, "matrix.xlsx").expect("save xlsx");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut z.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut xml).unwrap();
    assert!(xml.contains(r#"<f t="array" ref="E1:H1">SIN({-1,0,1,2})</f>"#), "{xml}");
    assert!(xml.contains(r#"<f t="array" ref="B2:C3">"#), "{xml}");
    assert!(xml.contains(r#"<f t="array" ref="D5:E6">"#), "{xml}");
}

#[test]
fn guards_plain_formulas_and_matrix_attributes_without_a_formula_are_unchanged() {
    // A plain (non-matrix) formula stays a plain formula; matrix attributes on a value cell are ignored.
    let r1 = format!(
        "{}{}{}",
        num(3.0),
        formula("of:=[.A1]*2", None),
        r#"<table:table-cell table:number-matrix-columns-spanned="2" table:number-matrix-rows-spanned="1" office:value-type="float" office:value="7"/>"#
    );
    let (wb, _) = io::open_bytes("plain.ods", &ods(&[&r1])).expect("open ods");
    let sh = wb.sheet(0).unwrap();
    let f = sh.cell(at("B1")).and_then(|c| c.formula.clone()).expect("B1 formula");
    assert_eq!(f.array, None);
    assert!(sh.cell(at("C1")).and_then(|c| c.formula.as_ref()).is_none());
    assert_eq!(sh.value(at("C1")), Value::Number(7.0));
}
