//! Drawing a column cut to the page width: a continuation page shows only the text running on
//! past the cut, shifted left by the printable width; the cell's fill stays on the first page.
use serde_json::json;

use super::*;

/// Letter portrait with 0.5 in side margins (540 pt printable) and column A 108.57 characters
/// wide, A1 holding a text of 30 words.
fn sheet() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("pageLayout.pageSetup", json!({"left": 0.5, "right": 0.5})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 108.57})).unwrap();
    let text = (1..=30).map(|i| format!("w{i:03}")).collect::<Vec<_>>().join(" ");
    s.execute("cell.set", json!({"cell": "A1", "input": text})).unwrap();
    s
}

fn pdf_pages(s: &mut Session) -> Vec<String> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let pdf = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    text.split("endstream").filter(|part| part.contains("stream\n")).map(str::to_string).collect()
}

/// The x of the first text drawn with `Tj` that starts with `w001`.
fn line_x(page: &str) -> Option<f32> {
    let before = &page[..page.find("(w001 ")?];
    let tm = before.rfind(" Tm")?;
    let mut nums = before[..tm].rsplit([' ', '\n']);
    let _y = nums.next()?;
    nums.next()?.parse().ok()
}

#[test]
fn a_continuation_page_shows_the_text_shifted_by_the_printable_width() {
    let mut s = sheet();
    let pages = pdf_pages(&mut s);
    assert_eq!(pages.len(), 2);
    let first = line_x(&pages[0]).expect("the text on page 1");
    let second = line_x(&pages[1]).expect("the text on page 2");
    assert!((36.0..46.0).contains(&first), "{first}");
    assert!((first - second - 540.0).abs() < 0.5, "{first} {second}");
}

#[test]
fn a_continuation_page_leaves_out_the_cell_fill() {
    let mut s = sheet();
    s.execute("home.fillColor", json!({"range": "A1", "color": "#FFFF00"})).unwrap();
    let pages = pdf_pages(&mut s);
    assert_eq!(pages.len(), 2);
    // Yellow is set as a fill colour on the first page only.
    assert!(pages[0].contains("1 1 0 rg"), "the fill on page 1");
    assert!(!pages[1].contains("1 1 0 rg"), "no fill on page 2");
    assert!(line_x(&pages[1]).is_some(), "the text on page 2");
}
