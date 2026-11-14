//! Fit To with an automatic page count on one axis still honours that axis' manual page breaks,
//! as Excel prints (measured from Excel's PDFs of workbooks with fitToWidth=1, fitToHeight=0 and
//! manual row breaks). An axis with a page count keeps ignoring its manual breaks.
use serde_json::json;

use super::*;

fn s() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn page_list(s: &mut Session, field: &str) -> Vec<String> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p[field].as_str().unwrap().to_string()).collect()
}

fn three_hundred_rows() -> Session {
    let mut s = s();
    let rows: Vec<Vec<Json>> = (1..=300).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

/// Letter portrait, 15pt rows: 47 rows per page (121 printed dots each, Calibri 11). One column is narrower than the page, so the
/// width fit keeps 100%; the manual break before row 11 still splits, then pages fill as usual.
#[test]
fn automatic_height_keeps_manual_row_breaks() {
    let mut s = three_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"fitWidth": 1, "fitHeight": 0, "rowBreaks": [11]})).unwrap();
    let rows = page_list(&mut s, "rows");
    assert_eq!(rows.len(), 8, "{rows:?}");
    assert_eq!(rows[0], "1:10");
    assert_eq!(rows[1], "11:57");
    assert_eq!(rows[2], "58:104");
    assert_eq!(rows[7], "293:300");
}

/// With a page count tall, the manual break is ignored: the rows are scaled onto two pages.
#[test]
fn a_height_count_still_ignores_manual_row_breaks() {
    let mut s = three_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"fitWidth": 1, "fitHeight": 2, "rowBreaks": [11]})).unwrap();
    let rows = page_list(&mut s, "rows");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_ne!(rows[0], "1:10");
}

/// The column side of the same rule: automatic width keeps the column break, the height count
/// of one page ignores the row break.
#[test]
fn automatic_width_keeps_manual_column_breaks() {
    let mut s = s();
    let rows: Vec<Vec<Json>> = (0..60).map(|r| (0..4).map(|c| json!(r * 10 + c)).collect()).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"fitWidth": 0, "fitHeight": 1, "colBreaks": ["C"], "rowBreaks": [31]})).unwrap();
    assert_eq!(page_list(&mut s, "range"), vec!["A1:B60".to_string(), "C1:D60".to_string()]);
}

/// Guards: Fit To forced from the print command ignores breaks; without Fit To they apply.
#[test]
fn forced_fit_and_no_fit_are_unchanged() {
    let mut s = three_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"rowBreaks": [11]})).unwrap();
    assert_eq!(page_list(&mut s, "rows")[0], "1:10");
    let pv = s.execute("file.printPreview", json!({"fitToPage": true})).unwrap();
    assert_eq!(pv["pages"], 1);
}
