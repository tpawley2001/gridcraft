//! Text running on past a column cut to the page width continues across the pages of the columns
//! printed after it, not on pages of its own (measured from Microsoft 365 Excel's own PDF prints,
//! 2026-11-13: after the cut column's page the next page starts at the next column, and the long
//! text runs on across it shifted left by exactly one printable width per page).
use serde_json::json;

use super::*;

/// Letter portrait with 0.5 in side margins: 612 - 2 * 36 pt printable.
const PRINTABLE_WIDTH: f32 = 540.0;

/// `w001 w002 ...`: 30 four-letter words, longer than one printable width and shorter than two.
fn words() -> String {
    (1..=30).map(|i| format!("w{i:03}")).collect::<Vec<_>>().join(" ")
}

/// 0.5 in side margins, column A 108.57 characters wide (wider than the page), A1 = `words()`,
/// and `other` = "x".
fn sheet(other: &str) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("pageLayout.pageSetup", json!({"left": 0.5, "right": 0.5})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 108.57})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": words()})).unwrap();
    s.execute("cell.set", json!({"cell": other, "input": "x"})).unwrap();
    s
}

fn ranges(s: &mut Session) -> Vec<String> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap().clone();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["range"].as_str().unwrap().to_string()).collect()
}

fn pdf_pages(s: &mut Session) -> Vec<String> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let pdf = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    let streams: Vec<String> = text.split("endstream").filter(|part| part.contains("stream\n")).map(str::to_string).collect();
    assert_eq!(streams.len() as u64, r["pages"].as_u64().unwrap());
    streams
}

/// The x position of the first `(text) Tj` in a page's content stream.
fn text_x(page: &str, text: &str) -> Option<f32> {
    let before = &page[..page.find(&format!("({text}) Tj"))?];
    let tm = before.rfind(" Tm")?;
    let mut nums = before[..tm].rsplit([' ', '\n']);
    let _y = nums.next()?;
    nums.next()?.parse().ok()
}

#[test]
fn cut_continuation_runs_on_across_the_next_columns_page() {
    // The text runs over the empty B1 and stops at C1: Excel prints A, then B:C. No page of its own.
    let mut s = sheet("C1");
    let ranges = ranges(&mut s);
    assert_eq!(ranges.len(), 2, "Excel prints 2 pages: {ranges:?}");
    assert_eq!(ranges[0], "A1");
    assert!(ranges[1].starts_with("B1"), "B starts page 2: {ranges:?}");
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(r["pages"], 2);
}

#[test]
fn cut_continuation_runs_on_across_several_empty_columns() {
    // B1:D1 empty, E1 = "x": still 2 pages, A then B:E.
    let mut s = sheet("E1");
    let ranges = ranges(&mut s);
    assert_eq!(ranges.len(), 2, "Excel prints 2 pages: {ranges:?}");
    assert_eq!(ranges[0], "A1");
    assert!(ranges[1].starts_with("B1"), "B starts page 2: {ranges:?}");
}

#[test]
fn cut_continuation_draws_the_text_on_the_next_columns_page_shifted_by_the_printable_width() {
    // Excel: page 2 starts at column B and shows the same line, every word exactly one printable
    // width left of where it was on page 1.
    let text = words();
    let mut s = sheet("C1");
    let pages = pdf_pages(&mut s);
    assert_eq!(pages.len(), 2);
    let first = text_x(&pages[0], &text).expect("the line is drawn on page 1");
    let second = text_x(&pages[1], &text).expect("the line runs on across page 2 (column B's page)");
    assert!(
        (second - (first - PRINTABLE_WIDTH)).abs() < 0.5,
        "page 2 shows the line shifted left by the printable width: page 1 {first}, page 2 {second}"
    );
    assert!(pages[1].contains("(x) Tj"), "C1 is drawn on page 2 too");
}
