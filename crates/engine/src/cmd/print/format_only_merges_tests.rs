//! Merged ranges over whole columns or rows, and sheets where nothing puts anything on paper.
//! Letter portrait, default margins, column widths and row heights.
use serde_json::json;

use super::*;

fn page_count(s: &mut Session) -> usize {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    pv["pages"].as_u64().unwrap() as usize
}

fn book() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

/// Merges over whole columns do not reach down to the grid's last row: A1 = 1 with A:A and B:D
/// merged prints one page.
#[test]
fn whole_column_merges_do_not_extend_the_range_down() {
    let mut s = book();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    s.execute("home.mergeCells", json!({"range": "A:A"})).unwrap();
    s.execute("home.mergeCells", json!({"range": "B:D"})).unwrap();
    s.execute("home.mergeCells", json!({"range": "E2:E4"})).unwrap();
    assert_eq!(page_count(&mut s), 1);
}

/// Merges over whole rows do not reach right to the grid's last column: B1 = 1 with 1:1 and 2:4
/// merged prints one page.
#[test]
fn whole_row_merges_do_not_extend_the_range_right() {
    let mut s = book();
    s.execute("range.setValues", json!({"range": "B1", "values": [[1]]})).unwrap();
    s.execute("home.mergeCells", json!({"range": "1:1"})).unwrap();
    s.execute("home.mergeCells", json!({"range": "2:4"})).unwrap();
    s.execute("home.mergeCells", json!({"range": "B5:D5"})).unwrap();
    assert_eq!(page_count(&mut s), 1);
}

/// An ordinary empty merge still counts fully (as in `format_only_cells_tests`).
#[test]
fn an_ordinary_empty_merge_still_counts() {
    let mut s = book();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    s.execute("home.mergeCells", json!({"range": "A120:B121"})).unwrap();
    assert_eq!(page_count(&mut s), 3);
}

/// A sheet whose stored cells carry only a font and a number format, with no value anywhere,
/// prints its whole block of stored cells, as before the formatting-only rule.
#[test]
fn a_sheet_where_nothing_counts_prints_its_stored_cells() {
    let mut s = book();
    s.execute("home.bold", json!({"range": "A1:Z200", "on": true})).unwrap();
    s.execute("home.numberFormat", json!({"range": "A1:Z200", "code": "0.00"})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    let bands: std::collections::BTreeSet<&str> = list.iter().map(|p| p["columns"].as_str().unwrap()).collect();
    assert!(bands.len() > 1, "{list:?}");
    assert!(list.len() > bands.len(), "{list:?}");
    let last = list.last().unwrap();
    assert!(last["columns"].as_str().unwrap().ends_with(":Z") && last["rows"].as_str().unwrap().ends_with(":200"), "{last:?}");
    // One value anywhere and the rule applies: only that cell's page prints.
    s.execute("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    assert_eq!(page_count(&mut s), 1);
}
