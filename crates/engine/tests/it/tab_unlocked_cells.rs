//! On a protected sheet Tab and Shift+Tab move between the unlocked cells in reading order,
//! wrapping from the last to the first, whether or not locked cells may be selected. Arrow keys,
//! unprotected sheets and sheets without unlocked cells keep the ordinary one-cell move.

use gridcraft_engine::Session;
use serde_json::{Value, json};

fn book(unlocked: &[&str], protect: Option<Value>) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for cell in unlocked {
        s.execute("home.lockCell", json!({"range": cell, "on": false})).unwrap();
    }
    if let Some(p) = protect {
        s.execute("review.protectSheet", p).unwrap();
    }
    s
}

fn tab(s: &mut Session, from: &str, dc: i64) -> String {
    s.execute("selection.set", json!({"cell": from})).unwrap();
    s.execute("selection.move", json!({"dr": 0, "dc": dc, "tab": true})).unwrap();
    s.doc().unwrap().selection.active.a1()
}

const UNLOCKED: [&str; 3] = ["B2", "D2", "C4"];

#[test]
fn tab_goes_to_the_next_unlocked_cell_and_wraps() {
    let mut s = book(&UNLOCKED, Some(json!({})));
    for (from, want) in [("B2", "D2"), ("D2", "C4"), ("C4", "B2")] {
        assert_eq!(tab(&mut s, from, 1), want, "Tab from {from}");
    }
}

#[test]
fn shift_tab_goes_to_the_previous_unlocked_cell_and_wraps() {
    let mut s = book(&UNLOCKED, Some(json!({})));
    for (from, want) in [("B2", "C4"), ("C4", "D2")] {
        assert_eq!(tab(&mut s, from, -1), want, "Shift+Tab from {from}");
    }
}

#[test]
fn tab_from_a_locked_cell_goes_to_the_next_unlocked_cell() {
    let mut s = book(&UNLOCKED, Some(json!({})));
    assert_eq!(tab(&mut s, "A1", 1), "B2");
}

#[test]
fn tab_wraps_to_the_next_row_when_locked_cells_cannot_be_selected() {
    let mut s = book(&UNLOCKED, Some(json!({"selectLockedCells": false})));
    assert_eq!(tab(&mut s, "D2", 1), "C4");
}

#[test]
fn guard_unprotected_sheet_moves_one_cell() {
    let mut s = book(&UNLOCKED, None);
    assert_eq!(tab(&mut s, "B2", 1), "C2");
}

#[test]
fn guard_protected_sheet_without_unlocked_cells_moves_one_cell() {
    let mut s = book(&[], Some(json!({})));
    assert_eq!(tab(&mut s, "A1", 1), "B1");
}

#[test]
fn guard_arrow_keys_are_unchanged_on_a_protected_sheet() {
    let mut s = book(&UNLOCKED, Some(json!({})));
    s.execute("selection.set", json!({"cell": "B2"})).unwrap();
    s.execute("selection.move", json!({"dr": 0, "dc": 1})).unwrap();
    assert_eq!(s.doc().unwrap().selection.active.a1(), "C2");
}
