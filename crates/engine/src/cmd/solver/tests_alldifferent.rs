//! Solver AllDifferent (`dif`) constraints: the cells take a permutation of 1..N.
use gridcraft_core::{CellRef, Value};
use serde_json::json;

use crate::Session;

fn s() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}
fn n(s: &Session, a: &str) -> f64 {
    let d = s.doc().unwrap();
    d.wb.active().unwrap().value(CellRef::parse(a).unwrap()).as_f64().unwrap_or(f64::NAN)
}
fn set(s: &mut Session, cells: &[(&str, &str)]) {
    for (c, v) in cells {
        s.execute("cell.set", json!({"cell": c, "input": v})).unwrap();
    }
}
fn three(s: &mut Session) {
    set(s, &[("A1", "0"), ("A2", "0"), ("A3", "0"), ("B1", "=3*A1+2*A2+A3")]);
}

#[test]
fn simplex_alldifferent_minimises_and_maximises_over_permutations() {
    let mut s = s();
    three(&mut s);
    let r = s
        .execute("data.solver", json!({"objective": "B1", "goal": "min", "variables": "A1:A3", "constraints": ["A1:A3 dif"], "method": "simplex"}))
        .unwrap();
    assert_eq!(r["status"], "optimal", "{r}");
    assert_eq!((n(&s, "A1"), n(&s, "A2"), n(&s, "A3"), n(&s, "B1")), (1.0, 2.0, 3.0, 10.0));
    let r = s
        .execute(
            "data.solver",
            json!({"objective": "B1", "goal": "max", "variables": "A1:A3", "constraints": [{"lhs": "A1:A3", "op": "dif"}], "method": "simplex"}),
        )
        .unwrap();
    assert_eq!(r["status"], "optimal", "{r}");
    assert_eq!((n(&s, "A1"), n(&s, "A2"), n(&s, "A3"), n(&s, "B1")), (3.0, 2.0, 1.0, 14.0));
}

#[test]
fn simplex_alldifferent_combines_with_ordinary_constraints() {
    let mut s = s();
    three(&mut s);
    let r = s
        .execute(
            "data.solver",
            json!({"objective": "B1", "goal": "min", "variables": "A1:A3", "constraints": ["A1:A3 alldifferent", "A1 >= 2"], "method": "simplex"}),
        )
        .unwrap();
    assert_eq!(r["status"], "optimal", "{r}");
    assert_eq!((n(&s, "A1"), n(&s, "A2"), n(&s, "A3"), n(&s, "B1")), (2.0, 1.0, 3.0, 11.0));
}

#[test]
fn evolutionary_keeps_alldifferent_cells_a_permutation() {
    let mut s = s();
    set(&mut s, &[("A1", "0"), ("A2", "0"), ("A3", "0"), ("A4", "0"), ("B1", "=4*A1+3*A2+2*A3+A4")]);
    let r = s
        .execute(
            "data.solver",
            json!({"objective": "B1", "goal": "min", "variables": "A1:A4", "constraints": ["A1:A4 dif"], "method": "evolutionary", "randomSeed": 7}),
        )
        .unwrap();
    let got = [n(&s, "A1"), n(&s, "A2"), n(&s, "A3"), n(&s, "A4")];
    assert_eq!(got, [1.0, 2.0, 3.0, 4.0], "{r}");
    assert_eq!(n(&s, "B1"), 20.0);
}

#[test]
fn alldifferent_round_trips_through_the_saved_model() {
    let mut s = s();
    three(&mut s);
    let model = json!({"objective": "B1", "goal": "min", "variables": "A1:A3", "constraints": ["A1:A3 dif", "A1 >= 2"], "method": "simplex"});
    s.execute("data.solver", model).unwrap();
    let r = s.execute("data.solver", json!({})).unwrap();
    assert_eq!(r["model"]["constraints"], json!(["A1:A3 dif", "A1 >= 2"]), "{r}");
    // The saved model's relation code for AllDifferent is 6, as in Solver's VBA `SolverAdd`.
    {
        let d = s.doc().unwrap();
        let six = d.wb.names.iter().any(|x| x.name.to_ascii_lowercase().starts_with("solver_rel") && x.formula.trim_start_matches('=') == "6");
        assert!(six, "no solver_relN = 6");
    }
    s.execute("data.solverSaveModel", json!({"to": "H1"})).unwrap();
    // The constraint row of the model range is TRUE at the solution (2, 1, 3).
    assert_eq!(s.doc().unwrap().wb.active().unwrap().value(CellRef::parse("H3").unwrap()), Value::Bool(true));
    s.execute("data.solverReset", json!({})).unwrap();
    let loaded = s.execute("data.solverLoadModel", json!({"from": "H1"})).unwrap();
    assert_eq!(loaded["model"]["constraints"], json!(["A1:A3 dif", "A1 >= 2"]), "{loaded}");
}

#[test]
fn alldifferent_must_be_on_variable_cells() {
    let mut s = s();
    three(&mut s);
    let r = s.execute("data.solver", json!({"objective": "B1", "variables": "A1:A2", "constraints": ["A1:A3 dif"], "method": "simplex"}));
    assert!(r.is_err(), "{r:?}");
}
