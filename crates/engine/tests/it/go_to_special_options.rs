//! Go To Special options Excel offers beyond the plain kinds: Constants / Formulas filtered by
//! value type, Row and Column differences, and Data Validation "Same".

use std::collections::BTreeSet;

use gridcraft_engine::Session;
use serde_json::{Value, json};

fn book(cells: &[(&str, &str)]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (c, input) in cells {
        s.execute("cell.set", json!({"cell": c, "input": input})).unwrap();
    }
    s
}

fn select(s: &mut Session, range: &str, active: &str) {
    s.execute("selection.set", json!({"range": range, "active": active})).unwrap();
}

/// The selected cells after Go To Special, as sorted A1 addresses.
fn special(s: &mut Session, p: Value) -> BTreeSet<String> {
    s.execute("edit.goToSpecial", p.clone()).unwrap_or_else(|e| panic!("{p}: {e}"));
    s.doc().unwrap().selection.ranges.iter().flat_map(|r| r.iter().map(|c| c.a1()).collect::<Vec<_>>()).collect()
}

fn set(cells: &[&str]) -> BTreeSet<String> {
    cells.iter().map(|c| c.to_string()).collect()
}

fn typed() -> Session {
    let mut s = book(&[("A1", "1"), ("A2", "t"), ("A3", "TRUE"), ("A4", "#N/A"), ("A5", "=1+1"), ("A6", "=\"a\""), ("A7", "=1=1"), ("A8", "=1/0")]);
    select(&mut s, "A1:A8", "A1");
    s
}

#[test]
fn constants_without_types_keeps_every_constant() {
    let mut s = typed();
    assert_eq!(special(&mut s, json!({"kind": "constants"})), set(&["A1", "A2", "A3", "A4"]));
}

#[test]
fn constants_filtered_by_type() {
    let cases: [(&[&str], &[&str]); 4] = [
        (&["numbers"], &["A1"]),
        (&["text", "logicals"], &["A2", "A3"]),
        (&["errors"], &["A4"]),
        (&["numbers", "text", "logicals", "errors"], &["A1", "A2", "A3", "A4"]),
    ];
    for (types, want) in cases {
        let mut s = typed();
        assert_eq!(special(&mut s, json!({"kind": "constants", "types": types})), set(want), "{types:?}");
    }
}

#[test]
fn formulas_filtered_by_result_type() {
    let cases: [(&[&str], &[&str]); 4] =
        [(&["numbers"], &["A5"]), (&["text"], &["A6"]), (&["logicals", "errors"], &["A7", "A8"]), (&["numbers", "text"], &["A5", "A6"])];
    for (types, want) in cases {
        let mut s = typed();
        assert_eq!(special(&mut s, json!({"kind": "formulas", "types": types})), set(want), "{types:?}");
    }
}

#[test]
fn empty_types_is_a_usage_error() {
    let mut s = typed();
    assert!(s.execute("edit.goToSpecial", json!({"kind": "constants", "types": []})).is_err());
}

fn grid() -> Session {
    let mut s = book(&[("A1", "1"), ("B1", "1"), ("C1", "2"), ("A2", "x"), ("B2", "y"), ("C2", "x"), ("A3", "5"), ("B3", "5"), ("C3", "5")]);
    select(&mut s, "A1:C3", "A1");
    s
}

#[test]
fn row_differences_compare_with_the_active_column() {
    let mut s = grid();
    assert_eq!(special(&mut s, json!({"kind": "rowDifferences"})), set(&["C1", "B2"]));
}

#[test]
fn column_differences_compare_with_the_active_row() {
    let mut s = grid();
    assert_eq!(special(&mut s, json!({"kind": "columnDifferences"})), set(&["A2", "A3", "B2", "B3", "C2", "C3"]));
}

#[test]
fn row_differences_follow_an_active_cell_inside_the_selection() {
    let mut s = book(&[("A1", "1"), ("B1", "2"), ("C1", "2"), ("A2", "3"), ("B2", "3"), ("C2", "4")]);
    select(&mut s, "A1:C2", "B1");
    assert_eq!(special(&mut s, json!({"kind": "rowDifferences"})), set(&["A1", "C2"]));
}

#[test]
fn column_differences_compare_formulas_not_values() {
    // A3 gives 4 like A1 would for B=2, but its formula is not the relative copy of A1's.
    let mut s = book(&[("B1", "1"), ("B2", "2"), ("B3", "2"), ("B4", "3"), ("A1", "=B1*2"), ("A2", "=B2*2"), ("A3", "=B3+2"), ("A4", "=B4*2")]);
    select(&mut s, "A1:A4", "A1");
    assert_eq!(special(&mut s, json!({"kind": "columnDifferences"})), set(&["A3"]));
}

fn validated() -> Session {
    let mut s = book(&[]);
    s.execute("data.validation", json!({"range": "A1:A2", "type": "whole", "operator": "between", "formula1": "1", "formula2": "9"})).unwrap();
    s.execute("data.validation", json!({"range": "B1:B2", "type": "list", "formula1": "a,b"})).unwrap();
    s
}

#[test]
fn data_validation_all_still_selects_every_rule() {
    let mut s = validated();
    select(&mut s, "A1:D5", "D5");
    assert_eq!(special(&mut s, json!({"kind": "dataValidation"})), set(&["A1", "A2", "B1", "B2"]));
}

#[test]
fn data_validation_same_selects_only_the_active_cells_rule() {
    let mut s = validated();
    select(&mut s, "B2", "B2");
    assert_eq!(special(&mut s, json!({"kind": "dataValidation", "same": true})), set(&["B1", "B2"]));
    select(&mut s, "A1", "A1");
    assert_eq!(special(&mut s, json!({"kind": "dataValidation", "same": true})), set(&["A1", "A2"]));
}

#[test]
fn data_validation_same_on_an_unvalidated_cell_finds_nothing() {
    let mut s = validated();
    select(&mut s, "D5", "D5");
    assert!(s.execute("edit.goToSpecial", json!({"kind": "dataValidation", "same": true})).is_err());
}
