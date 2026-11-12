//! MAKEARRAY takes whole numbers for rows and cols: a fractional size is #VALUE! in Excel (not rounded down).
//! Answers are Microsoft 365 Excel's for these formulas.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

/// Excel's answer for a cell (each table uses only the kinds it needs).
#[allow(dead_code)]
enum X {
    N(f64),
    T(&'static str),
    B(bool),
    E(&'static str),
}

/// Input cells the formulas read.
const DATA: &[(&str, &str)] = &[];

/// (cell, formula, Excel's answer)
const CASES: &[(&str, &str, X)] = &[
    ("B20", r#"TEXTJOIN(",",FALSE,MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))"#, X::T(r#"11,12,13,21,22,23"#)),
    ("B21", r#"SUM(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))"#, X::N(102.0)),
    ("B22", r#"ROWS(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))*10+COLUMNS(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))"#, X::N(23.0)),
    ("B23", r#"INDEX(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)),2,1)"#, X::N(21.0)),
    ("B48", r#"ROWS(MAKEARRAY(2.7,1.2,LAMBDA(r,c,r)))*10+COLUMNS(MAKEARRAY(2.7,1.2,LAMBDA(r,c,r)))"#, X::E(r#"#VALUE!"#)),
    ("B49", r#"ERROR.TYPE(MAKEARRAY(0,2,LAMBDA(r,c,r)))"#, X::N(3.0)),
    ("B50", r#"ERROR.TYPE(MAKEARRAY(-1,2,LAMBDA(r,c,r)))"#, X::N(3.0)),
    ("B51", r#"ERROR.TYPE(MAKEARRAY("a",2,LAMBDA(r,c,r)))"#, X::N(3.0)),
];

#[test]
fn makearray_sizes_like_excel() {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in DATA {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    for (cell, f, _) in CASES {
        s.execute("cell.set", json!({"cell": cell, "input": format!("={f}")})).unwrap();
    }
    let sheet = s.doc().unwrap().wb.sheet(0).unwrap();
    let mut wrong = Vec::new();
    for (cell, f, want) in CASES {
        let got = sheet.value(CellRef::parse(cell).unwrap());
        let ok = match (want, &got) {
            (X::N(w), Value::Number(n)) => (n - w).abs() <= 1e-9 * w.abs().max(1.0),
            (X::T(w), Value::Text(t)) => &**t == *w,
            (X::B(w), Value::Bool(b)) => b == w,
            (X::E(w), Value::Error(e)) => e.to_string() == *w,
            _ => false,
        };
        if !ok {
            let w = match want {
                X::N(n) => n.to_string(),
                X::T(t) => format!("{t:?}"),
                X::B(b) => b.to_string(),
                X::E(e) => (*e).to_string(),
            };
            wrong.push(format!("{cell} ={f}: Excel {w}, got {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ from Excel:\n{}", wrong.len(), CASES.len(), wrong.join("\n"));
}
