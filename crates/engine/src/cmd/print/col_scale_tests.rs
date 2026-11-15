//! Columns paginate at their printed width, and Fit To scales are whole percents.
use serde_json::json;

use super::*;

/// Calibri 11: round(11 × 600 / 72) = 92 dots an em, × 1038/2048 = 46.6, so 47 dots (5.64pt) a
/// character at 600 dpi, over the 7 px (5.25pt) on screen: 5.64 / 5.25 = 1.0743.
#[test]
fn calibri_11_prints_columns_7_4_percent_wider() {
    let mut s = Session::new();
    s.new_workbook();
    let scale = layout::print_col_scale(&s.doc().unwrap().wb);
    assert!((scale - 1.0743).abs() <= 0.0005, "{scale}");
}

/// Fifteen default columns (1.5 Letter pages wide) fitted to one page wide: 511.2pt over
/// 15 × 48pt × 1.0743 = 773.5pt is 66.09 %, printed at 66 % (rounded down to a whole percent).
#[test]
fn fit_to_width_scale_is_a_whole_percent() {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (0..5).map(|r| (0..15).map(|c| json!(r * 100 + c)).collect()).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"fitWidth": 1, "fitHeight": 0})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let pct = pv["pageList"][0]["scale"].as_f64().unwrap();
    assert_eq!(pct, pct.round(), "{pct}");
    assert_eq!(pct, 66.0);
    assert_eq!(pv["pages"], 1);
}
