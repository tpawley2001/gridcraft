//! MID, REPLACE, FIND and SEARCH count positions in UTF-16 code units, as Excel does: a character
//! outside the Basic Multilingual Plane (an emoji) takes two. LEFT and RIGHT keep such a character whole.
//! Answers are Microsoft 365 Excel's for these formulas. Where Excel returned half of a surrogate pair
//! (an unpaired surrogate, which UTF-8 text cannot hold) the expected text has U+FFFD in its place: one
//! UTF-16 unit, so lengths and later positions agree with Excel.

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
    ("B54", r#"MID("a😀b",2,1)"#, X::T(r#"�"#)),
    ("B55", r#"MID("a😀b",2,2)"#, X::T(r#"😀"#)),
    ("B56", r#"MID("a😀b",3,1)"#, X::T(r#"�"#)),
    ("B57", r#"MID("a😀b",4,1)"#, X::T(r#"b"#)),
    ("B58", r#"LEN(MID("a😀b",2,1))"#, X::N(1.0)),
    ("B61", r#"LEFT("😀b",1)"#, X::T(r#"😀"#)),
    ("B62", r#"LEN(LEFT("😀b",1))"#, X::N(2.0)),
    ("B63", r#"RIGHT("a😀",1)"#, X::T(r#"😀"#)),
    ("B64", r#"RIGHT("a😀",2)"#, X::T(r#"a😀"#)),
    ("B65", r#"FIND("b","a😀b")"#, X::N(4.0)),
    ("B66", r#"SEARCH("b","a😀b")"#, X::N(4.0)),
    ("B67", r#"FIND("😀","ab😀")"#, X::N(3.0)),
    ("B68", r#"REPLACE("a😀b",2,2,"X")"#, X::T(r#"aXb"#)),
    ("B69", r#"REPLACE("a😀b",2,1,"X")"#, X::T(r#"aX�b"#)),
    ("B70", r#"SUBSTITUTE("😀😀","😀","x",2)"#, X::T(r#"😀x"#)),
    ("B71", r#"LENB("😀")"#, X::N(2.0)),
    ("B73", r#"CODE("😀")"#, X::N(63.0)),
    ("B75", r#"LEN(REPT("😀",3))"#, X::N(6.0)),
    ("B76", r#"UPPER("a😀b")"#, X::T(r#"A😀B"#)),
    ("B77", r#"MID("😀😀",3,2)"#, X::T(r#"😀"#)),
];

#[test]
fn text_positions_count_utf16_units_like_excel() {
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
