//! LAMBDA helpers (MAP, SCAN, REDUCE, BYROW, BYCOL, MAKEARRAY), LAMBDA as a value, GROUPBY and PIVOTBY.
//! Expected values are Microsoft Excel's own recalculated answers for the same formulas.

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
    ("E1", r#"3"#),
    ("F1", r#"1"#),
    ("H1", r#"a"#),
    ("I1", r#"p"#),
    ("E2", r#"5"#),
    ("F2", r#"3"#),
    ("G2", r#"x"#),
    ("H2", r#"b"#),
    ("I2", r#"q"#),
    ("E3", r#"5"#),
    ("F3", r#"5"#),
    ("G3", r#"0"#),
    ("H3", r#"a"#),
    ("I3", r#"p"#),
    ("E4", r#"8"#),
    ("F4", r#"7"#),
    ("H4", r#"c"#),
    ("I4", r#"p"#),
    ("E5", r#"1"#),
    ("F5", r#"9"#),
    ("H5", r#"b"#),
    ("I5", r#"q"#),
    ("E6", r#"9"#),
    ("F6", r#"11"#),
    ("H6", r#"a"#),
    ("I6", r#"q"#),
    ("E7", r#"5"#),
    ("F7", r#"13"#),
    ("H7", r#"c"#),
    ("I7", r#"p"#),
    ("E8", r#"2"#),
    ("F8", r#"15"#),
    ("H8", r#"a"#),
    ("I8", r#"q"#),
    ("E9", r#"8"#),
    ("F9", r#"17"#),
    ("H9", r#"b"#),
    ("I9", r#"p"#),
    ("E10", r#"4"#),
    ("F10", r#"19"#),
    ("H10", r#"A"#),
    ("I10", r#"p"#),
];

/// (cell, formula, Excel's answer)
const CASES: &[(&str, &str, X)] = &[
    ("B2", r#"TEXTJOIN(",",FALSE,{1,2})"#, X::T(r#"1,2"#)),
    ("B4", r#"TEXTJOIN(",",FALSE,MAP(E1:E3,LAMBDA(v,v*2)))"#, X::T(r#"6,10,10"#)),
    ("G4", r#""""#, X::T(r#""#)),
    ("B5", r#"SUM(MAP(E1:E3,LAMBDA(v,v*2)))"#, X::N(26.0)),
    ("B6", r#"ROWS(MAP(E1:E3,LAMBDA(v,v*2)))*10+COLUMNS(MAP(E1:E3,LAMBDA(v,v*2)))"#, X::N(31.0)),
    ("B7", r#"INDEX(MAP(E1:E3,LAMBDA(v,v*2)),2,1)"#, X::N(10.0)),
    ("B8", r#"TEXTJOIN(",",FALSE,SCAN(1,E1:E4,LAMBDA(a,v,a*v)))"#, X::T(r#"3,15,75,600"#)),
    ("B9", r#"SUM(SCAN(1,E1:E4,LAMBDA(a,v,a*v)))"#, X::N(693.0)),
    ("B10", r#"ROWS(SCAN(1,E1:E4,LAMBDA(a,v,a*v)))*10+COLUMNS(SCAN(1,E1:E4,LAMBDA(a,v,a*v)))"#, X::N(41.0)),
    ("B11", r#"INDEX(SCAN(1,E1:E4,LAMBDA(a,v,a*v)),2,1)"#, X::N(15.0)),
    ("B12", r#"TEXTJOIN(",",FALSE,BYROW(E1:F3,LAMBDA(r,SUM(r))))"#, X::T(r#"4,8,10"#)),
    ("B13", r#"SUM(BYROW(E1:F3,LAMBDA(r,SUM(r))))"#, X::N(22.0)),
    ("B14", r#"ROWS(BYROW(E1:F3,LAMBDA(r,SUM(r))))*10+COLUMNS(BYROW(E1:F3,LAMBDA(r,SUM(r))))"#, X::N(31.0)),
    ("B15", r#"INDEX(BYROW(E1:F3,LAMBDA(r,SUM(r))),2,1)"#, X::N(8.0)),
    ("B16", r#"TEXTJOIN(",",FALSE,BYCOL(E1:F3,LAMBDA(c,MAX(c))))"#, X::T(r#"5,5"#)),
    ("B17", r#"SUM(BYCOL(E1:F3,LAMBDA(c,MAX(c))))"#, X::N(10.0)),
    ("B18", r#"ROWS(BYCOL(E1:F3,LAMBDA(c,MAX(c))))*10+COLUMNS(BYCOL(E1:F3,LAMBDA(c,MAX(c))))"#, X::N(12.0)),
    ("B19", r#"INDEX(BYCOL(E1:F3,LAMBDA(c,MAX(c))),2,1)"#, X::E(r#"#REF!"#)),
    ("B20", r#"TEXTJOIN(",",FALSE,MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))"#, X::T(r#"11,12,13,21,22,23"#)),
    ("B21", r#"SUM(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))"#, X::N(102.0)),
    ("B22", r#"ROWS(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))*10+COLUMNS(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)))"#, X::N(23.0)),
    ("B23", r#"INDEX(MAKEARRAY(2,3,LAMBDA(r,c,r*10+c)),2,1)"#, X::N(21.0)),
    ("B24", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,E1:E10,LAMBDA(v,SUM(v))))"#, X::T(r#"a,23,b,14,c,13,Total,50"#)),
    ("B25", r#"SUM(GROUPBY(H1:H10,E1:E10,LAMBDA(v,SUM(v))))"#, X::N(100.0)),
    ("B26", r#"ROWS(GROUPBY(H1:H10,E1:E10,LAMBDA(v,SUM(v))))*10+COLUMNS(GROUPBY(H1:H10,E1:E10,LAMBDA(v,SUM(v))))"#, X::N(42.0)),
    ("B27", r#"INDEX(GROUPBY(H1:H10,E1:E10,LAMBDA(v,SUM(v))),2,1)"#, X::T(r#"b"#)),
    ("B28", r#"TEXTJOIN(",",FALSE,MAP(E1:E3,F1:F3,LAMBDA(a,b,a&b)))"#, X::T(r#"31,53,55"#)),
    ("B29", r#"TEXTJOIN(",",FALSE,MAP(E1:E3,F1:F2,LAMBDA(a,b,a+b)))"#, X::E(r#"#N/A"#)),
    ("B30", r#"MAP(5,LAMBDA(v,v+1))"#, X::N(6.0)),
    ("B31", r#"ERROR.TYPE(MAP(E1:E3,LAMBDA(a,b,a)))"#, X::N(3.0)),
    ("B32", r#"ERROR.TYPE(MAP(E1:E3,5))"#, X::N(3.0)),
    ("B33", r#"ERROR.TYPE(INDEX(MAP(E1:E3,LAMBDA(v,{1,2}*v)),1))"#, X::N(14.0)),
    ("B34", r#"TEXTJOIN(",",FALSE,MAP(G1:G3,LAMBDA(v,ISBLANK(v))))"#, X::T(r#"TRUE,FALSE,FALSE"#)),
    ("B35", r#"TEXTJOIN(",",FALSE,SCAN("",H1:H3,LAMBDA(a,v,a&v)))"#, X::T(r#"a,ab,aba"#)),
    ("B36", r#"TEXTJOIN(",",FALSE,SCAN(0,{1,0,2},LAMBDA(a,v,IFERROR(a+1/v,-1))))"#, X::T(r#"1,-1,-0.5"#)),
    ("B37", r#"TEXTJOIN(",",FALSE,SCAN(,E1:E3,LAMBDA(a,v,a+v)))"#, X::T(r#"3,8,13"#)),
    ("B38", r#"TEXTJOIN(",",FALSE,SCAN(0,E1:F2,LAMBDA(a,v,a+v)))"#, X::T(r#"3,4,9,12"#)),
    ("B39", r#"ERROR.TYPE(REDUCE(0,E1:E3,LAMBDA(a,v,{1,2}+v)))"#, X::E(r#"#N/A"#)),
    ("B40", r#"ROWS(REDUCE("h",E1:E3,LAMBDA(a,v,VSTACK(a,v))))"#, X::N(4.0)),
    ("B41", r#"REDUCE(0,E1:F3,LAMBDA(a,v,a+v))"#, X::N(22.0)),
    ("B44", r#"TEXTJOIN(",",FALSE,BYROW(E1:F3,LAMBDA(r,INDEX(r,1,2))))"#, X::T(r#"1,3,5"#)),
    ("B45", r#"ERROR.TYPE(BYROW(E1:F3,LAMBDA(a,b,a)))"#, X::N(3.0)),
    ("B46", r#"TEXTJOIN(",",FALSE,BYCOL(H1:H3,LAMBDA(c,COUNTA(c))))"#, X::T(r#"3"#)),
    ("B47", r#"TEXTJOIN(",",FALSE,BYROW(E1:F3,SUM))"#, X::T(r#"4,8,10"#)),
    ("B49", r#"ERROR.TYPE(MAKEARRAY(0,2,LAMBDA(r,c,r)))"#, X::N(3.0)),
    ("B50", r#"ERROR.TYPE(MAKEARRAY(-1,2,LAMBDA(r,c,r)))"#, X::N(3.0)),
    ("B51", r#"ERROR.TYPE(MAKEARRAY("a",2,LAMBDA(r,c,r)))"#, X::N(3.0)),
    ("B52", r#"ERROR.TYPE(LAMBDA(q,q))"#, X::E(r#"#N/A"#)),
    ("B53", r#"TYPE(LAMBDA(q,q))"#, X::N(128.0)),
    ("B54", r#"ISERROR(LAMBDA(q,q))"#, X::B(false)),
    ("B55", r#"ISREF(LAMBDA(q,q))"#, X::B(false)),
    ("B56", r#"ERROR.TYPE(LAMBDA(q,q)(1,2))"#, X::N(3.0)),
    ("B57", r#"LAMBDA(a,b,IF(ISOMITTED(b),"om",b))(1)"#, X::E(r#"#VALUE!"#)),
    ("B58", r#"ROWS(MAP(E1:E5,LAMBDA(v,v)))"#, X::N(5.0)),
    ("B59", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,E1:E10,SUM))"#, X::T(r#"a,23,b,14,c,13,Total,50"#)),
    ("B60", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,E1:E10,SUM,3))"#, X::T(r#"a,3,a,20,b,14,c,13,Total,47"#)),
    ("B61", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,E1:E10,SUM,0,1,-2))"#, X::T(r#"a,23,b,14,c,13,Total,50"#)),
    ("B62", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,E1:E10,SUM,0,0,,E1:E10>3))"#, X::T(r#"a,18,b,13,c,13"#)),
    ("B63", r#"ROWS(GROUPBY(H1:I10,E1:E10,SUM,0,2))"#, X::N(9.0)),
    ("B64", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,E1:E10,PERCENTOF,0,0))"#, X::T(r#"a,0.46,b,0.28,c,0.26"#)),
    ("B65", r#"TEXTJOIN(",",FALSE,GROUPBY(H1:H10,H1:H10,COUNTA,0,0))"#, X::T(r#"a,5,b,3,c,2"#)),
    ("B66", r#"COLUMNS(GROUPBY(H1:H10,E1:E10,HSTACK(SUM,MAX)))"#, X::N(3.0)),
    ("B67", r#"TEXTJOIN(",",FALSE,PIVOTBY(H1:H10,I1:I10,E1:E10,SUM))"#, X::T(r#",p,q,Total,a,12,11,23,b,8,6,14,c,13,,13,Total,33,17,50"#)),
    ("B68", r#"ROWS(PIVOTBY(H1:H10,I1:I10,E1:E10,SUM))*10+COLUMNS(PIVOTBY(H1:H10,I1:I10,E1:E10,SUM))"#, X::N(54.0)),
];

#[test]
fn lambda_helpers_and_groupby_match_excel() {
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
