//! Data › Outline › Group › Auto Outline: summary formulas next to their detail lines group them,
//! nested by span, in the sheet's outline direction; the old outline is replaced.

use gridcraft_engine::Session;
use serde_json::json;

fn sheet(cells: &[(&str, &str)]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in cells {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    s.execute("selection.set", json!({"range": "A1"})).unwrap();
    s
}

fn levels(s: &Session, rows: bool, n: u32) -> Vec<u8> {
    let sh = s.doc().unwrap().wb.active().unwrap();
    let map = if rows { &sh.rows } else { &sh.cols };
    (0..n).map(|i| map.get(&i).map_or(0, |l| l.outline)).collect()
}

fn subtotals() -> Session {
    sheet(&[
        ("A1", "Item"),
        ("B1", "Qty"),
        ("B2", "1"),
        ("B3", "2"),
        ("B4", "3"),
        ("B5", "=SUBTOTAL(9,B2:B4)"),
        ("B6", "4"),
        ("B7", "5"),
        ("B8", "6"),
        ("B9", "=SUBTOTAL(9,B6:B8)"),
        ("B10", "=SUBTOTAL(9,B2:B9)"),
    ])
}

#[test]
fn nested_summary_rows_below_their_detail() {
    let mut s = subtotals();
    s.execute("data.autoOutline", json!({})).unwrap();
    assert_eq!(levels(&s, true, 11), [0, 2, 2, 2, 1, 2, 2, 2, 1, 0, 0]);
    assert_eq!(levels(&s, false, 6), [0; 6]);
}

#[test]
fn a_summary_column_right_of_its_detail() {
    let mut s = sheet(&[("A2", "x"), ("B2", "1"), ("C2", "2"), ("D2", "3"), ("E2", "=SUM(B2:D2)")]);
    s.execute("data.autoOutline", json!({})).unwrap();
    assert_eq!(levels(&s, false, 6), [0, 1, 1, 1, 0, 0]);
    assert_eq!(levels(&s, true, 4), [0; 4]);
}

#[test]
fn summary_rows_above_the_detail_follow_outline_settings() {
    let mut s = sheet(&[("B1", "=SUM(B2:B4)"), ("B2", "1"), ("B3", "2"), ("B4", "3")]);
    s.execute("data.outlineSettings", json!({"summaryBelow": false})).unwrap();
    s.execute("data.autoOutline", json!({})).unwrap();
    assert_eq!(levels(&s, true, 5), [0, 1, 1, 1, 0]);
}

#[test]
fn no_summary_formulas_cannot_create_an_outline() {
    for cells in [&[("A1", "1"), ("A2", "2")][..], &[("B2", "1"), ("C3", "2"), ("D4", "=SUM(B2:C3)")][..]] {
        let mut s = sheet(cells);
        let e = s.execute("data.autoOutline", json!({})).unwrap_err();
        assert!(e.to_string().contains("Cannot create an outline."), "{e}");
        assert!(levels(&s, true, 6).iter().chain(&levels(&s, false, 6)).all(|&l| l == 0));
    }
}

#[test]
fn the_existing_outline_is_replaced() {
    let mut s = subtotals();
    s.execute("data.group", json!({"rows": "12:14"})).unwrap();
    s.execute("data.group", json!({"rows": "2:4"})).unwrap();
    s.execute("data.autoOutline", json!({})).unwrap();
    assert_eq!(levels(&s, true, 15), [0, 2, 2, 2, 1, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn a_selected_block_outlines_only_that_block() {
    let mut s = sheet(&[("B2", "1"), ("B3", "2"), ("B4", "=SUM(B2:B3)"), ("D2", "1"), ("D3", "2"), ("D4", "3"), ("D5", "=SUM(D2:D4)")]);
    s.execute("data.autoOutline", json!({"range": "A1:C4"})).unwrap();
    assert_eq!(levels(&s, true, 6), [0, 1, 1, 0, 0, 0]);
}

#[test]
fn auto_outline_is_one_undo_step() {
    let mut s = subtotals();
    s.execute("data.autoOutline", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(levels(&s, true, 11), [0; 11]);
}

#[test]
fn auto_outline_is_in_the_data_outline_group() {
    let c = gridcraft_engine::command_specs().iter().find(|c| c.id == "data.autoOutline").expect("data.autoOutline");
    assert_eq!(c.label, "Auto Outline");
    assert_eq!(c.menu, ["Data", "Outline"]);
}
