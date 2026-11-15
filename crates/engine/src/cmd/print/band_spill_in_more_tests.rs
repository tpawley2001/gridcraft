//! More cases of text overflowing from a cell left of a column band onto that band's pages.
use serde_json::json;

use super::*;

/// As in `band_spill_in_draw_tests`: columns A and B 60 characters wide on Letter portrait with
/// 0.5 in side margins, so A alone fills the first band; `cells` hold long lines.
fn sheet(cells: &[(&str, &str)]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("pageLayout.pageSetup", json!({"left": 0.5, "right": 0.5})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "A:B", "chars": 60})).unwrap();
    for (cell, prefix) in cells {
        let text = (1..=120).map(|i| format!("{prefix}{i:03}")).collect::<Vec<_>>().join(" ");
        s.execute("cell.set", json!({"cell": cell, "input": text})).unwrap();
    }
    s
}

fn pdf_pages(s: &mut Session) -> Vec<String> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let pdf = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    text.split("endstream").filter(|part| part.contains("stream\n")).map(str::to_string).collect()
}

/// The x of the first text drawn with `Tj` that starts with `start`.
fn line_x(page: &str, start: &str) -> Option<f32> {
    let before = &page[..page.find(&format!("({start} "))?];
    let tm = before.rfind(" Tm")?;
    let mut nums = before[..tm].rsplit([' ', '\n']);
    let _y = nums.next()?;
    nums.next()?.parse().ok()
}

#[test]
fn centred_text_runs_into_the_next_band_at_the_same_shift() {
    let mut s = sheet(&[("A1", "c")]);
    s.execute("home.alignCenter", json!({"range": "A1"})).unwrap();
    let pages = pdf_pages(&mut s);
    assert!(pages.len() >= 2, "{} pages", pages.len());
    let first = line_x(&pages[0], "c001").expect("the text on page 1");
    let second = line_x(&pages[1], "c001").expect("the centred text on page 2");
    assert!((first - second - 318.75).abs() < 2.0, "{first} {second}");
}

#[test]
fn every_row_with_overflowing_text_reaches_the_next_band() {
    let mut s = sheet(&[("A1", "a"), ("A3", "b")]);
    let pages = pdf_pages(&mut s);
    assert!(pages.len() >= 2, "{} pages", pages.len());
    assert!(line_x(&pages[1], "a001").is_some(), "row 1 on page 2");
    assert!(line_x(&pages[1], "b001").is_some(), "row 3 on page 2");
}

#[test]
fn a_hidden_source_row_draws_nothing_on_the_next_band() {
    let mut s = sheet(&[("A1", "a"), ("A3", "b")]);
    s.execute("home.hideRows", json!({"rows": "3:3"})).unwrap();
    let pages = pdf_pages(&mut s);
    assert!(pages.len() >= 2, "{} pages", pages.len());
    assert!(line_x(&pages[1], "a001").is_some(), "row 1 on page 2");
    assert!(line_x(&pages[1], "b001").is_none(), "hidden row 3 not on page 2");
}
