//! Data-array statistics (PERCENTILE, QUARTILE, LARGE, SMALL) read a one-cell reference like any other
//! reference: text, TRUE/FALSE and empty cells are ignored, so nothing left gives #NUM!, as in Excel.

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

fn sheet() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in [("A1", "x"), ("A2", "5"), ("A3", "TRUE")] {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    s
}

fn eval(s: &mut Session, formula: &str) -> Value {
    s.execute("cell.set", json!({"cell": "C1", "input": formula})).unwrap();
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse("C1").unwrap())
}

#[test]
fn text_in_one_cell_reference_is_ignored() {
    let mut s = sheet();
    // Measured in Excel: PERCENTILE over a one-cell range holding text is #NUM!.
    for f in ["=PERCENTILE(A1:A1,0.3)", "=PERCENTILE(A1,0.3)", "=PERCENTILE.INC(A1,0.3)", "=QUARTILE(A1,1)", "=LARGE(A1,1)", "=SMALL(A1,1)"] {
        assert_eq!(eval(&mut s, f), Value::Error(CellError::Num), "{f}");
    }
}

#[test]
fn logical_and_empty_in_one_cell_reference_are_ignored() {
    let mut s = sheet();
    for f in ["=PERCENTILE(A3,0.5)", "=PERCENTILE(A4,0.5)", "=LARGE(A3,1)"] {
        assert_eq!(eval(&mut s, f), Value::Error(CellError::Num), "{f}");
    }
}

#[test]
fn numbers_still_count() {
    let mut s = sheet();
    for f in ["=PERCENTILE(A2,0.3)", "=LARGE(A2,1)", "=PERCENTILE(A1:A2,0.3)", "=SMALL(A1:A3,1)"] {
        assert_eq!(eval(&mut s, f), Value::Number(5.0), "{f}");
    }
    assert_eq!(eval(&mut s, "=PERCENTILE({1,2,3},0.5)"), Value::Number(2.0));
}
