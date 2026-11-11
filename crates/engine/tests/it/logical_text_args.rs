//! AND/OR skip a text argument typed directly into the formula unless it reads as TRUE/FALSE, and PHONETIC of a
//! cell holding a number gives #N/A. Expected values are Microsoft Excel's own recalculated answers for the same
//! formulas and inputs.

use gridcraft_core::{CellError, CellRef, Value};
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

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(CellRef::parse(cell).unwrap())
}

#[test]
fn and_skips_typed_in_text_that_is_not_a_logical() {
    // Excel: AND("1",1) = TRUE ("1" is skipped, 1 is TRUE).
    let s = session(&[("J1", "=AND(\"1\",1)")]);
    assert_eq!(value(&s, "J1"), Value::Bool(true));
}

#[test]
fn and_with_only_text_left_is_still_value_error() {
    // Excel: text from references and the typed-in "1" are all skipped; nothing logical is left -> #VALUE!.
    let s = session(&[("E14", "=\"0.000001\""), ("E17", "=\"2.99999\""), ("I1", "=AND(E14,E17,\"1\")")]);
    assert_eq!(value(&s, "I1"), Value::Error(CellError::Value));
}

#[test]
fn and_still_reads_typed_in_true_text() {
    // Guard (upstream behaviour): "TRUE" text typed in counts as TRUE.
    let s = session(&[("A1", "=AND(\"TRUE\",1)"), ("A2", "=AND(\"abc\")")]);
    assert_eq!(value(&s, "A1"), Value::Bool(true));
    assert_eq!(value(&s, "A2"), Value::Error(CellError::Value));
}

#[test]
fn phonetic_of_a_number_cell_is_na() {
    // Excel: A133 =DATEDIF("2023-02-02","2023-02-05","D") is 3; PHONETIC(A133) = #N/A.
    let s = session(&[("A133", "=DATEDIF(\"2023-02-02\",\"2023-02-05\",\"D\")"), ("A136", "=PHONETIC(A133)")]);
    assert_eq!(value(&s, "A133"), Value::Number(3.0));
    assert_eq!(value(&s, "A136"), Value::Error(CellError::NA));
}

#[test]
fn phonetic_of_a_text_cell_returns_the_text() {
    // Guard: text without phonetic runs comes back unchanged.
    let s = session(&[("A1", "abc"), ("B1", "=PHONETIC(A1)")]);
    assert_eq!(value(&s, "B1"), Value::Text("abc".into()));
}
