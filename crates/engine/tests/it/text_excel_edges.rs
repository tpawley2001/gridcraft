//! Text-function edge cases where Excel differs from a naive implementation. Expected values are Microsoft Excel's
//! own recalculated answers for the same formulas.

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

fn eval(formula: &str) -> Value {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": formula})).unwrap();
    s.doc().unwrap().wb.sheet(0).unwrap().value(CellRef::parse("A1").unwrap())
}

fn text(v: &str) -> Value {
    Value::Text(v.into())
}

#[test]
fn len_counts_utf16_units() {
    assert_eq!(eval("=LEN(\"\u{1F600}\")"), Value::Number(2.0));
    assert_eq!(eval("=LEN(\"abc\")"), Value::Number(3.0));
}

#[test]
fn substitute_instance_counts_overlapping_matches() {
    assert_eq!(eval(r#"=SUBSTITUTE("aaaa","aa","b",2)"#), text("aba"));
    assert_eq!(eval(r#"=SUBSTITUTE("abc","b","x",0)"#), Value::Error(CellError::Value));
    assert_eq!(eval(r#"=SUBSTITUTE("abc","","x")"#), text("abc"));
}

#[test]
fn lower_uses_final_sigma() {
    assert_eq!(eval("=LOWER(\"\u{3A3}\u{391}\u{3A3}\")"), text("\u{3C3}\u{3B1}\u{3C2}"));
    assert_eq!(eval("=UPPER(\"\u{DF}\")"), text("\u{DF}"));
}

#[test]
fn big_whole_number_to_text_keeps_all_digits() {
    assert_eq!(eval(r#"="x"&123456789012345000"#), text("x123456789012345000"));
    assert_eq!(eval(r#"="x"&1E-20"#), text("x1E-20"));
    assert_eq!(eval(r#"="x"&(2/3*1000000)"#), text("x666666.666666667"));
    assert_eq!(eval(r#"="x"&DATE(2024,1,1)"#), text("x45292"));
    assert_eq!(eval("=LEN(1/3)"), Value::Number(17.0));
}
