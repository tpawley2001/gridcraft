//! PERCENTRANK of a value between two data points, and TIMEVALUE with a seconds field of 60 or more. Expected values
//! are Microsoft Excel's own recalculated answers for the same formulas and inputs.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

fn session(cells: &[(&str, &str)]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in cells {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    s
}

fn number(s: &Session, cell: &str) -> f64 {
    match s.doc().unwrap().wb.sheet(0).unwrap().value(CellRef::parse(cell).unwrap()) {
        Value::Number(n) => n,
        other => panic!("{cell}: expected a number, got {other:?}"),
    }
}

const DATA: [(&str, &str); 10] =
    [("E1", "3"), ("E2", "5"), ("E3", "5"), ("E4", "8"), ("E5", "1"), ("E6", "9"), ("E7", "5"), ("E8", "2"), ("E9", "8"), ("E10", "4")];

#[test]
fn percentrank_between_values_interpolates_from_last_lower_occurrence() {
    let mut cells = DATA.to_vec();
    cells.push(("A1", "=PERCENTRANK(E1:E10,6)"));
    cells.push(("A2", "=PERCENTRANK.INC(E1:E10,5)"));
    let s = session(&cells);
    assert!((number(&s, "A1") - 0.703).abs() < 1e-12, "PERCENTRANK(E1:E10,6) = {}", number(&s, "A1"));
    assert!((number(&s, "A2") - 0.444).abs() < 1e-12, "PERCENTRANK.INC(E1:E10,5) = {}", number(&s, "A2"));
}

#[test]
fn timevalue_accepts_seconds_of_sixty_or_more() {
    let s = session(&[("A1", "=TIMEVALUE(\"0:0:90\")"), ("A2", "=TIMEVALUE(\"25:00\")")]);
    assert!((number(&s, "A1") - 90.0 / 86400.0).abs() < 1e-15);
    assert!((number(&s, "A2") - (25.0 / 24.0 - 1.0)).abs() < 1e-15);
}
