//! TEXT with elapsed-time tokens ([h], [m], [s]) mixed with ordinary time tokens. Expected values are Microsoft
//! Excel's own recalculated answers for the same formulas.

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
    ("B29", r#"TEXT(2.75,"[h] s [m]")"#, X::E(r#"#VALUE!"#)),
    ("B30", r#"TEXT(2.75,"[h]:[mm]")"#, X::E(r#"#VALUE!"#)),
    ("B31", r#"TEXT(2.75,"[h]:[m]:ss")"#, X::E(r#"#VALUE!"#)),
    ("B32", r#"TEXT(2.75,"[h] [m]")"#, X::E(r#"#VALUE!"#)),
    ("B33", r#"TEXT(2.75,"[m] [s]")"#, X::E(r#"#VALUE!"#)),
    ("B34", r#"TEXT(2.75,"[mm]:ss")"#, X::T(r#"3960:00"#)),
    ("B35", r#"TEXT(2.75,"[h]:mm:[ss]")"#, X::E(r#"#VALUE!"#)),
    ("B36", r#"TEXT(2.75,"[ss] [h]")"#, X::E(r#"#VALUE!"#)),
    ("B37", r#"TEXT(2.75,"[h]")"#, X::T(r#"66"#)),
    ("B38", r#"TEXT(2.75,"[hh]:mm")"#, X::T(r#"66:00"#)),
    ("B39", r#"TEXT(2.75,"[s].00")"#, X::T(r#"237600.00"#)),
    ("B40", r#"TEXT(2.75,"h [m]")"#, X::E(r#"#VALUE!"#)),
    ("B41", r#"TEXT(2.75,"[m]:ss [h]")"#, X::E(r#"#VALUE!"#)),
    ("B42", r#"TEXT(2.75,"[h]:mm [s]")"#, X::E(r#"#VALUE!"#)),
];

#[test]
fn text_elapsed_time_tokens_match_excel() {
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
