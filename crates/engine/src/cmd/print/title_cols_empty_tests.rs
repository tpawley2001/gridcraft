//! Repeated title columns that hold nothing printable leave the printed pages as without titles:
//! the later bands of columns stop after their last page with something on it.
use serde_json::json;

use super::*;

#[test]
fn empty_title_columns_keep_trimming_the_later_bands() {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (1..=150).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "B1", "values": rows})).unwrap();
    s.execute("cell.set", json!({"cell": "Z1", "input": "z"})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"titleCols": "A:A"})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let pages: Vec<(String, String)> = pv["pageList"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string()))
        .collect();
    let first_cols = &pages[0].0;
    let first_band = pages.iter().filter(|p| &p.0 == first_cols).count();
    let last_cols = &pages.last().unwrap().0;
    let last_band: Vec<_> = pages.iter().filter(|p| &p.0 == last_cols).collect();
    assert!(first_band >= 4 && last_cols != first_cols, "{pages:?}");
    assert_eq!(last_band.len(), 1, "{pages:?}");
    assert_eq!(last_band[0].1, pages[0].1, "{pages:?}");
}
