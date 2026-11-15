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

/// Excel probe 1114n prints neither font-only nor number-format-only sheets. The old
/// "all stored cells" expectation was disproved by that measured PDF.
#[test]
fn a_sheet_where_nothing_counts_has_no_automatic_print_range() {
    let mut s = book();
    s.execute("home.bold", json!({"range": "A1:Z200", "on": true})).unwrap();
    s.execute("home.numberFormat", json!({"range": "A1:Z200", "code": "0.00"})).unwrap();
    let d = s.doc().unwrap();
    let sh = d.wb.sheet(0).unwrap();
    assert!(print_ranges(&d.wb, sh).is_empty());
    // One value anywhere makes that cell's page print; no formatting is lost.
    s.execute("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    assert_eq!(page_count(&mut s), 1);
}
