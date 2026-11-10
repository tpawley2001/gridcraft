//! Fill Down / Right / Up / Left on a multi-area selection fill every area on its own, as Excel does.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

fn sheet(cells: &[(&str, &str)]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in cells {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    s
}

fn fill(s: &mut Session, command: &str, range: &str) {
    s.execute("selection.set", json!({"range": range})).unwrap();
    s.execute(command, json!({})).unwrap();
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(cell).unwrap())
}

fn formula(s: &Session, cell: &str) -> Option<String> {
    let sh = s.doc().unwrap().wb.active().unwrap();
    sh.cell(CellRef::parse(cell).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

#[test]
fn fill_down_fills_every_area() {
    let mut s = sheet(&[("A1", "1"), ("C1", "5")]);
    fill(&mut s, "edit.fillDown", "A1:A3,C1:C3");
    for c in ["A2", "A3"] {
        assert_eq!(value(&s, c), num(1.0), "{c}");
    }
    for c in ["C2", "C3"] {
        assert_eq!(value(&s, c), num(5.0), "{c}");
    }
    for c in ["B2", "B3"] {
        assert_eq!(value(&s, c), Value::Empty, "{c} is between the areas");
    }
}

#[test]
fn each_area_fills_from_its_own_top_row() {
    let mut s = sheet(&[("A1", "1"), ("C3", "7")]);
    fill(&mut s, "edit.fillDown", "A1:A3,C3:C5");
    assert_eq!(value(&s, "A2"), num(1.0));
    assert_eq!(value(&s, "A3"), num(1.0));
    assert_eq!(value(&s, "C4"), num(7.0));
    assert_eq!(value(&s, "C5"), num(7.0));
    assert_eq!(value(&s, "C2"), Value::Empty);
}

#[test]
fn one_cell_areas_copy_from_the_cell_above_each() {
    let mut s = sheet(&[("A1", "1"), ("A2", "=A1+1"), ("C5", "10")]);
    fill(&mut s, "edit.fillDown", "A3,C6");
    assert_eq!(formula(&s, "A3").as_deref().map(|f| f.trim_start_matches('=')), Some("A2+1"));
    assert_eq!(value(&s, "A3"), num(3.0));
    assert_eq!(value(&s, "C6"), num(10.0));
}

#[test]
fn fill_right_fills_every_area() {
    let mut s = sheet(&[("A1", "2"), ("C1", "9"), ("A3", "4")]);
    fill(&mut s, "edit.fillRight", "A1:C1,A3:C3");
    assert_eq!(value(&s, "B1"), num(2.0));
    assert_eq!(value(&s, "C1"), num(2.0), "C1 is overwritten from A1");
    assert_eq!(value(&s, "B3"), num(4.0));
    assert_eq!(value(&s, "C3"), num(4.0));
}

#[test]
fn fill_up_fills_every_area() {
    let mut s = sheet(&[("A3", "1"), ("C3", "5")]);
    fill(&mut s, "edit.fillUp", "A1:A3,C1:C3");
    for (c, v) in [("A1", 1.0), ("A2", 1.0), ("C1", 5.0), ("C2", 5.0)] {
        assert_eq!(value(&s, c), num(v), "{c}");
    }
}

#[test]
fn one_undo_restores_every_area() {
    let mut s = sheet(&[("A1", "1"), ("C1", "5")]);
    fill(&mut s, "edit.fillDown", "A1:A3,C1:C3");
    assert_eq!(value(&s, "C3"), num(5.0), "the second area must fill first");
    s.execute("edit.undo", json!({})).unwrap();
    for c in ["A2", "A3", "C2", "C3"] {
        assert_eq!(value(&s, c), Value::Empty, "{c} after one undo");
    }
    assert_eq!(value(&s, "A1"), num(1.0));
    assert_eq!(value(&s, "C1"), num(5.0));
}

#[test]
fn a_single_area_still_fills_as_before() {
    let mut s = sheet(&[("A1", "x")]);
    fill(&mut s, "edit.fillDown", "A1:A3");
    assert_eq!(value(&s, "A2"), Value::Text("x".into()));
    assert_eq!(value(&s, "A3"), Value::Text("x".into()));
}
