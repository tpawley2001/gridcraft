//! CUMIPMT and CUMPRINC with fractional periods, fractional nper and argument validation. Expected values are
//! Microsoft Excel's own recalculated answers for the same formulas.

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
    ("B1", r#"CUMIPMT(0.09/12,360,125000,13.2,24,0)"#, X::N(-10204.137103548666)),
    ("B2", r#"CUMIPMT(0.09/12,360,125000,13.5,24,0)"#, X::N(-10204.137103548666)),
    ("B3", r#"CUMIPMT(0.09/12,360,125000,13.7,24,0)"#, X::N(-10204.137103548666)),
    ("B4", r#"CUMIPMT(0.09/12,360,125000,13,24.5,0)"#, X::N(-11135.232130750841)),
    ("B5", r#"CUMIPMT(0.09/12,360,125000,13,24.7,0)"#, X::N(-11135.232130750841)),
    ("B6", r#"CUMIPMT(0.09/12,360,125000,13,24.2,0)"#, X::N(-11135.232130750841)),
    ("B7", r#"CUMIPMT(0.09/12,360.2,125000,13,24,0)"#, X::N(-11135.415973843741)),
    ("B8", r#"CUMIPMT(0.09/12,360.5,125000,13,24,0)"#, X::N(-11135.691149132714)),
    ("B9", r#"CUMIPMT(0.09/12,360.7,125000,13,24,0)"#, X::N(-11135.87420730183)),
    ("B10", r#"CUMIPMT(0.09/12,360,125000,0.5,1,0)"#, X::E(r#"#NUM!"#)),
    ("B11", r#"CUMIPMT(0.09/12,360,125000,359.5,360,0)"#, X::N(-7.487183160155951)),
    ("B12", r#"CUMIPMT(0.09/12,360.5,125000,360,360.5,0)"#, X::N(-11.206804175149955)),
    ("B13", r#"CUMIPMT(0.09/12,360,125000,13.5,13.7,0)"#, X::N(0.0)),
    ("B14", r#"CUMIPMT(0.09/12,360,125000,1.0000001,2,0)"#, X::N(-936.9879129661424)),
    ("B15", r#"CUMPRINC(0.09/12,360,125000,13.2,24,0)"#, X::N(-859.4238794420938)),
    ("B16", r#"CUMPRINC(0.09/12,360,125000,13.5,24,0)"#, X::N(-859.4238794420938)),
    ("B17", r#"CUMPRINC(0.09/12,360,125000,13.7,24,0)"#, X::N(-859.4238794420938)),
    ("B18", r#"CUMPRINC(0.09/12,360,125000,13,24.5,0)"#, X::N(-934.1071234208979)),
    ("B19", r#"CUMPRINC(0.09/12,360,125000,13,24.7,0)"#, X::N(-934.1071234208979)),
    ("B20", r#"CUMPRINC(0.09/12,360,125000,13,24.2,0)"#, X::N(-934.1071234208979)),
    ("B21", r#"CUMPRINC(0.09/12,360.2,125000,13,24,0)"#, X::N(-932.610806169454)),
    ("B22", r#"CUMPRINC(0.09/12,360.5,125000,13,24,0)"#, X::N(-930.3711270737314)),
    ("B23", r#"CUMPRINC(0.09/12,360.7,125000,13,24,0)"#, X::N(-928.8811983951742)),
    ("B24", r#"CUMPRINC(0.09/12,360,125000,0.5,1,0)"#, X::E(r#"#NUM!"#)),
    ("B25", r#"CUMPRINC(0.09/12,360,125000,359.5,360,0)"#, X::N(-998.2910880208223)),
    ("B26", r#"CUMPRINC(0.09/12,360.5,125000,360,360.5,0)"#, X::N(-994.2983855087205)),
    ("B27", r#"CUMPRINC(0.09/12,360,125000,13.5,13.7,0)"#, X::N(0.0)),
    ("B28", r#"CUMPRINC(0.09/12,360,125000,1.0000001,2,0)"#, X::N(-68.79035821483576)),
];

#[test]
fn cumipmt_cumprinc_fractional_args_match_excel() {
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
