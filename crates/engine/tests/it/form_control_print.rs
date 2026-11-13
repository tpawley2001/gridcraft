//! Form controls (check boxes, buttons, ...) print with the sheet when their "Print object" setting
//! is on, and extend the automatic print range like pictures and shapes, as Excel does.
//!
//! Measured in Excel: a sheet whose only content is a form check box anchored in C517 prints as
//! 10 pages (9 blank pages, then the check box and its caption on page 10).

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const CAPTION: &str = "form control inside cell b517";

fn pages(s: &mut Session) -> Vec<Json> {
    s.execute("file.printPreview", json!({})).unwrap()["pageList"].as_array().unwrap().clone()
}

/// `"12:40"` (or `"7"`) -> (12, 40).
fn rows(page: &Json) -> (u32, u32) {
    let r = page["rows"].as_str().unwrap();
    let (a, b) = r.split_once(':').unwrap_or((r, r));
    (a.parse().unwrap(), b.parse().unwrap())
}

fn pdf(s: &mut Session, params: Json) -> (u64, String) {
    let out = s.execute("file.exportPdf", params).unwrap();
    let bytes = gridcraft_engine::io::base64_decode(out["base64"].as_str().unwrap()).unwrap();
    (out["pages"].as_u64().unwrap(), String::from_utf8_lossy(&bytes).into_owned())
}

/// A new sheet with only a check box in C517.
fn control_only() -> (Session, u64) {
    let mut s = Session::new();
    s.new_workbook();
    let r = s.execute("insert.formCheckBox", json!({"at": "C517", "text": CAPTION, "value": 1})).unwrap();
    let id = r["control"].as_u64().unwrap();
    (s, id)
}

#[test]
fn a_control_far_down_an_empty_sheet_prints_every_page_from_a1_down_to_it() {
    let (mut s, _) = control_only();
    let p = pages(&mut s);
    assert!(p.len() > 1, "blank pages above the control print too: {p:?}");
    assert_eq!(rows(&p[0]).0, 1, "printing starts at row 1: {p:?}");
    // The 20 px check box sits at the top of row 517 and its bottom edge lies on row 518's top,
    // which counts toward the printed range like an anchor ending at a cell's top-left corner.
    // At 47 rows a page (Calibri 11), row 517 ends page 11 and row 518 is alone on the last page.
    let last = p.last().unwrap();
    assert_eq!(rows(last), (518, 518), "the last page holds the row the control's edge reaches: {last:?}");
    let before = &p[p.len() - 2];
    let (a, b) = rows(before);
    assert!(a <= 517 && 517 <= b, "the page before holds row 517, the control's row: {before:?}");
    let cols = p[0]["columns"].as_str().unwrap();
    assert!(cols.starts_with("A:") && cols != "A:A" && cols != "A:B", "columns reach the control in C: {cols}");
}

#[test]
fn the_control_and_its_caption_are_drawn_in_the_pdf() {
    let (mut s, _) = control_only();
    let preview = pages(&mut s).len() as u64;
    let (n, text) = pdf(&mut s, json!({}));
    assert_eq!(n, preview);
    assert!(text.contains(&format!("({CAPTION}) Tj")), "caption printed");
}

#[test]
fn a_control_only_sheet_is_not_skipped_when_printing_all_sheets() {
    let (mut s, _) = control_only();
    let (n, text) = pdf(&mut s, json!({"sheets": "all"}));
    assert!(n > 1, "{n}");
    assert!(text.contains(&format!("({CAPTION}) Tj")));
}

#[test]
fn print_object_off_leaves_the_control_out() {
    let (mut s, id) = control_only();
    s.execute("control.format", json!({"id": id, "print": false})).unwrap();
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(rows(&p[0]), (1, 1), "{p:?}");
    let (_, text) = pdf(&mut s, json!({}));
    assert!(!text.contains(&format!("({CAPTION}) Tj")));
}

#[test]
fn a_control_below_the_data_extends_the_range() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"ref": "A1", "value": "x"})).unwrap();
    s.execute("insert.button", json!({"at": "B60", "text": "Run"})).unwrap();
    let p = pages(&mut s);
    let last = rows(p.last().unwrap()).1;
    assert!(last >= 60, "range reaches the button in row 60: {p:?}");
    let (_, text) = pdf(&mut s, json!({}));
    assert!(text.contains("(Run) Tj"), "button caption printed");
}

#[test]
fn a_print_area_still_wins_over_controls() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"ref": "A1", "value": "x"})).unwrap();
    s.execute("insert.button", json!({"at": "B60", "text": "Run"})).unwrap();
    s.execute("pageLayout.printArea", json!({"range": "A1:B2"})).unwrap();
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0]["range"], "A1:B2");
}
