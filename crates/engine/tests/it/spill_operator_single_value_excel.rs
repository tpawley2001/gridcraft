//! The spill operator `A1#` on a formula cell that has no spilled range - a single-value result, an
//! error, or a blocked spill (`#SPILL!`) - is the 1x1 range of that cell in Excel, not `#REF!`.
//! Answers are Microsoft 365 Excel's for this sheet (Excel stores `#SPILL!` as a rich error of type 8;
//! its ERROR.TYPE is 9).

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

/// Input cells the formulas read. B10 blocks the spill of B9.
const DATA: &[(&str, &str)] = &[("M1", "3"), ("M2", "5"), ("M3", "5"), ("B10", "x")];

/// (cell, formula, Excel's answer)
const CASES: &[(&str, &str, X)] = &[
    ("B9", r#"SEQUENCE(3)"#, X::E("#SPILL!")),
    ("I9", r#"ERROR.TYPE(B9)"#, X::N(9.0)),
    ("H9", r#"ROWS(B9#)*10+COLUMNS(B9#)"#, X::N(11.0)),
    ("G9", r#"TEXTJOIN(",",FALSE,B9#)"#, X::E("#SPILL!")),
    ("B73", r#"SUM(B9#)"#, X::E("#SPILL!")),
    ("I73", r#"ERROR.TYPE(B73)"#, X::N(9.0)),
    ("H73", r#"ROWS(B73#)*10+COLUMNS(B73#)"#, X::N(11.0)),
    ("G73", r#"TEXTJOIN(",",FALSE,B73#)"#, X::E("#SPILL!")),
    ("B101", r#"AND(M1:M3>0)"#, X::B(true)),
    ("G101", r#"TEXTJOIN(",",FALSE,B101#)"#, X::T("TRUE")),
    ("H101", r#"ROWS(B101#)*10+COLUMNS(B101#)"#, X::N(11.0)),
    ("B109", r#"SUM(@M1:M10)"#, X::E("#VALUE!")),
    ("I109", r#"ERROR.TYPE(B109)"#, X::N(3.0)),
    ("G109", r#"TEXTJOIN(",",FALSE,B109#)"#, X::E("#VALUE!")),
    ("H109", r#"ROWS(B109#)*10+COLUMNS(B109#)"#, X::N(11.0)),
];

#[test]
fn spill_operator_on_a_single_value_formula_cell_like_excel() {
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
