//! MATCH approximate modes (1 and -1) on unsorted and wrongly sorted data, where Excel's binary search gives
//! answers a linear scan does not. Expected values are Microsoft Excel's own recalculated answers for the same formulas.

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
const DATA: &[(&str, &str)] = &[
    ("E1", r#"1"#),
    ("F1", r#"10"#),
    ("G1", r#"1"#),
    ("H1", r#"5"#),
    ("E2", r#"2"#),
    ("F2", r#"9"#),
    ("G2", r#"a"#),
    ("H2", r#"1"#),
    ("E3", r#"3"#),
    ("F3", r#"8"#),
    ("G3", r#"3"#),
    ("H3", r#"9"#),
    ("E4", r#"4"#),
    ("F4", r#"7"#),
    ("G4", r#"b"#),
    ("H4", r#"3"#),
    ("E5", r#"5"#),
    ("F5", r#"6"#),
    ("G5", r#"5"#),
    ("H5", r#"7"#),
    ("E6", r#"6"#),
    ("F6", r#"5"#),
    ("G6", r#"TRUE"#),
    ("H6", r#"2"#),
    ("E7", r#"7"#),
    ("F7", r#"4"#),
    ("G7", r#"7"#),
    ("H7", r#"8"#),
    ("E8", r#"8"#),
    ("F8", r#"3"#),
    ("G8", r#"c"#),
    ("H8", r#"4"#),
    ("E9", r#"9"#),
    ("F9", r#"2"#),
    ("G9", r#"9"#),
    ("H9", r#"10"#),
    ("E10", r#"10"#),
    ("F10", r#"1"#),
    ("G10", r#"d"#),
    ("H10", r#"6"#),
];

/// (cell, formula, Excel's answer)
const CASES: &[(&str, &str, X)] = &[
    ("B43", r#"MATCH(8,E1:E10,-1)"#, X::E(r#"#N/A"#)),
    ("B44", r#"MATCH(0,E1:E10,-1)"#, X::N(10.0)),
    ("B45", r#"MATCH(11,E1:E10,-1)"#, X::E(r#"#N/A"#)),
    ("B46", r#"MATCH(1,E1:E10,-1)"#, X::N(1.0)),
    ("B47", r#"MATCH(10,E1:E10,-1)"#, X::E(r#"#N/A"#)),
    ("B48", r#"MATCH(5,E1:E10,-1)"#, X::E(r#"#N/A"#)),
    ("B49", r#"MATCH(8,F1:F10,1)"#, X::N(10.0)),
    ("B50", r#"MATCH(0,F1:F10,1)"#, X::E(r#"#N/A"#)),
    ("B51", r#"MATCH(11,F1:F10,1)"#, X::N(10.0)),
    ("B52", r#"MATCH(5,F1:F10,1)"#, X::E(r#"#N/A"#)),
    ("B53", r#"MATCH(10,F1:F10,1)"#, X::N(10.0)),
    ("B54", r#"MATCH(1,F1:F10,1)"#, X::E(r#"#N/A"#)),
    ("B55", r#"MATCH(4,G1:G10,1)"#, X::N(3.0)),
    ("B56", r#"MATCH(6,G1:G10,1)"#, X::N(5.0)),
    ("B57", r#"MATCH(99,G1:G10,1)"#, X::N(9.0)),
    ("B58", r#"MATCH("b",G1:G10,1)"#, X::N(4.0)),
    ("B59", r#"MATCH("bb",G1:G10,1)"#, X::N(4.0)),
    ("B60", r#"MATCH("z",G1:G10,1)"#, X::N(10.0)),
    ("B61", r#"MATCH(TRUE,G1:G10,1)"#, X::N(6.0)),
    ("B62", r#"MATCH("b",G1:G10,-1)"#, X::E(r#"#N/A"#)),
    ("B63", r#"MATCH(4,G1:G10,-1)"#, X::E(r#"#N/A"#)),
    ("B64", r#"MATCH(6,H1:H10,1)"#, X::N(2.0)),
    ("B65", r#"MATCH(4,H1:H10,1)"#, X::N(2.0)),
    ("B66", r#"MATCH(11,H1:H10,1)"#, X::N(10.0)),
    ("B67", r#"MATCH(0,H1:H10,1)"#, X::E(r#"#N/A"#)),
    ("B68", r#"MATCH(6,H1:H10,-1)"#, X::E(r#"#N/A"#)),
    ("B69", r#"MATCH(0,H1:H10,-1)"#, X::N(10.0)),
    ("B70", r#"MATCH(5,H1:H10,1)"#, X::N(2.0)),
    ("B71", r#"MATCH(8,H1:H10,1)"#, X::N(8.0)),
    ("B72", r#"MATCH(2,H1:H10,1)"#, X::N(2.0)),
    ("B73", r#"MATCH(3,{1,2,3,4,5},-1)"#, X::E(r#"#N/A"#)),
    ("B74", r#"MATCH(3,{5,4,3,2,1},1)"#, X::N(3.0)),
    ("B75", r#"MATCH(4,{5,1,3,9,2},1)"#, X::N(3.0)),
];

#[test]
fn match_approximate_binary_search_matches_excel() {
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
