//! Additional data-table preservation cases.

use crate::package::Package;
use crate::tests::minimal;
use crate::{read_xlsx, write_xlsx};
use gridcraft_core::{CellError, CellRef, RangeRef, Value};
use gridcraft_model::{DataTable, Workbook};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rr(a: &str) -> RangeRef {
    RangeRef::parse(a).unwrap()
}

fn sorted(mut v: Vec<DataTable>) -> Vec<DataTable> {
    v.sort_by_key(|t| (t.range.start.row, t.range.start.col));
    v
}

fn sheet_xml(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
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

#[test]
fn ordinary_formula_takes_precedence_over_a_table_anchor() {
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap();
    sheet.data_tables.push(DataTable { range: rr("B2:B3"), ..DataTable::default() });
    sheet.cells.set(
        at("B2"),
        gridcraft_model::Cell {
            formula: Some(std::sync::Arc::new(gridcraft_model::Formula::new("1+2"))),
            ..gridcraft_model::Cell::value(Value::Number(3.0))
        },
    );
    let xml = sheet_xml(&wb);
    assert!(xml.contains("<f>1+2</f><v>3</v>"), "{xml}");
    assert!(!xml.contains("t=\"dataTable\""), "{xml}");
}

#[test]
fn missing_range_and_invalid_inputs_keep_the_anchor_and_boolean_flags() {
    let (wb, report) = read_xlsx(&minimal(
        r#"<sheetData><row r="5"><c r="C5" s="0"><f t="dataTable" dt2D="true" dtr="true" r1="invalid" r2="XFE1" del1="true" del2="true" ca="true"/><v>42</v></c></row></sheetData>"#,
        &[], "", "",
    )).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.data_tables, vec![DataTable { range: rr("C5"), two_d: true, row: true, r1: None, r2: None, del1: true, del2: true, ca: true }]);
    assert_eq!(sheet.value(at("C5")), Value::Number(42.0));
    assert!(!report.warnings.iter().any(|warning| warning.contains("data table")), "{:?}", report.warnings);
    let (reopened, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(reopened.sheet(0).unwrap().data_tables, sheet.data_tables);
}
