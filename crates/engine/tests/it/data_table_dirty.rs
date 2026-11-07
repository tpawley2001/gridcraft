//! What-If data tables are recomputed only when something they depend on changed, and
//! "Automatic Except for Data Tables" leaves them alone until Calculate Now (F9), as in Excel.

use gridcraft_core::CellRef;
use gridcraft_engine::Session;
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

fn set(s: &mut Session, a1: &str, input: &str) {
    s.execute("cell.set", json!({"cell": a1, "input": input})).unwrap();
}

fn recomputed(s: &Session) -> usize {
    s.doc().unwrap().last_data_tables_recalculated
}

/// D2:E5: amounts down D3:D5, formula in E2 (uses B1 and B2), column input B2.
fn rate_table(s: &mut Session) {
    s.execute("range.setValues", json!({"range": "D3", "values": [[100], [200], [300]]})).unwrap();
    set(s, "E2", "=B2*(1+B1)");
    s.execute("data.dataTable", json!({"range": "D2:E5", "columnInput": "B2"})).unwrap();
}

/// G2:H4: amounts down G3:G4, formula in H2 (uses B2 and K1 only), column input B2.
fn fee_table(s: &mut Session) {
    set(s, "K1", "7");
    s.execute("range.setValues", json!({"range": "G3", "values": [[10], [20]]})).unwrap();
    set(s, "H2", "=B2+K1");
    s.execute("data.dataTable", json!({"range": "G2:H4", "columnInput": "B2"})).unwrap();
}

#[test]
fn an_unrelated_edit_recomputes_no_table() {
    let mut s = session();
    rate_table(&mut s);
    set(&mut s, "Z50", "1");
    assert_eq!(recomputed(&s), 0);
    set(&mut s, "Z51", "=Z50*3");
    assert_eq!(recomputed(&s), 0);
    near(&s, "E3", 105.0);
}

#[test]
fn editing_a_precedent_of_the_formula_recomputes_the_table() {
    let mut s = session();
    rate_table(&mut s);
    set(&mut s, "B1", "0.1");
    assert_eq!(recomputed(&s), 1);
    near(&s, "E3", 110.0);
    near(&s, "E5", 330.0);
}

#[test]
fn only_the_affected_table_is_recomputed() {
    let mut s = session();
    rate_table(&mut s);
    fee_table(&mut s);
    near(&s, "H3", 17.0);
    // K1 feeds only the fee table.
    set(&mut s, "K1", "8");
    assert_eq!(recomputed(&s), 1);
    near(&s, "H3", 18.0);
    near(&s, "H4", 28.0);
    near(&s, "E3", 105.0);
    // B1 feeds only the rate table.
    set(&mut s, "B1", "0.2");
    assert_eq!(recomputed(&s), 1);
    near(&s, "E3", 120.0);
}

#[test]
fn editing_an_input_value_recomputes_the_table() {
    let mut s = session();
    rate_table(&mut s);
    set(&mut s, "D4", "250");
    assert_eq!(recomputed(&s), 1);
    near(&s, "E4", 262.5);
}

#[test]
fn a_transitive_precedent_recomputes_the_table() {
    let mut s = session();
    set(&mut s, "C1", "0.05");
    set(&mut s, "B1", "=C1*2");
    rate_table(&mut s);
    near(&s, "E3", 110.0);
    set(&mut s, "C1", "0.1");
    assert_eq!(recomputed(&s), 1);
    near(&s, "E3", 120.0);
}

#[test]
fn automatic_except_tables_waits_for_calculate_now() {
    let mut s = session();
    rate_table(&mut s);
    s.execute("formulas.calculationOptions", json!({"mode": "automaticExceptTables"})).unwrap();
    set(&mut s, "B1", "0.1");
    assert_eq!(recomputed(&s), 0);
    // The formula cell itself recalculates; the table keeps its old results.
    near(&s, "E2", 1100.0);
    near(&s, "E3", 105.0);
    s.execute("formulas.calculateNow", json!({})).unwrap();
    assert_eq!(recomputed(&s), 1);
    near(&s, "E3", 110.0);
    near(&s, "E5", 330.0);
}
