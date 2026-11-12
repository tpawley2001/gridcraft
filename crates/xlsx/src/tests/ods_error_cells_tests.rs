//! ODS cells saved as errors are read as error values, not as text. Two producers: Microsoft 365
//! Excel writes `office:value-type="error"` with the error text in `office:string-value`;
//! LibreOffice-family writers mark a string cell with `calcext:value-type="error"`.
//! Original ODS packages assembled in Rust.

use gridcraft_core::{CellError, CellRef, Value};

use super::make_zip;
use crate::read_ods;

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;
const ERRORS: [(&str, &str, CellError); 7] = [
    ("#NULL!", "of:=[.A1] [.B2]", CellError::Null),
    ("#DIV/0!", "of:=1/0", CellError::Div0),
    ("#VALUE!", "of:=1+&quot;x&quot;", CellError::Value),
    ("#REF!", "of:=INDEX([.A1:.A2];5)", CellError::Ref),
    ("#NAME?", "of:=NOSUCHFUNCTION()", CellError::Name),
    ("#NUM!", "of:=SQRT(-1)", CellError::Num),
    ("#N/A", "of:=NA()", CellError::NA),
];

/// One cell per row, in column A.
fn ods(cells: &[String]) -> Vec<u8> {
    let rows: String = cells.iter().map(|c| format!("<table:table-row>{c}</table:table-row>")).collect();
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2" xmlns:calcext="urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0"><office:body><office:spreadsheet><table:table table:name="Sheet1">{rows}</table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    make_zip(&[("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_bytes())])
}

/// The cell as Microsoft 365 Excel saves it.
fn excel_cell(text: &str, formula: &str) -> String {
    format!(
        r#"<table:table-cell office:value-type="error" office:string-value="{text}" table:formula="{formula}"><text:p>{text}</text:p></table:table-cell>"#
    )
}

/// The cell as LibreOffice-family writers save it.
fn calcext_cell(text: &str, formula: &str) -> String {
    format!(
        r#"<table:table-cell table:formula="{formula}" office:value-type="string" office:string-value="" calcext:value-type="error"><text:p>{text}</text:p></table:table-cell>"#
    )
}

fn unsupported(warnings: &[String]) -> bool {
    warnings.iter().any(|w| w.contains("unsupported or missing"))
}

#[test]
fn ods_error_cells_saved_by_excel_are_error_values() {
    let cells: Vec<String> = ERRORS.iter().map(|(text, formula, _)| excel_cell(text, formula)).collect();
    let (wb, report) = read_ods(&ods(&cells)).unwrap();
    assert!(!unsupported(&report.warnings), "{:?}", report.warnings);
    for (row, (text, _, error)) in ERRORS.iter().enumerate() {
        let at = CellRef::new(row as u32, 0);
        assert_eq!(wb.sheets[0].value(at), Value::Error(*error), "{text}");
        assert!(wb.sheets[0].cell(at).unwrap().formula.is_some(), "{text}: the formula is kept");
    }
}

#[test]
fn ods_error_cells_marked_with_calcext_are_error_values() {
    let cells: Vec<String> = ERRORS.iter().map(|(text, formula, _)| calcext_cell(text, formula)).collect();
    let (wb, report) = read_ods(&ods(&cells)).unwrap();
    assert!(!unsupported(&report.warnings), "{:?}", report.warnings);
    for (row, (text, _, error)) in ERRORS.iter().enumerate() {
        assert_eq!(wb.sheets[0].value(CellRef::new(row as u32, 0)), Value::Error(*error), "{text}");
    }
}

#[test]
fn ods_error_cell_without_displayed_text_or_formula_uses_the_string_value() {
    let cells = [
        r##"<table:table-cell office:value-type="error" office:string-value="#REF!" table:formula="of:=INDEX([.B1:.B2];5)"/>"##.to_string(),
        r##"<table:table-cell office:value-type="error" office:string-value="#N/A"><text:p>#N/A</text:p></table:table-cell>"##.to_string(),
    ];
    let (wb, report) = read_ods(&ods(&cells)).unwrap();
    assert!(!unsupported(&report.warnings), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Error(CellError::Ref));
    assert_eq!(wb.sheets[0].value(CellRef::new(1, 0)), Value::Error(CellError::NA));
    assert!(wb.sheets[0].cell(CellRef::new(1, 0)).unwrap().formula.is_none());
}

#[test]
fn ods_unknown_error_text_and_plain_strings_stay_text() {
    let cells = [
        calcext_cell("Err:502", "of:=SQRT(-1)"),
        excel_cell("#FEHLER!", "of:=1/0"),
        r##"<table:table-cell office:value-type="string"><text:p>#DIV/0!</text:p></table:table-cell>"##.to_string(),
    ];
    let (wb, report) = read_ods(&ods(&cells)).unwrap();
    assert!(unsupported(&report.warnings), "unknown error texts keep today's warning: {:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Text("Err:502".into()));
    assert_eq!(wb.sheets[0].value(CellRef::new(1, 0)), Value::Text("#FEHLER!".into()));
    assert_eq!(wb.sheets[0].value(CellRef::new(2, 0)), Value::Text("#DIV/0!".into()));
}
