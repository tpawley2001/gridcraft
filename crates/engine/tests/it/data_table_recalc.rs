//! What-If data tables (`{=TABLE()}`) are live: `data.dataTable` records the table on the sheet and every
//! recalculation recomputes its result cells, as in Excel.

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_engine::Session;
use gridcraft_model::DataTable;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    // Rate B1, amount B2.
    s.execute("range.setValues", json!({"range": "B1", "values": [[0.05], [1000]]})).unwrap();
    s
}

fn n(s: &Session, a1: &str) -> f64 {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(a1).unwrap()).as_f64().unwrap_or(f64::NAN)
}

fn near(s: &Session, a1: &str, want: f64) {
    let got = n(s, a1);
    assert!((got - want).abs() < 1e-9, "{a1}: got {got}, want {want}");
}

fn tables(s: &Session) -> Vec<DataTable> {
    s.doc().unwrap().wb.active().unwrap().data_tables.clone()
}

fn set(s: &mut Session, a1: &str, input: &str) {
    s.execute("cell.set", json!({"cell": a1, "input": input})).unwrap();
}

/// Column-oriented one-variable table D2:E5: amounts down D3:D5, formula in E2, column input B2.
fn column_table(s: &mut Session) {
    s.execute("range.setValues", json!({"range": "D3", "values": [[100], [200], [300]]})).unwrap();
    set(s, "E2", "=B2*(1+B1)");
    s.execute("data.dataTable", json!({"range": "D2:E5", "columnInput": "B2"})).unwrap();
}

#[test]
fn column_table_is_recorded_and_recalculates() {
    let mut s = session();
    column_table(&mut s);
    let t = tables(&s);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].range.a1(), "E3:E5");
    assert!(!t[0].two_d && !t[0].row);
    assert_eq!(t[0].r1.map(|c| c.a1()).as_deref(), Some("B2"));
    assert_eq!(t[0].r2, None);
    near(&s, "E3", 105.0);
    // The rate changes: every result follows.
    set(&mut s, "B1", "0.1");
    near(&s, "E3", 110.0);
    near(&s, "E4", 220.0);
    near(&s, "E5", 330.0);
    // An input value changes.
    set(&mut s, "D4", "250");
    near(&s, "E4", 275.0);
    // The table formula changes.
    set(&mut s, "E2", "=B2*2");
    near(&s, "E3", 200.0);
    near(&s, "E5", 600.0);
    // The input cell keeps its own value.
    near(&s, "B2", 1000.0);
}

#[test]
fn row_table_is_recorded_and_recalculates() {
    let mut s = session();
    s.execute("range.setValues", json!({"range": "H2", "values": [[0.1, 0.2, 0.3]]})).unwrap();
    set(&mut s, "G3", "=B2*(1+B1)");
    s.execute("data.dataTable", json!({"range": "G2:J3", "rowInput": "B1"})).unwrap();
    let t = tables(&s);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].range.a1(), "H3:J3");
    assert!(!t[0].two_d && t[0].row);
    assert_eq!(t[0].r1.map(|c| c.a1()).as_deref(), Some("B1"));
    near(&s, "J3", 1300.0);
    set(&mut s, "B2", "2000");
    near(&s, "H3", 2200.0);
    near(&s, "J3", 2600.0);
    near(&s, "B1", 0.05);
}

#[test]
fn two_variable_table_is_recorded_and_recalculates() {
    let mut s = session();
    set(&mut s, "L1", "=B2*(1+B1)");
    s.execute("range.setValues", json!({"range": "M1", "values": [[0.5, 1]]})).unwrap();
    s.execute("range.setValues", json!({"range": "L2", "values": [[10], [20]]})).unwrap();
    s.execute("data.dataTable", json!({"range": "L1:N3", "rowInput": "B1", "columnInput": "B2"})).unwrap();
    let t = tables(&s);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].range.a1(), "M2:N3");
    assert!(t[0].two_d);
    assert_eq!(t[0].r1.map(|c| c.a1()).as_deref(), Some("B1"));
    assert_eq!(t[0].r2.map(|c| c.a1()).as_deref(), Some("B2"));
    near(&s, "M2", 15.0);
    set(&mut s, "M1", "2");
    near(&s, "M2", 30.0);
    near(&s, "M3", 60.0);
    set(&mut s, "L1", "=B2+B1");
    near(&s, "N3", 21.0);
    near(&s, "N2", 11.0);
}

#[test]
fn editing_part_of_a_table_is_refused() {
    let mut s = session();
    column_table(&mut s);
    assert!(s.execute("cell.set", json!({"cell": "E4", "input": "5"})).is_err());
    assert!(s.execute("range.setValues", json!({"range": "E5", "values": [[1], [2]]})).is_err());
    near(&s, "E4", 210.0);
    near(&s, "E5", 315.0);
    assert_eq!(tables(&s).len(), 1);
}

#[test]
fn undo_removes_the_table() {
    let mut s = session();
    column_table(&mut s);
    assert_eq!(tables(&s).len(), 1);
    s.execute("edit.undo", json!({})).unwrap();
    assert!(tables(&s).is_empty());
}

#[test]
fn a_table_loaded_with_stale_values_recalculates() {
    // As read from a file: the TABLE() entry plus cached values that no longer match.
    let mut s = session();
    s.execute("range.setValues", json!({"range": "D3", "values": [[100], [200], [300]]})).unwrap();
    set(&mut s, "E2", "=B2*(1+B1)");
    s.execute("range.setValues", json!({"range": "E3", "values": [[1], [2], [3]]})).unwrap();
    let dt = DataTable { range: RangeRef::parse("E3:E5").unwrap(), r1: Some(CellRef::parse("B2").unwrap()), ..DataTable::default() };
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().wb).sheet_mut(0).unwrap().data_tables.push(dt);
    s.execute("formulas.calculateNow", json!({})).unwrap();
    near(&s, "E3", 105.0);
    near(&s, "E4", 210.0);
    near(&s, "E5", 315.0);
    assert!(matches!(s.doc().unwrap().wb.active().unwrap().value(CellRef::parse("B2").unwrap()), Value::Number(_)));
}
