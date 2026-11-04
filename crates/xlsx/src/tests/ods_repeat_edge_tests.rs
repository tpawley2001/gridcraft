//! ODS rows with data whose repeat count runs past the last worksheet row: read once, as Excel does.
//! Original ODS packages assembled in Rust.

use gridcraft_core::{CellRef, Value};

use super::make_zip;
use crate::{IoError, read_ods};

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;

fn ods(rows: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet><table:table table:name="Sheet1">{rows}</table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    make_zip(&[("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_bytes())])
}

fn num(v: u32) -> String {
    format!(r#"<table:table-cell office:value-type="float" office:value="{v}"><text:p>{v}</text:p></table:table-cell>"#)
}

#[test]
fn ods_data_row_repeated_past_the_last_row_is_read_once() {
    let row = format!(r#"<table:table-row table:number-rows-repeated="5000000">{}{}{}</table:table-row>"#, num(1), num(2), num(3));
    let (wb, report) = read_ods(&ods(&row)).expect("Excel opens this file");
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.value(CellRef::new(0, 0)), Value::Number(1.0));
    assert_eq!(sheet.value(CellRef::new(0, 1)), Value::Number(2.0));
    assert_eq!(sheet.value(CellRef::new(0, 2)), Value::Number(3.0));
    assert_eq!(sheet.value(CellRef::new(1, 0)), Value::Empty);
    assert_eq!(sheet.cells.len(), 3);
    assert!(report.warnings.iter().any(|w| w.to_ascii_lowercase().contains("repeated")), "{:?}", report.warnings);
}

#[test]
fn ods_rows_after_a_shortened_repeat_continue_on_the_next_row() {
    let rows =
        format!(r#"<table:table-row table:number-rows-repeated="5000000">{}</table:table-row><table:table-row>{}</table:table-row>"#, num(1), num(9));
    let (wb, _) = read_ods(&ods(&rows)).expect("opens");
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(1.0));
    assert_eq!(wb.sheets[0].value(CellRef::new(1, 0)), Value::Number(9.0));
    assert_eq!(wb.sheets[0].cells.len(), 2);
}

#[test]
fn ods_repeats_that_fit_and_blank_edge_rules_are_unchanged() {
    let (wb, _) = read_ods(&ods(&format!(r#"<table:table-row table:number-rows-repeated="3">{}</table:table-row>"#, num(7)))).unwrap();
    for r in 0..3 {
        assert_eq!(wb.sheets[0].value(CellRef::new(r, 0)), Value::Number(7.0));
    }
    assert_eq!(wb.sheets[0].cells.len(), 3);
    // Blank rows reaching the edge, then data: still refused, never silently dropped.
    let late = format!(
        r#"<table:table-row>{}</table:table-row><table:table-row table:number-rows-repeated="1048575"><table:table-cell/></table:table-row><table:table-row>{}</table:table-row>"#,
        num(1),
        num(2)
    );
    assert!(matches!(read_ods(&ods(&late)), Err(IoError::TooLarge(_))));
}
