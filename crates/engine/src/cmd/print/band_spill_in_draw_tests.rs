//! Text that overflows from a cell left of a column band is drawn on that band's pages too,
//! shifted left by the columns before the band and clipped to the band (as Excel prints it).
use serde_json::json;

use super::*;

/// Letter portrait with 0.5 in side margins (540 pt printable); columns A and B 60 characters
/// wide, so A alone fills the first band; A1 holds 120 short words that run far past B.
fn sheet() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("pageLayout.pageSetup", json!({"left": 0.5, "right": 0.5})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "A:B", "chars": 60})).unwrap();
    let text = (1..=120).map(|i| format!("w{i:03}")).collect::<Vec<_>>().join(" ");
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
fn the_next_band_draws_the_text_running_into_it() {
    let mut s = sheet();
    let pages = pdf_pages(&mut s);
    assert!(pages.len() >= 2, "{} pages", pages.len());
    let first = line_x(&pages[0]).expect("the text on page 1");
    let second = line_x(&pages[1]).expect("the overflowing text on page 2");
    assert!((36.0..46.0).contains(&first), "{first}");
    // Column A (60 chars = 425 px = 318.75 pt) lies left of page 2's printable area.
    assert!(second < 36.0 - 300.0, "{second}");
    assert!((first - second - 318.75).abs() < 2.0, "{first} {second}");
}

#[test]
fn a_filled_neighbour_stops_the_text_before_the_next_band() {
    let mut s = sheet();
    s.execute("cell.set", json!({"cell": "B1", "input": "x"})).unwrap();
    let pages = pdf_pages(&mut s);
    assert!(pages.len() >= 2, "{} pages", pages.len());
    assert!(line_x(&pages[0]).is_some(), "the text on page 1");
    assert!(line_x(&pages[1]).is_none(), "no A1 text on page 2");
}
