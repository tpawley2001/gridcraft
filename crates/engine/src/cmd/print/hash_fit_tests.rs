//! A number or date prints in full when its digits fit the column in the cell font's own widths,
//! as Excel prints it; `#` fills the cell only when it truly does not fit. New workbook: Normal
//! font Calibri 11. Column widths are in screen pixels (a stored width of 11 characters is 77 px,
//! 12 is 84 px, 8 is 56 px). Expected values from Excel 365's printed PDF of the same workbook.
use serde_json::json;

use super::*;

/// The PDF of a sheet whose A1 holds `input` formatted with `code`, column A `px` wide, B1 a text.
fn pdf_text(px: f64, input: &str, code: Option<&str>) -> String {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("home.columnWidth", json!({"cols": "A:A", "width": px})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": input})).unwrap();
    if let Some(code) = code {
        s.execute("home.numberFormat", json!({"range": "A1", "code": code})).unwrap();
    }
    s.execute("cell.set", json!({"cell": "B1", "input": "x"})).unwrap();
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let pdf = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    String::from_utf8_lossy(&pdf).into_owned()
}

const DATE: &str = "44498"; // 10/29/2021

#[test]
fn a_date_in_an_eleven_character_column_prints_in_full() {
    let pdf = pdf_text(77.0, DATE, Some("m/d/yyyy"));
    assert!(pdf.contains("(10/29/2021) Tj"), "the date should print in full");
    assert!(!pdf.contains("(#"), "no #### for a date that fits");
}

#[test]
fn a_date_in_a_twelve_character_column_prints_in_full() {
    let pdf = pdf_text(84.0, DATE, Some("m/d/yyyy"));
    assert!(pdf.contains("(10/29/2021) Tj"));
    assert!(!pdf.contains("(#"));
}

#[test]
fn a_date_in_an_eight_character_column_still_prints_hashes() {
    let pdf = pdf_text(56.0, DATE, Some("m/d/yyyy"));
    assert!(!pdf.contains("10/29/2021"), "the date does not fit 56 px");
    assert!(pdf.contains("(#"), "a number that does not fit fills its cell with #");
}

#[test]
fn a_ten_digit_general_number_prints_every_digit_in_an_eleven_character_column() {
    let pdf = pdf_text(77.0, "1234567890", None);
    assert!(pdf.contains("(1234567890) Tj"), "all 10 digits should print");
    assert!(!pdf.contains("(#"));
}

#[test]
fn text_that_does_not_fit_is_unchanged_and_never_hashed() {
    // Guard: strings keep today's clipping; B1 is filled, so A1's text is cut at the cell edge.
    let pdf = pdf_text(56.0, "10/29/2021xx", Some("@"));
    assert!(!pdf.contains("(#"), "text never turns into #");
}
