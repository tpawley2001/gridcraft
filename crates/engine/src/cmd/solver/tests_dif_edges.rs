//! Additional AllDifferent coverage beyond the task's fixed acceptance tests.
use gridcraft_core::{CellRef, Value};
use serde_json::json;

use crate::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "B1", "input": "=3*A1+2*A2+A3"})).unwrap();
    s
}

#[test]
fn aliases_and_grg_share_integer_distinctness_search() {
    for (method, op) in [("simplex", "AlLdIfF"), ("grg", "DIF")] {
        let mut s = session();
        let r = s
            .execute(
                "data.solver",
                json!({"objective": "B1", "goal": "min", "variables": "A1:A3", "constraints": [{"lhs": "A1:A3", "op": op}], "method": method}),
            )
            .unwrap();
        assert_eq!(r["objective"], 10.0, "{r}");
    }
}

#[test]
fn oversized_distinctness_search_is_refused() {
    let mut s = session();
    let err =
        s.execute("data.solver", json!({"objective": "B1", "variables": "A1:A65", "constraints": ["A1:A65 dif"], "method": "simplex"})).unwrap_err();
    assert!(err.to_string().contains("64 cells"), "{err}");
}

#[test]
fn model_predicate_rejects_duplicates_fractions_and_out_of_range_values() {
    let mut s = session();
    s.execute("data.solverSaveModel", json!({"to": "H1", "model": {"objective": "B1", "variables": "A1:A3", "constraints": ["A1:A3 dif"]}})).unwrap();
    for (values, expected) in
        [([1.0, 2.0, 3.0], true), ([1.0, 1.0, 3.0], false), ([1.0, 1.5, 3.0], false), ([0.0, 2.0, 3.0], false), ([1.0, 2.0, 4.0], false)]
    {
        for (cell, v) in ["A1", "A2", "A3"].iter().zip(values) {
            s.execute("cell.set", json!({"cell": cell, "input": v.to_string()})).unwrap();
        }
        assert_eq!(s.doc().unwrap().wb.active().unwrap().value(CellRef::parse("H3").unwrap()), Value::Bool(expected), "{values:?}");
    }
}

#[test]
fn answer_report_names_the_distinctness_range() {
    let mut s = session();
    s.execute(
        "data.solver",
        json!({"objective": "B1", "goal": "min", "variables": "A1:A3", "constraints": ["A1:A3 dif"], "method": "simplex", "reports": ["answer"]}),
    )
    .unwrap();
    let d = s.doc().unwrap();
    let report = d.wb.sheet(0).unwrap();
    assert!((0..60).any(|r| report.value(CellRef::new(r, 4)) == Value::text("$A$1:$A$3=AllDifferent")));
}

#[test]
fn overlapping_groups_remain_distinct_in_both_searches() {
    for method in ["simplex", "evolutionary"] {
        let mut s = session();
        let r = s.execute("data.solver", json!({"objective": "B1", "goal": "min", "variables": "A1:A3", "constraints": ["A1:A2 dif", "A2:A3 dif"], "method": method, "randomSeed": 7})).unwrap();
        assert_eq!(r["objective"], 8.0, "{r}");
        assert_eq!(r["values"]["A1"], 1.0, "{r}");
        assert_eq!(r["values"]["A2"], 2.0, "{r}");
        assert_eq!(r["values"]["A3"], 1.0, "{r}");
    }
}
