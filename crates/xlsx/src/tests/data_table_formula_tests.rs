//! What-If data tables (`<f t="dataTable">`, ECMA-376 Part 1, 18.3.1.40) are kept through open +
//! save. The cell shapes below are the ones Excel writes (one-variable column and row tables, a
//! two-variable table, and tables whose input cells were deleted).

use gridcraft_core::{CellError, CellRef, RangeRef, Value};
use gridcraft_model::{DataTable, Workbook};

use super::minimal;
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rr(a: &str) -> RangeRef {
    RangeRef::parse(a).unwrap()
}

const BODY: &str = r#"<sheetData>
    <row r="1"><c r="A1"><v>3</v></c><c r="B1"><v>4</v></c></row>
    <row r="2"><c r="A2"><v>5</v></c><c r="C2" t="e"><f t="dataTable" ref="C2:D3" dt2D="1" dtr="1" del1="1" del2="1" r1="A2" r2="A1"/><v>#REF!</v></c><c r="D2" t="e"><v>#REF!</v></c></row>
    <row r="3"><c r="C3" t="e"><v>#REF!</v></c><c r="D3" t="e"><v>#REF!</v></c></row>
    <row r="4"><c r="C4"><f t="dataTable" ref="C4:C6" dt2D="0" dtr="0" r1="A1"/><v>10</v></c></row>
    <row r="5"><c r="C5"><v>20</v></c><c r="E5" s="0"><f t="dataTable" ref="E5:G5" dt2D="0" dtr="1" r1="B1" ca="1"/><v>7</v></c><c r="F5"><v>8</v></c><c r="G5"><v>9</v></c></row>
    <row r="6"><c r="C6"><v>30</v></c></row>
    <row r="8"><c r="H8" t="str"><f t="dataTable" ref="H8" dt2D="1" dtr="1" r1="B1" r2="A1"/><v>Z10</v></c></row>
    <row r="9"><c r="H9" t="e"><f t="dataTable" ref="H9:H10" dt2D="0" dtr="1" del1="1" r1="B1"/><v>#REF!</v></c></row>
    <row r="10"><c r="H10" t="e"><v>#REF!</v></c></row>
</sheetData>"#;

fn expected() -> Vec<DataTable> {
    vec![
        DataTable { range: rr("C2:D3"), two_d: true, row: true, r1: Some(at("A2")), r2: Some(at("A1")), del1: true, del2: true, ca: false },
        DataTable { range: rr("C4:C6"), two_d: false, row: false, r1: Some(at("A1")), r2: None, del1: false, del2: false, ca: false },
        DataTable { range: rr("E5:G5"), two_d: false, row: true, r1: Some(at("B1")), r2: None, del1: false, del2: false, ca: true },
        DataTable { range: rr("H8"), two_d: true, row: true, r1: Some(at("B1")), r2: Some(at("A1")), del1: false, del2: false, ca: false },
        DataTable { range: rr("H9:H10"), two_d: false, row: true, r1: Some(at("B1")), r2: None, del1: true, del2: false, ca: false },
    ]
}

fn sorted(mut v: Vec<DataTable>) -> Vec<DataTable> {
    v.sort_by_key(|t| (t.range.start.row, t.range.start.col));
    v
}

fn read() -> Workbook {
    let (wb, rep) = read_xlsx(&minimal(BODY, &[], "", "")).unwrap();
    assert!(!rep.warnings.iter().any(|w| w.contains("data table")), "no data-table warning: {:?}", rep.warnings);
    wb
}

fn sheet_xml(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
}

#[test]
fn data_tables_are_read_with_every_attribute_and_values_kept() {
    let wb = read();
    let s = wb.sheet(0).unwrap();
    assert_eq!(sorted(s.data_tables.clone()), expected());
    assert_eq!(s.value(at("C4")), Value::Number(10.0));
    assert_eq!(s.value(at("C6")), Value::Number(30.0));
    assert_eq!(s.value(at("G5")), Value::Number(9.0));
    assert_eq!(s.value(at("H8")), Value::text("Z10"));
    assert_eq!(s.value(at("C2")), Value::Error(CellError::Ref));
    for a in ["C2", "C4", "E5", "H8", "H9"] {
        assert!(s.cell(at(a)).and_then(|c| c.formula.as_ref()).is_none(), "{a}: a data table is not an ordinary formula");
    }
}

#[test]
fn data_tables_are_written_as_excel_writes_them() {
    let xml = sheet_xml(&read());
    for f in [
        r#"<f t="dataTable" ref="C2:D3" dt2D="1" dtr="1" del1="1" del2="1" r1="A2" r2="A1"/>"#,
        r#"<f t="dataTable" ref="C4:C6" dt2D="0" dtr="0" r1="A1"/>"#,
        r#"<f t="dataTable" ref="E5:G5" dt2D="0" dtr="1" r1="B1" ca="1"/>"#,
        r#"<f t="dataTable" ref="H8" dt2D="1" dtr="1" r1="B1" r2="A1"/>"#,
        r#"<f t="dataTable" ref="H9:H10" dt2D="0" dtr="1" del1="1" r1="B1"/>"#,
    ] {
        assert_eq!(xml.matches(f).count(), 1, "missing or repeated {f} in {xml}");
    }
    assert_eq!(xml.matches("t=\"dataTable\"").count(), 5, "only the top-left result cell carries the table");
    // Each <f> sits in its table's top-left cell, before the cached value.
    let c4 = xml.find(r#"<c r="C4""#).unwrap();
    let f4 = xml.find(r#"ref="C4:C6""#).unwrap();
    let c5 = xml.find(r#"<c r="C5""#).unwrap();
    assert!(c4 < f4 && f4 < c5);
}

#[test]
fn data_tables_survive_two_saves() {
    let wb = read();
    let (wb2, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let (wb3, _) = read_xlsx(&write_xlsx(&wb2).unwrap()).unwrap();
    assert_eq!(sorted(wb3.sheet(0).unwrap().data_tables.clone()), expected());
    assert_eq!(wb3.sheet(0).unwrap().value(at("F5")), Value::Number(8.0));
}

#[test]
fn table_whose_top_left_cell_has_no_stored_value_still_writes_its_formula() {
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().data_tables.push(DataTable {
        range: rr("B2:B3"),
        two_d: false,
        row: false,
        r1: Some(at("A1")),
        r2: None,
        del1: false,
        del2: false,
        ca: false,
    });
    let xml = sheet_xml(&wb);
    assert!(xml.contains(r#"<c r="B2"><f t="dataTable" ref="B2:B3" dt2D="0" dtr="0" r1="A1"/></c>"#), "{xml}");
}

#[test]
fn sheets_without_data_tables_write_no_data_table_formula() {
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().cells.set(at("A1"), gridcraft_model::Cell::value(Value::Number(1.0)));
    assert!(wb.sheet(0).unwrap().data_tables.is_empty());
    assert!(!sheet_xml(&wb).contains("dataTable"));
}

/// A table whose top-left result is an error, text or a boolean: the saved sheet XML must stay
/// well formed (the `t` attribute belongs on the `<c>` start tag, before the `<f>`), and reopening
/// keeps the table and the cached value and type.
fn error_text_bool_body() -> Vec<u8> {
    minimal(
        r#"<sheetData>
    <row r="2"><c r="B2" t="e"><f t="dataTable" ref="B2:B3" dt2D="0" dtr="0" r1="A1"/><v>#DIV/0!</v></c><c r="B3" t="e"><v>#N/A</v></c></row>
    <row r="5"><c r="C5" t="str"><f t="dataTable" ref="C5:C6" dt2D="0" dtr="1" r1="A1"/><v>text</v></c><c r="C6" t="str"><v>more</v></c></row>
    <row r="8"><c r="D8" t="b"><f t="dataTable" ref="D8:D9" dt2D="0" dtr="0" ca="1" r1="A1"/><v>1</v></c><c r="D9" t="b"><v>0</v></c></row>
</sheetData>"#,
        &[],
        "",
        "",
    )
}

#[test]
fn a_table_with_an_error_text_or_bool_top_left_writes_well_formed_xml() {
    let (wb, _) = read_xlsx(&error_text_bool_body()).unwrap();
    let xml = sheet_xml(&wb);
    // The `t` attribute sits on the `<c>` start tag, before the `<f>`, as in a normal cell.
    assert!(xml.contains(r#"<c r="B2" t="e"><f t="dataTable" ref="B2:B3" dt2D="0" dtr="0" r1="A1"/><v>#DIV/0!</v></c>"#), "{xml}");
    assert!(xml.contains(r#"<c r="C5" t="s"><f t="dataTable" ref="C5:C6" dt2D="0" dtr="1" r1="A1"/><v>"#), "{xml}");
    assert!(xml.contains(r#"<c r="D8" t="b"><f t="dataTable" ref="D8:D9" dt2D="0" dtr="0" r1="A1" ca="1"/><v>1</v></c>"#), "{xml}");
    // The saved XML parses as XML.
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut ok = false;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => {
                ok = true;
                break;
            }
            Err(_) => break,
            Ok(_) => {}
        }
    }
    assert!(ok, "the saved sheet XML must parse: {xml}");
    assert_eq!(xml.matches("t=\"dataTable\"").count(), 3, "{xml}");
}

#[test]
fn a_table_with_an_error_text_or_bool_top_left_reopens_with_table_and_value() {
    let (wb, _) = read_xlsx(&error_text_bool_body()).unwrap();
    let (wb2, rep) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(!rep.warnings.iter().any(|w| w.contains("data table")), "no data-table warning: {:?}", rep.warnings);
    let s = wb2.sheet(0).unwrap();
    assert_eq!(
        sorted(s.data_tables.clone()),
        vec![
            DataTable { range: rr("B2:B3"), two_d: false, row: false, r1: Some(at("A1")), r2: None, del1: false, del2: false, ca: false },
            DataTable { range: rr("C5:C6"), two_d: false, row: true, r1: Some(at("A1")), r2: None, del1: false, del2: false, ca: false },
            DataTable { range: rr("D8:D9"), two_d: false, row: false, r1: Some(at("A1")), r2: None, del1: false, del2: false, ca: true },
        ]
    );
    assert_eq!(s.value(at("B2")), Value::Error(CellError::Div0));
    assert_eq!(s.value(at("B3")), Value::Error(CellError::NA));
    assert_eq!(s.value(at("C5")), Value::text("text"));
    assert_eq!(s.value(at("C6")), Value::text("more"));
    assert_eq!(s.value(at("D8")), Value::Bool(true));
    assert_eq!(s.value(at("D9")), Value::Bool(false));
}
