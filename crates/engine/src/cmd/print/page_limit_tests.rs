//! A long workbook prints every page, not only the first 2,000 (Excel prints them all: real
//! workbooks of 65,536 rows in two bands of columns are 2,600 to 2,900 pages), while a hostile
//! page setup still stops at `MAX_PAGES`.
//! Letter portrait with default margins, column widths and row heights; the rows on a page are
//! measured in the test, not assumed.
use serde_json::json;

use super::*;

fn s() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

/// Rows on one printed page of a default sheet, read from a three-page workbook.
fn rows_per_page() -> u32 {
    let mut s = s();
    s.execute("cell.set", json!({"cell": "A1", "input": "top"})).unwrap();
    s.execute("cell.set", json!({"cell": "A120", "input": "end"})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let first = pv["pageList"][0]["rows"].as_str().unwrap().to_string();
    let n: u32 = first.strip_prefix("1:").unwrap().parse().unwrap();
    assert!((30..=60).contains(&n), "{first}");
    assert_eq!(pv["pages"], 120_u32.div_ceil(n));
    n
}

/// A sheet with a value in A1 and in the last row of page `pages` (one band of columns).
fn tall(s: &mut Session, pages: u32) -> u32 {
    let last = rows_per_page() * pages;
    s.execute("cell.set", json!({"cell": "A1", "input": "top"})).unwrap();
    s.execute("cell.set", json!({"cell": format!("A{last}"), "input": "end"})).unwrap();
    last
}

fn pdf_of(s: &mut Session, p: Json) -> (Vec<u8>, u64) {
    let r = s.execute("file.exportPdf", p).unwrap();
    let bytes = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    (bytes, r["pages"].as_u64().unwrap())
}

/// Page objects in a PDF (`/Type /Page`, not the `/Type /Pages` tree nodes).
fn page_objects(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.matches("/Type /Page").count() - text.matches("/Type /Pages").count()
}

#[test]
fn a_sheet_of_2500_pages_prints_all_of_them() {
    let mut s = s();
    tall(&mut s, 2500);
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    assert_eq!(pv["pages"], 2500);
    let (pdf, pages) = pdf_of(&mut s, json!({}));
    assert_eq!(pages, 2500);
    assert_eq!(page_objects(&pdf), 2500);
}

#[test]
fn two_bands_of_columns_print_5000_pages() {
    let mut s = s();
    let last = tall(&mut s, 2500);
    // Column P is in the second band of columns across; its value is on that band's last page.
    s.execute("cell.set", json!({"cell": format!("P{last}"), "input": "far"})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    assert_eq!(pv["pages"], 5000);
    let (pdf, pages) = pdf_of(&mut s, json!({}));
    assert_eq!(pages, 5000);
    assert_eq!(page_objects(&pdf), 5000);
}

#[test]
fn the_limit_counts_one_export_and_both_long_sheets_print_whole() {
    let mut s = s();
    tall(&mut s, 1500);
    s.execute("home.insertSheet", json!({"name": "Second"})).unwrap();
    s.execute("sheet.activate", json!({"name": "Second"})).unwrap();
    tall(&mut s, 1500);
    let (pdf, pages) = pdf_of(&mut s, json!({"sheets": "all"}));
    assert_eq!(pages, 3000);
    assert_eq!(page_objects(&pdf), 3000);
}

#[test]
fn page_numbers_count_every_page() {
    let mut s = s();
    tall(&mut s, 2500);
    s.execute("pageLayout.pageSetup", json!({"footer": "&P of &N"})).unwrap();
    let (pdf, pages) = pdf_of(&mut s, json!({}));
    assert_eq!(pages, 2500);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("(1 of 2500) Tj"), "first page footer");
    assert!(text.contains("(2000 of 2500) Tj"), "page 2,000 footer");
    assert!(text.contains("(2500 of 2500) Tj"), "last page footer");
}

#[test]
fn a_hostile_page_setup_still_stops_at_the_limit() {
    let mut s = s();
    s.execute("cell.set", json!({"cell": "XFD1048576", "input": "far"})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "near"})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"scale": 400, "margins": {"left": 20, "right": 20, "top": 20, "bottom": 20}})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    assert!(pv["pages"].as_u64().unwrap() <= MAX_PAGES as u64);
    // No real workbook meets the limit (Excel printed 72,034 pages of one).
    assert!(std::hint::black_box(MAX_PAGES) >= 100_000);
}
