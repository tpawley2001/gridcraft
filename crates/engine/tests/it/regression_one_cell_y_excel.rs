//! TREND, GROWTH, LINEST and LOGEST with a one-cell known_y's: Excel reads it as ONE observation whose known_x's cells are
//! each an x variable. The formulas carry the `@` Excel shows for them (they were entered in single cells of a
//! workbook saved by pre-dynamic-array Excel). Expected values are Microsoft Excel's own recalculated answers for the same formulas.

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
    ("E1", r#"12"#),
    ("F1", r#"24"#),
    ("G1", r#"5"#),
    ("E2", r#"213"#),
    ("F2", r#"24"#),
    ("G2", r#"45"#),
    ("E3", r#"4"#),
    ("F3", r#"654"#),
    ("G3", r#"1"#),
    ("E4", r#"23423"#),
    ("F4", r#"234"#),
    ("G4", r#"45"#),
    ("E5", r#"423"#),
    ("F5", r#"567"#),
    ("G5", r#"4"#),
    ("E6", r#"423"#),
    ("F6", r#"57"#),
    ("G6", r#"23"#),
    ("E7", r#"532"#),
    ("F7", r#"324"#),
    ("G7", r#"3"#),
    ("E8", r#"2342"#),
    ("F8", r#"234"#),
    ("G8", r#"4"#),
    ("E9", r#"323"#),
    ("F9", r#"234"#),
    ("G9", r#"5"#),
];

/// (cell, formula, Excel's answer)
const CASES: &[(&str, &str, X)] = &[
    ("B1", r#"@TREND(E1,F1:F9)"#, X::N(12.0)),
    ("B2", r#"@TREND(E1,F1:F9,,FALSE)"#, X::N(12.0)),
    ("B3", r#"@TREND(E1,F1:F9,,TRUE)"#, X::N(12.0)),
    ("B4", r#"@TREND(E1,F1:F9,G1:G9)"#, X::N(12.0)),
    ("B5", r#"@TREND(E1,F1:F9,G1:G9,TRUE)"#, X::N(12.0)),
    ("B6", r#"@TREND(E1,F1:F9,G1:G9,FALSE)"#, X::N(0.01834862385321101)),
    ("B7", r#"@TREND(E1,F1:F9,7)"#, X::E(r#"#REF!"#)),
    ("B8", r#"@TREND(E1,F1:F9,7,FALSE)"#, X::E(r#"#REF!"#)),
    ("B9", r#"@TREND(E1,F1:F9,G3,FALSE)"#, X::E(r#"#REF!"#)),
    ("B10", r#"@TREND(E1,F1:F9,G1:G3,FALSE)"#, X::E(r#"#REF!"#)),
    ("B11", r#"@TREND(E2,F1:F9,G1:G9,FALSE)"#, X::N(0.3256880733944954)),
    ("B12", r#"@TREND(E3,F1:F9,G1:G9,FALSE)"#, X::N(0.0061162079510703364)),
    ("B13", r#"@TREND(E1,F1:F3,G1:G9,FALSE)"#, X::E(r#"#REF!"#)),
    ("B14", r#"@TREND(E1,F2:F9,G1:G9,FALSE)"#, X::E(r#"#REF!"#)),
    ("B15", r#"@TREND(E1,F4:F9,G1:G9,FALSE)"#, X::E(r#"#REF!"#)),
    ("B16", r#"@TREND(E1,F1,G1,FALSE)"#, X::N(2.5)),
    ("B17", r#"@TREND(E1,F1)"#, X::N(12.0)),
    ("B18", r#"@TREND(E1,F1:F2,G1:G2,FALSE)"#, X::N(2.5)),
    ("B19", r#"@TREND(E1:E2,F1:F9)"#, X::E(r#"#REF!"#)),
    ("B20", r#"@TREND(E1:E9,F1:F9,G1:G9,FALSE)"#, X::N(32.013139756562545)),
    ("B21", r#"@TREND(12,F1:F9,G1:G9,FALSE)"#, X::N(0.01834862385321101)),
    ("B22", r#"@TREND(12,{24,24,654},{5,45,1},FALSE)"#, X::N(0.01834862385321101)),
    ("B23", r#"@TREND(12,{24;24;654},{5;45;1},FALSE)"#, X::N(0.01834862385321101)),
    ("B24", r#"@TREND(E1,F1:F9,G1:G9,0)"#, X::N(0.01834862385321101)),
    ("B25", r#"@TREND(E1)"#, X::N(12.0)),
    ("B26", r#"@INDEX(TREND(E1,F1:F9,G1:G9,FALSE),1)"#, X::N(0.01834862385321101)),
    ("B27", r#"@INDEX(TREND(E1,F1:F9),1)"#, X::N(12.0)),
    ("B28", r#"@INDEX(TREND(E1,F1:F9,G1:G9,FALSE),2)"#, X::E(r#"#REF!"#)),
    ("B29", r#"@INDEX(TREND(E1,F1:F9),2)"#, X::E(r#"#REF!"#)),
    ("B30", r#"@INDEX(TREND(E1,F1:F9,G1:G9,FALSE),3)"#, X::E(r#"#REF!"#)),
    ("B31", r#"@INDEX(TREND(E1,F1:F9),3)"#, X::E(r#"#REF!"#)),
    ("B32", r#"@INDEX(TREND(E1,F1:F9,G1:G9,FALSE),4)"#, X::E(r#"#REF!"#)),
    ("B33", r#"@INDEX(TREND(E1,F1:F9),4)"#, X::E(r#"#REF!"#)),
    ("B34", r#"@INDEX(TREND(E1,F1:F9,G1:G9,FALSE),9)"#, X::E(r#"#REF!"#)),
    ("B35", r#"@INDEX(TREND(E1,F1:F9),9)"#, X::E(r#"#REF!"#)),
    ("B36", r#"ROWS(TREND(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0)),
    ("B37", r#"ROWS(TREND(E1,F1:F9))"#, X::N(1.0)),
    ("B38", r#"COLUMNS(TREND(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0)),
    ("B39", r#"COLUMNS(TREND(E1,F1:F9))"#, X::N(1.0)),
    ("B40", r#"SUM(TREND(E1,F1:F9,G1:G9,FALSE))"#, X::N(0.01834862385321101)),
    ("B41", r#"SUM(TREND(E1,F1:F9))"#, X::N(12.0)),
    ("B42", r#"COUNT(TREND(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0)),
    ("B43", r#"COUNT(TREND(E1,F1:F9))"#, X::N(1.0)),
    ("B44", r#"@GROWTH(E1,F1:F9)"#, X::N(12.0)),
    ("B45", r#"@GROWTH(E1,F1:F9,,FALSE)"#, X::N(12.00000000000001)),
    ("B46", r#"@GROWTH(E1,F1:F9,,TRUE)"#, X::N(12.0)),
    ("B47", r#"@GROWTH(E1,F1:F9,G1:G9)"#, X::N(12.0)),
    ("B48", r#"@GROWTH(E1,F1:F9,G1:G9,TRUE)"#, X::N(12.0)),
    ("B49", r#"@GROWTH(E1,F1:F9,G1:G9,FALSE)"#, X::N(1.0038067788986802)),
    ("B50", r#"@GROWTH(E1,F1:F9,7)"#, X::E(r#"#REF!"#)),
    ("B51", r#"@GROWTH(E1,F1:F9,7,FALSE)"#, X::E(r#"#REF!"#)),
    ("B52", r#"@GROWTH(E1,F1:F9,G3,FALSE)"#, X::E(r#"#REF!"#)),
    ("B53", r#"@GROWTH(E1,F1:F9,G1:G3,FALSE)"#, X::E(r#"#REF!"#)),
    ("B54", r#"@GROWTH(E2,F1:F9,G1:G9,FALSE)"#, X::N(1.0082313875457278)),
    ("B55", r#"@GROWTH(E3,F1:F9,G1:G9,FALSE)"#, X::N(1.0021219643350017)),
    ("B56", r#"@GROWTH(E1,F1:F3,G1:G9,FALSE)"#, X::E(r#"#REF!"#)),
    ("B57", r#"@GROWTH(E1,F2:F9,G1:G9,FALSE)"#, X::E(r#"#REF!"#)),
    ("B58", r#"@GROWTH(E1,F4:F9,G1:G9,FALSE)"#, X::E(r#"#REF!"#)),
    ("B59", r#"@GROWTH(E1,F1,G1,FALSE)"#, X::N(1.6781447796010298)),
    ("B60", r#"@GROWTH(E1,F1)"#, X::N(12.0)),
    ("B61", r#"@GROWTH(E1,F1:F2,G1:G2,FALSE)"#, X::N(1.6781447796010298)),
    ("B62", r#"@GROWTH(E1:E2,F1:F9)"#, X::E(r#"#REF!"#)),
    ("B63", r#"@GROWTH(E1:E9,F1:F9,G1:G9,FALSE)"#, X::N(1.062614605723532)),
    ("B64", r#"@GROWTH(12,F1:F9,G1:G9,FALSE)"#, X::N(1.0038067788986802)),
    ("B65", r#"@GROWTH(12,{24,24,654},{5,45,1},FALSE)"#, X::N(1.0038067788986802)),
    ("B66", r#"@GROWTH(12,{24;24;654},{5;45;1},FALSE)"#, X::N(1.0038067788986802)),
    ("B67", r#"@GROWTH(E1,F1:F9,G1:G9,0)"#, X::N(1.0038067788986802)),
    ("B68", r#"@GROWTH(E1)"#, X::N(12.0)),
    ("B69", r#"@INDEX(GROWTH(E1,F1:F9,G1:G9,FALSE),1)"#, X::N(1.0038067788986802)),
    ("B70", r#"@INDEX(GROWTH(E1,F1:F9),1)"#, X::N(12.0)),
    ("B71", r#"@INDEX(GROWTH(E1,F1:F9,G1:G9,FALSE),2)"#, X::E(r#"#REF!"#)),
    ("B72", r#"@INDEX(GROWTH(E1,F1:F9),2)"#, X::E(r#"#REF!"#)),
    ("B73", r#"@INDEX(GROWTH(E1,F1:F9,G1:G9,FALSE),3)"#, X::E(r#"#REF!"#)),
    ("B74", r#"@INDEX(GROWTH(E1,F1:F9),3)"#, X::E(r#"#REF!"#)),
    ("B75", r#"@INDEX(GROWTH(E1,F1:F9,G1:G9,FALSE),4)"#, X::E(r#"#REF!"#)),
    ("B76", r#"@INDEX(GROWTH(E1,F1:F9),4)"#, X::E(r#"#REF!"#)),
    ("B77", r#"@INDEX(GROWTH(E1,F1:F9,G1:G9,FALSE),9)"#, X::E(r#"#REF!"#)),
    ("B78", r#"@INDEX(GROWTH(E1,F1:F9),9)"#, X::E(r#"#REF!"#)),
    ("B79", r#"ROWS(GROWTH(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0)),
    ("B80", r#"ROWS(GROWTH(E1,F1:F9))"#, X::N(1.0)),
    ("B81", r#"COLUMNS(GROWTH(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0)),
    ("B82", r#"COLUMNS(GROWTH(E1,F1:F9))"#, X::N(1.0)),
    ("B83", r#"SUM(GROWTH(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0038067788986802)),
    ("B84", r#"SUM(GROWTH(E1,F1:F9))"#, X::N(12.0)),
    ("B85", r#"COUNT(GROWTH(E1,F1:F9,G1:G9,FALSE))"#, X::N(1.0)),
    ("B86", r#"COUNT(GROWTH(E1,F1:F9))"#, X::N(1.0)),
    ("B87", r#"@INDEX(LINEST(E1,F1:F9),1)"#, X::N(0.0)),
    ("B88", r#"@INDEX(LINEST(E1,F1:F9),2)"#, X::N(0.0)),
    ("B89", r#"@INDEX(LINEST(E1,F1:F9,FALSE),1)"#, X::N(0.0)),
    ("B90", r#"@INDEX(LINEST(E1,F1:F9,FALSE),2)"#, X::N(0.0)),
    ("B91", r#"COLUMNS(LINEST(E1,F1:F9))"#, X::N(10.0)),
    ("B92", r#"COLUMNS(LINEST(E1,F1:F9,FALSE))"#, X::N(10.0)),
    ("B93", r#"@INDEX(LINEST(E1,F1:F9,FALSE),1,3)"#, X::N(0.0)),
    ("B94", r#"@INDEX(LINEST(E1,F1:F9,FALSE),1,9)"#, X::N(0.0)),
    ("B95", r#"@INDEX(LOGEST(E1,F1:F9,FALSE),1)"#, X::N(1.0)),
];

#[test]
fn regression_with_a_one_cell_known_y_matches_excel() {
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
