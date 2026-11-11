//! When several arguments of a function are errors, Excel lets an error produced by a computed argument (a literal,
//! `-A1`, `A1+0`, `1/0`, a function result) win over an error found inside a reference or range argument; among
//! computed arguments the first one wins, otherwise the first error in the references. Expected values are
//! Microsoft Excel's own recalculated answers (ERROR.TYPE of each result) for the same formulas and inputs.

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

/// E1 #REF!, E2 #VALUE!, E3 #DIV/0!, E4 #N/A, E5 1, E6 #NUM!.
const INPUTS: [(&str, &str); 6] = [("E1", "=INDEX(E5:E5,2)"), ("E2", "=\"a\"+1"), ("E3", "=1/0"), ("E4", "=NA()"), ("E5", "1"), ("E6", "=SQRT(-1)")];

/// Not an error: ERROR.TYPE itself returns #N/A.
const NA: i32 = 0;

/// (formula inside ERROR.TYPE, Excel's ERROR.TYPE result; NA = the result was not an error)
const CASES: &[(&str, i32)] = &[
    ("SUM(E1,E2)", 4),
    ("SUM(E1,-E2)", 3),
    ("SUM(-E2,E1)", 3),
    ("SUM(-E1,E2)", 4),
    ("SUM(-E1,-E2)", 4),
    ("SUM(E1,+E2)", 3),
    ("SUM(E1,(E2))", 4),
    ("SUM(E1,E2+0)", 3),
    ("SUM(E1:E2)", 4),
    ("SUM(E2:E3)", 3),
    ("SUM(E3:E4)", 2),
    ("SUM(E1:E2,1/0)", 2),
    ("SUM(1/0,E1:E2)", 2),
    ("SUM(E1,#N/A)", 7),
    ("SUM(#N/A,E1)", 7),
    ("SUM(#N/A,#REF!)", 7),
    ("SUM(E6,-E2)", 3),
    ("SUM(E4,-E3)", 2),
    ("SUM(E3,-E4)", 7),
    ("SUM(E1,E3,-E2)", 3),
    ("SUM(E1,-E3,-E2)", 2),
    ("SUM(E3,-E2,E1)", 3),
    ("SUM(E1,N(E2))", 3),
    ("MAX(E1,-E2)", 3),
    ("MIN(-E2,E1)", 3),
    ("AVERAGE(E1,-E2)", 3),
    ("PRODUCT(E1,-E2)", 3),
    ("MAX(E1:E2,-E3)", 2),
    ("COUNT(E1,-E2)+100", NA),
    ("STDEV(E1,-E2,E5)", 3),
    ("E1+E2", 4),
    ("E2+E1", 3),
    ("-E1+E2", 4),
    ("E1+-E2", 4),
    ("E1&E2", 4),
    ("E1=E2", 4),
    ("#N/A+#REF!", 7),
    ("E1+E2%", 4),
    ("E3^E4", 2),
    ("CONCAT(E1,E2)", 4),
    ("CONCATENATE(E1,-E2)", 4),
    (r#"TEXTJOIN(",",TRUE,E1,-E2)"#, 3),
    ("IF(E1,E2,E3)", 4),
    ("AND(E1,E2)", 4),
    ("AND(E1,-E2)", 3),
    ("OR(-E1,E2)", 4),
    ("SUMPRODUCT(E1:E2,E3:E4)", 4),
    ("SUMPRODUCT(E3:E4,E1:E2)", 2),
    ("SUMPRODUCT(E1:E2*E3:E4)", 4),
    ("ROUND(E1,E2)", 4),
    ("ROUND(E2,E1)", 3),
    ("POWER(E3,-E4)", 2),
    ("MOD(E2,E1)", 3),
    ("DATE(E1,E2,E3)", 4),
    ("VLOOKUP(E1,E2:E3,1)", 4),
    ("INDEX(E5:E6,E2)", 3),
    ("LEFT(E1,E2)", 4),
    ("LEFT(E2,-E1)", 3),
    ("CHOOSE(E1,E2,E3)", 4),
    ("SUMIF(E5:E6,E2,E1:E2)", NA),
    ("NPV(0.1,E1,-E2)", 3),
    ("MAX(E1,E3,-E4)", 7),
    ("SUM((E1,E2))", 4),
    ("SUM((E2,E1))", 3),
];

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(CellRef::parse(cell).unwrap())
}

#[test]
fn error_precedence_matches_excel() {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in INPUTS {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    for (i, (f, _)) in CASES.iter().enumerate() {
        let cell = format!("B{}", i + 1);
        s.execute("cell.set", json!({"cell": cell, "input": format!("=ERROR.TYPE({f})")})).unwrap();
    }
    let mut wrong = Vec::new();
    for (i, (f, want)) in CASES.iter().enumerate() {
        let got = value(&s, &format!("B{}", i + 1));
        let expected = if *want == NA { Value::Error(CellError::NA) } else { Value::Number(f64::from(*want)) };
        if got != expected {
            wrong.push(format!("ERROR.TYPE({f}): Excel {expected:?}, got {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ from Excel:\n{}", wrong.len(), CASES.len(), wrong.join("\n"));
}
