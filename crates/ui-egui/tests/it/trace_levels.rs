//! Tracer arrows: a repeated trace adds the next level, traces add up, and Remove Precedent /
//! Dependent Arrows take one level away at a time.

use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A1 and B1 feed C1; C1 feeds D1; D1 feeds E1. F1 = A1 on its own.
fn chain() -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1, 2]]})).unwrap();
    s.execute("cell.set", json!({"cell": "C1", "input": "=A1+B1"})).unwrap();
    s.execute("cell.set", json!({"cell": "D1", "input": "=C1*2"})).unwrap();
    s.execute("cell.set", json!({"cell": "E1", "input": "=D1+1"})).unwrap();
    s.execute("cell.set", json!({"cell": "F1", "input": "=A1"})).unwrap();
    crate::support::harness(s, 4)
}

fn select(h: &mut Harness, cell: &str) {
    h.state_mut().session.execute("selection.set", json!({"range": cell})).unwrap();
}

fn arrows(h: &Harness) -> Vec<String> {
    h.state().trace_arrows()
}

fn v(list: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = list.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

#[test]
fn a_first_trace_draws_one_level() {
    let mut h = chain();
    select(&mut h, "D1");
    h.state_mut().trace_precedents();
    assert_eq!(arrows(&h), v(&["C1->D1 precedent 1"]));
}

#[test]
fn tracing_precedents_again_adds_the_next_level() {
    let mut h = chain();
    select(&mut h, "E1");
    h.state_mut().trace_precedents();
    h.state_mut().trace_precedents();
    assert_eq!(arrows(&h), v(&["D1->E1 precedent 1", "C1->D1 precedent 2"]));
    h.state_mut().trace_precedents();
    assert_eq!(arrows(&h), v(&["D1->E1 precedent 1", "C1->D1 precedent 2", "A1->C1 precedent 3", "B1->C1 precedent 3"]));
    // A1 and B1 hold constants: nothing further to trace, nothing changes.
    h.state_mut().trace_precedents();
    assert_eq!(arrows(&h).len(), 4);
}

#[test]
fn tracing_dependents_again_adds_the_next_level() {
    let mut h = chain();
    select(&mut h, "C1");
    h.state_mut().trace_dependents();
    assert_eq!(arrows(&h), v(&["C1->D1 dependent 1"]));
    h.state_mut().trace_dependents();
    assert_eq!(arrows(&h), v(&["C1->D1 dependent 1", "D1->E1 dependent 2"]));
}

#[test]
fn traces_of_other_cells_and_kinds_add_up() {
    let mut h = chain();
    select(&mut h, "D1");
    h.state_mut().trace_precedents();
    h.state_mut().trace_dependents();
    select(&mut h, "F1");
    h.state_mut().trace_precedents();
    assert_eq!(arrows(&h), v(&["C1->D1 precedent 1", "D1->E1 dependent 1", "A1->F1 precedent 1"]));
}

#[test]
fn the_arrows_stay_when_the_selection_moves() {
    let mut h = chain();
    select(&mut h, "D1");
    h.state_mut().trace_precedents();
    select(&mut h, "H9");
    h.run_steps(2);
    assert_eq!(arrows(&h), v(&["C1->D1 precedent 1"]));
}

#[test]
fn remove_precedent_arrows_takes_the_deepest_level_first() {
    let mut h = chain();
    select(&mut h, "E1");
    h.state_mut().trace_precedents();
    h.state_mut().trace_precedents();
    h.state_mut().trace_dependents();
    h.state_mut().remove_precedent_arrows();
    assert_eq!(arrows(&h), v(&["D1->E1 precedent 1"]), "E1 has no dependents, so only precedent arrows were shown");
    h.state_mut().remove_precedent_arrows();
    assert!(arrows(&h).is_empty());
    assert!(h.state().grid.trace.is_none(), "no arrows left: no trace");
    h.state_mut().remove_precedent_arrows();
    assert!(arrows(&h).is_empty(), "nothing to remove is fine");
}

#[test]
fn remove_dependent_arrows_leaves_precedent_arrows() {
    let mut h = chain();
    select(&mut h, "C1");
    h.state_mut().trace_precedents();
    h.state_mut().trace_dependents();
    h.state_mut().trace_dependents();
    h.state_mut().remove_dependent_arrows();
    assert_eq!(arrows(&h), v(&["A1->C1 precedent 1", "B1->C1 precedent 1", "C1->D1 dependent 1"]));
    h.state_mut().remove_dependent_arrows();
    assert_eq!(arrows(&h), v(&["A1->C1 precedent 1", "B1->C1 precedent 1"]));
}

#[test]
fn remove_arrows_removes_them_all() {
    let mut h = chain();
    select(&mut h, "C1");
    h.state_mut().trace_precedents();
    h.state_mut().trace_dependents();
    h.state_mut().remove_arrows();
    assert!(arrows(&h).is_empty());
}

#[test]
fn a_range_precedent_is_drawn_boxed_and_an_edit_clears_the_arrows() {
    let mut h = chain();
    h.state_mut().session.execute("cell.set", json!({"cell": "G1", "input": "=SUM(A1:B1)"})).unwrap();
    select(&mut h, "G1");
    h.state_mut().trace_precedents();
    assert_eq!(arrows(&h), v(&["A1:B1->G1 precedent 1"]));
    h.state_mut().session.execute("cell.set", json!({"cell": "A1", "input": "5"})).unwrap();
    h.run_steps(2);
    assert!(arrows(&h).is_empty(), "a change to the workbook removes the arrows, as before");
}

#[test]
fn the_engine_has_the_two_remove_commands() {
    let mut h = chain();
    for id in ["formulas.removeArrows", "formulas.removePrecedentArrows", "formulas.removeDependentArrows"] {
        h.state_mut().session.execute(id, json!({})).unwrap_or_else(|e| panic!("{id}: {e:?}"));
    }
}
