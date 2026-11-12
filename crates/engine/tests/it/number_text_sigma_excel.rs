//! Numbers converted to text with the General rule (concatenation, LEN, CONCATENATE) and Greek sigma case
//! mapping. Expected values are Microsoft Excel's own recalculated answers for the same formulas.

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
    ("B1", r#""x"&1000000000000000"#, X::T(r#"x1000000000000000"#)),
    ("B2", r#""x"&1000000000000000+1"#, X::T(r#"x1000000000000000"#)),
    ("B3", r#""x"&1500000000000000"#, X::T(r#"x1500000000000000"#)),
    ("B4", r#""x"&1234567890123450"#, X::T(r#"x1234567890123450"#)),
    ("B5", r#""x"&2^53"#, X::T(r#"x9007199254740990"#)),
    ("B6", r#""x"&10000000000000000"#, X::T(r#"x10000000000000000"#)),
    ("B7", r#""x"&-10000000000000000"#, X::T(r#"x-10000000000000000"#)),
    ("B8", r#""x"&12000000000000000"#, X::T(r#"x12000000000000000"#)),
    ("B9", r#""x"&123456789012345000"#, X::T(r#"x123456789012345000"#)),
    ("B10", r#""x"&100000000000000000"#, X::T(r#"x100000000000000000"#)),
    ("B11", r#""x"&123456789012345000"#, X::T(r#"x123456789012345000"#)),
    ("B12", r#""x"&2^60"#, X::T(r#"x1152921504606850000"#)),
    ("B13", r#""x"&10000000000000000000"#, X::T(r#"x10000000000000000000"#)),
    ("B14", r#""x"&100000000000000000000"#, X::T(r#"x1E+20"#)),
    ("B15", r#""x"&99999999999999900000"#, X::T(r#"x99999999999999900000"#)),
    ("B16", r#""x"&123456789012345000000"#, X::T(r#"x1.23456789012345E+20"#)),
    ("B17", r#""x"&1E+21"#, X::T(r#"x1E+21"#)),
    ("B18", r#""x"&1.5E+21"#, X::T(r#"x1.5E+21"#)),
    ("B19", r#""x"&0.000000000000001"#, X::T(r#"x0.000000000000001"#)),
    ("B20", r#""x"&0.000000000000123"#, X::T(r#"x0.000000000000123"#)),
    ("B21", r#""x"&1000000000000000-0.5"#, X::T(r#"x999999999999999"#)),
    ("B22", r#""x"&999999999999999"#, X::T(r#"x999999999999999"#)),
    ("B23", r#""x"&-999999999999999"#, X::T(r#"x-999999999999999"#)),
    ("B24", r#"LEN(1000000000000000)"#, X::N(16.0)),
    ("B25", r#"TEXT(1000000000000000,"General")"#, X::T(r#"1E+15"#)),
    ("B26", r#"CONCATENATE(1000000000000000)"#, X::T(r#"1000000000000000"#)),
    ("B28", r#"EXACT(1000000000000000,""&1000000000000000)"#, X::B(true)),
    ("B29", r#"LEN(10000000000000000)"#, X::N(17.0)),
    ("B30", r#"TEXT(10000000000000000,"General")"#, X::T(r#"1E+16"#)),
    ("B31", r#"CONCATENATE(10000000000000000)"#, X::T(r#"10000000000000000"#)),
    ("B33", r#"EXACT(10000000000000000,""&10000000000000000)"#, X::B(true)),
    ("B34", r#"LEN(123456789012345000)"#, X::N(18.0)),
    ("B35", r#"TEXT(123456789012345000,"General")"#, X::T(r#"1.23457E+17"#)),
    ("B36", r#"CONCATENATE(123456789012345000)"#, X::T(r#"123456789012345000"#)),
    ("B38", r#"EXACT(123456789012345000,""&123456789012345000)"#, X::B(true)),
    ("B39", r#"LEN(100000000000000000000)"#, X::N(5.0)),
    ("B40", r#"TEXT(100000000000000000000,"General")"#, X::T(r#"1E+20"#)),
    ("B41", r#"CONCATENATE(100000000000000000000)"#, X::T(r#"1E+20"#)),
    ("B43", r#"EXACT(100000000000000000000,""&100000000000000000000)"#, X::B(true)),
    ("B44", r#"LEN(1E+21)"#, X::N(5.0)),
    ("B45", r#"TEXT(1E+21,"General")"#, X::T(r#"1E+21"#)),
    ("B46", r#"CONCATENATE(1E+21)"#, X::T(r#"1E+21"#)),
    ("B48", r#"EXACT(1E+21,""&1E+21)"#, X::B(true)),
    ("B49", r#"LEN(2^53)"#, X::N(16.0)),
    ("B50", r#"TEXT(2^53,"General")"#, X::T(r#"9.0072E+15"#)),
    ("B51", r#"CONCATENATE(2^53)"#, X::T(r#"9007199254740990"#)),
    ("B53", r#"EXACT(2^53,""&2^53)"#, X::B(true)),
    ("B78", r#"LOWER("Σ")"#, X::T(r#"ς"#)),
    ("B79", r#"LOWER("ΑΣ.")"#, X::T(r#"ας."#)),
    ("B80", r#"LOWER("ΑΣ1")"#, X::T(r#"ασ1"#)),
    ("B81", r#"LOWER("ΑΣΑ")"#, X::T(r#"ασα"#)),
    ("B82", r#"LOWER("AΣ")"#, X::T(r#"aς"#)),
    ("B83", r#"PROPER("ΣΑΣ ΑΣ")"#, X::T(r#"Σας Ας"#)),
    ("B84", r#"UPPER("ς")"#, X::T(r#"Σ"#)),
    ("B85", r#"LOWER("İ")"#, X::T(r#"i"#)),
    ("B86", r#"LEN(LOWER("İ"))"#, X::N(1.0)),
    ("B87", r#"SUBSTITUTE("aaa","aa","b",2)"#, X::T(r#"ab"#)),
    ("B88", r#"SUBSTITUTE("ababa","aba","X",2)"#, X::T(r#"abX"#)),
];

#[test]
fn number_to_text_and_greek_case_match_excel() {
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
