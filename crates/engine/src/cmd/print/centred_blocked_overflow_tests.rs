//! A centred cell whose text is wider than its column spills nothing past the last used column
//! when the cell to its left holds a value, so it adds no column band to the printed range (as
//! Excel prints). Same probe page setup as `overflow_past_last_col_tests.rs`: Letter portrait,
//! 0.5" margins, numbers in A1, L1 and A120 (bands A:J and K:L). Text in K120 makes the K band
//! print on all three row bands, so the tests compare the printed column bands, not the page count.
use serde_json::json;

use super::*;

/// Numbers in A1, L1 and A120 under the probe page setup.
fn base() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "pageLayout.pageSetup",
        json!({"paper": "Letter", "orientation": "portrait", "margins": {"left": 0.5, "right": 0.5, "top": 0.5, "bottom": 0.5}, "header": "&A p&P"}),
    )
    .unwrap();
    for c in ["A1", "L1", "A120"] {
        s.execute("cell.set", json!({"cell": c, "input": "1"})).unwrap();
    }
    s
}

fn set(s: &mut Session, cell: &str, text: &str) {
    s.execute("range.setValues", json!({"range": cell, "values": [[text]]})).unwrap();
}

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

/// The pages when K120 holds text: A:J then `second` over the row bands 1:49, 50:98 and 99:120
/// (K120's own page prints, and so does the blank K 50:98 page before it).
fn with_k120_bands(second: &str) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for cols in ["A:J", second] {
        for rows in ["1:49", "50:98", "99:120"] {
            v.push((cols.to_string(), rows.to_string()));
        }
    }
    v
}

/// 200 characters of "abcdefghi " in K120 (a single line far wider than column K).
fn long_text_in_k120(centred: bool) -> Session {
    let mut s = base();
    let text: String = "abcdefghi ".repeat(20).chars().take(200).collect();
    set(&mut s, "K120", &text);
    if centred {
        s.execute("home.alignCenter", json!({"range": "K120"})).unwrap();
    }
    s
}

#[test]
fn centred_text_with_filled_left_neighbour_adds_no_band() {
    let mut s = long_text_in_k120(true);
    s.execute("cell.set", json!({"cell": "J120", "input": "1"})).unwrap();
    assert_eq!(page_list(&mut s), with_k120_bands("K:L"));
}

#[test]
fn centred_text_with_text_left_neighbour_adds_no_band() {
    let mut s = long_text_in_k120(true);
    set(&mut s, "J120", "x");
    assert_eq!(page_list(&mut s), with_k120_bands("K:L"));
}

#[test]
fn left_aligned_text_with_filled_left_neighbour_still_widens() {
    let mut s = long_text_in_k120(false);
    s.execute("cell.set", json!({"cell": "J120", "input": "1"})).unwrap();
    let list = page_list(&mut s);
    assert_eq!(list.len(), 9, "{list:?}");
}

#[test]
fn centred_text_with_empty_left_neighbour_still_widens() {
    let mut s = long_text_in_k120(true);
    assert_eq!(page_list(&mut s), with_k120_bands("K:T"));
}
