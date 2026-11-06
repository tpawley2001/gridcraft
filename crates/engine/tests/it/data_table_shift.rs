//! What-If data tables (`Sheet::data_tables`) follow sheet row and column inserts and deletes.

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_engine::Session;
use gridcraft_model::DataTable;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range":"A1", "values":[["1"], ["2"]]})).unwrap();
    let dt = DataTable {
        range: RangeRef::parse("C4:D8").unwrap(),
        two_d: true,
        row: true,
        r1: Some(CellRef::parse("A1").unwrap()),
        r2: Some(CellRef::parse("A2").unwrap()),
        ..DataTable::default()
    };
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().wb).sheet_mut(0).unwrap().data_tables.push(dt);
    s
}

fn tables(s: &Session) -> Vec<DataTable> {
    s.doc().unwrap().wb.active().unwrap().data_tables.clone()
}

#[test]
fn insert_row_above_moves_table_and_inputs() {
    let mut s = session();
    s.execute("home.insertRows", json!({"rows": "1:1"})).unwrap();
    let t = &tables(&s)[0];
    assert_eq!(t.range.a1(), "C5:D9");
    assert_eq!(t.r1.map(|c| c.a1()).as_deref(), Some("A2"));
    assert_eq!(t.r2.map(|c| c.a1()).as_deref(), Some("A3"));
    assert!(!t.del1 && !t.del2);
}

#[test]
fn insert_column_left_moves_table_and_inputs() {
    let mut s = session();
    s.execute("home.insertColumns", json!({"cols": "A:A"})).unwrap();
    let t = &tables(&s)[0];
    assert_eq!(t.range.a1(), "D4:E8");
    assert_eq!(t.r1.map(|c| c.a1()).as_deref(), Some("B1"));
    assert_eq!(t.r2.map(|c| c.a1()).as_deref(), Some("B2"));
}

#[test]
fn deleting_the_tables_rows_drops_it() {
    let mut s = session();
    s.execute("home.deleteRows", json!({"rows": "4:8"})).unwrap();
    assert!(tables(&s).is_empty());
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(tables(&s).len(), 1);
}

#[test]
fn deleting_an_input_row_marks_it_deleted() {
    let mut s = session();
    s.execute("home.deleteRows", json!({"rows": "1:1"})).unwrap();
    let t = &tables(&s)[0];
    assert_eq!(t.range.a1(), "C3:D7");
    assert!(t.del1);
    assert_eq!(t.r2.map(|c| c.a1()).as_deref(), Some("A1"));
    assert!(!t.del2);
}
