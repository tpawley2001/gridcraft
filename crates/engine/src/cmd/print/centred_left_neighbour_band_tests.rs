//! A centred cell whose text is wider than its column does not widen the printed column band
//! past the last used column when the cell to its left holds a value. Probe page setup of
//! `overflow_past_last_col_tests.rs`: Letter portrait, 0.5" margins, numbers in A1, L1 and A120.
use serde_json::json;

use super::*;

/// Column spans of the printed pages with K120 holding 200 characters of centred text and J120
/// holding `left` (or nothing).
fn columns_with_centred_k120(left: Option<&str>) -> Vec<String> {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "pageLayout.pageSetup",
        json!({"paper": "Letter", "orientation": "portrait", "margins": {"left": 0.5, "right": 0.5, "top": 0.5, "bottom": 0.5}}),
    )
    .unwrap();
    for c in ["A1", "L1", "A120"] {
        s.execute("cell.set", json!({"cell": c, "input": "1"})).unwrap();
    }
    let text: String = "abcdefghi ".repeat(20).chars().take(200).collect();
    s.execute("range.setValues", json!({"range": "K120", "values": [[text]]})).unwrap();
    s.execute("home.alignCenter", json!({"range": "K120"})).unwrap();
    if let Some(v) = left {
        s.execute("range.setValues", json!({"range": "J120", "values": [[v]]})).unwrap();
    }
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    pv["pageList"].as_array().unwrap().iter().map(|p| p["columns"].as_str().unwrap().to_string()).collect()
}

#[test]
fn filled_left_neighbour_keeps_the_band_at_the_last_used_column() {
    for left in ["1", "x"] {
        let cols = columns_with_centred_k120(Some(left));
        assert_eq!(cols.last().map(String::as_str), Some("K:L"), "{left}: {cols:?}");
    }
}

#[test]
fn empty_left_neighbour_still_widens_the_band() {
    let cols = columns_with_centred_k120(None);
    assert_eq!(cols.last().map(String::as_str), Some("K:T"), "{cols:?}");
}
