//! A column cut to the page whose text runs on past the cut prints continuation pages of its
//! own; Excel prints such a page only down to the last row band where some text of the column
//! reaches it (pages above that are kept even when blank). A short value in the column on a lower
//! row band does not pull the continuation strip down with it. Measured from Microsoft 365
//! Excel's own PDF print of a real workbook (2026-11-15): a column wider than the page with 134
//! row bands printed its second strip on only 28 of them, the 28th holding the last long text.
use serde_json::json;

use super::*;

/// `w001 w002 ...`: 30 four-letter words, longer than one printable width and shorter than two.
fn words() -> String {
    (1..=30).map(|i| format!("w{i:03}")).collect::<Vec<_>>().join(" ")
}

/// 0.5 in side margins, column A 108.57 characters wide (wider than the page), A1:A120 = "x"
/// except `long` (if any), which holds `words()`. 120 default rows make 3 row bands on Letter.
fn sheet(long: Option<u32>) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("pageLayout.pageSetup", json!({"left": 0.5, "right": 0.5})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 108.57})).unwrap();
    for r in 1..=120u32 {
        let input = if Some(r) == long { words() } else { "x".to_string() };
        s.execute("cell.set", json!({"cell": format!("A{r}"), "input": input})).unwrap();
    }
    s
}

/// (printPreview pages, exportPdf pages, distinct row ranges printed).
fn pages(s: &mut Session) -> (u64, u64, usize) {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap().clone();
    let mut ranges: Vec<String> = list.iter().map(|p| p["range"].as_str().unwrap().to_string()).collect();
    ranges.sort();
    ranges.dedup();
    let pdf = s.execute("file.exportPdf", json!({})).unwrap();
    (pv["pages"].as_u64().unwrap(), pdf["pages"].as_u64().unwrap(), ranges.len())
}

#[test]
fn no_long_text_prints_no_continuation_strip() {
    let mut s = sheet(None);
    let (pv, pdf, bands) = pages(&mut s);
    assert_eq!(bands, 3, "120 default rows make 3 row bands");
    assert_eq!((pv, pdf), (3, 3));
}

#[test]
fn long_text_on_the_first_row_band_prints_one_continuation_page() {
    let mut s = sheet(Some(1));
    let (pv, pdf, bands) = pages(&mut s);
    assert_eq!(bands, 3);
    assert_eq!((pv, pdf), (4, 4), "3 pages of the first strip + 1 continuation page (row band 1 only)");
}

#[test]
fn long_text_on_the_middle_row_band_keeps_the_blank_continuation_page_above_it() {
    let mut s = sheet(Some(60));
    let (pv, pdf, bands) = pages(&mut s);
    assert_eq!(bands, 3);
    assert_eq!((pv, pdf), (5, 5), "3 + continuation pages for row bands 1 (blank, kept) and 2");
}

#[test]
fn long_text_on_the_last_row_band_prints_every_continuation_page() {
    let mut s = sheet(Some(120));
    let (pv, pdf, bands) = pages(&mut s);
    assert_eq!(bands, 3);
    assert_eq!((pv, pdf), (6, 6), "3 + 3 continuation pages");
}
