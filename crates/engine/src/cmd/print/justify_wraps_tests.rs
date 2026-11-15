//! Justified and distributed text (horizontal or vertical) folds inside its cell when printed,
//! as wrapped text does, so it neither widens the printed range past the last used column nor
//! adds strips to a column wider than the page. Page setup of `overflow_past_last_col_tests.rs`:
//! Letter portrait, 0.5" margins, Calibri 11, default column widths; numbers in A1, L1 and A120,
//! so without overflow the printed column bands are A:J and K:L.
use serde_json::json;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "pageLayout.pageSetup",
        json!({"paper": "Letter", "orientation": "portrait", "margins": {"left": 0.5, "right": 0.5, "top": 0.5, "bottom": 0.5}}),
    )
    .unwrap();
    s
}

/// "abcdefghi " repeated: `n` characters.
fn words(n: usize) -> String {
    "abcdefghi ".repeat(n / 10 + 1).chars().take(n).collect()
}

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

/// The printed pages with 200 characters of text in K120 under the alignment `align`
/// (partial alignment JSON for `home.formatCells`; `None` leaves the cell General).
fn pages_with_k120(align: Option<serde_json::Value>) -> Vec<(String, String)> {
    let mut s = session();
    for c in ["A1", "L1", "A120"] {
        s.execute("cell.set", json!({"cell": c, "input": "1"})).unwrap();
    }
    s.execute("range.setValues", json!({"range": "K120", "values": [[words(200)]]})).unwrap();
    if let Some(a) = align {
        s.execute("home.formatCells", json!({"range": "K120", "style": {"align": a}})).unwrap();
    }
    page_list(&mut s)
}

fn bands(pages: &[(String, String)]) -> Vec<String> {
    let mut b: Vec<String> = pages.iter().map(|p| p.0.clone()).collect();
    b.dedup();
    b.sort();
    b.dedup();
    b
}

#[test]
fn left_aligned_text_still_widens_the_printed_range() {
    let pages = pages_with_k120(None);
    assert!(bands(&pages).iter().any(|b| b.starts_with("U:")), "{pages:?}");
}

#[test]
fn wrapped_text_keeps_the_last_used_column() {
    let pages = pages_with_k120(Some(json!({"wrap": true})));
    assert_eq!(bands(&pages), ["A:J", "K:L"], "{pages:?}");
}

#[test]
fn justified_and_distributed_text_prints_like_wrapped_text() {
    let wrapped = pages_with_k120(Some(json!({"wrap": true})));
    for align in [json!({"h": "Justify"}), json!({"h": "Distributed"}), json!({"v": "Justify"}), json!({"v": "Distributed"})] {
        let pages = pages_with_k120(Some(align.clone()));
        assert_eq!(bands(&pages), ["A:J", "K:L"], "{align}: {pages:?}");
        assert_eq!(pages, wrapped, "{align}: same pages as Wrap Text");
    }
}

/// Column A wider than the page (120 characters) holding 600 characters: justified text folds
/// inside A, so A prints in as many strips as with Wrap Text, and fewer than unwrapped text that
/// runs on to the right of A.
#[test]
fn justified_text_in_a_column_wider_than_the_page_adds_no_strips() {
    let pages = |align: Option<serde_json::Value>| {
        let mut s = session();
        s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 120})).unwrap();
        s.execute("range.setValues", json!({"range": "A1", "values": [[words(600)]]})).unwrap();
        if let Some(a) = align {
            s.execute("home.formatCells", json!({"range": "A1", "style": {"align": a}})).unwrap();
        }
        page_list(&mut s).len()
    };
    let (wrapped, justified, left) = (pages(Some(json!({"wrap": true}))), pages(Some(json!({"h": "Justify"}))), pages(None));
    assert_eq!(justified, wrapped);
    assert!(left > wrapped, "left {left}, wrapped {wrapped}");
}
