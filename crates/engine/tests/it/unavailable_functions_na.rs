//! RTD, DATESTRING and NUMBERSTRING are functions Excel knows but cannot run here: RTD needs a
//! real-time data server, the other two only produce text in East Asian editions. Microsoft 365
//! Excel (English) answers `#N/A` for each after a full recalculation, not `#NAME?`.

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

fn values(formulas: &[&str]) -> Vec<Value> {
    let mut s = Session::new();
    s.new_workbook();
    for (i, f) in formulas.iter().enumerate() {
        s.execute("cell.set", json!({"cell": format!("A{}", i + 1), "input": format!("={f}")})).unwrap();
    }
    let sheet = s.doc().unwrap().wb.sheet(0).unwrap();
    (0..formulas.len()).map(|i| sheet.value(CellRef::parse(&format!("A{}", i + 1)).unwrap())).collect()
}

/// The three formulas as Excel recalculated them (all `#N/A`).
#[test]
fn excel_answers_na() {
    let formulas = [r#"RTD(1223,XL11,1)"#, r#"DATESTRING("2023-02-02")"#, r#"NUMBERSTRING(1234567890,1)"#];
    let got = values(&formulas);
    for (f, v) in formulas.iter().zip(&got) {
        assert_eq!(*v, Value::Error(CellError::NA), "={f}: Excel answers #N/A");
    }
}

/// With no real-time data server RTD is `#N/A` whatever the ProgID, server and topics are.
#[test]
fn rtd_is_na_for_any_server_and_topics() {
    let formulas = [r#"RTD("my.rtd.server","","Price")"#, r#"RTD("my.rtd.server",,"MSFT","Last","Size")"#, r#"RTD(B1,B2,B3)"#];
    let got = values(&formulas);
    for (f, v) in formulas.iter().zip(&got) {
        assert_eq!(*v, Value::Error(CellError::NA), "={f}");
    }
}

/// They are known functions: the error is `#N/A`, so ISNA is TRUE and IFNA catches it.
#[test]
fn na_is_caught_like_any_na() {
    let formulas = [
        r#"ISNA(RTD(1223,XL11,1))"#,
        r#"ISNA(DATESTRING(45000))"#,
        r#"ISNA(NUMBERSTRING(12,2))"#,
        r#"IFNA(RTD("srv","","t"),"no feed")"#,
        r#"ERROR.TYPE(NUMBERSTRING(1,1))"#,
    ];
    let got = values(&formulas);
    assert_eq!(got[0], Value::Bool(true), "={}", formulas[0]);
    assert_eq!(got[1], Value::Bool(true), "={}", formulas[1]);
    assert_eq!(got[2], Value::Bool(true), "={}", formulas[2]);
    assert_eq!(got[3], Value::Text("no feed".into()), "={}", formulas[3]);
    // ERROR.TYPE(#N/A) is 7; #NAME? would be 5.
    assert_eq!(got[4], Value::Number(7.0), "={}", formulas[4]);
}

/// A name Excel does not know still answers `#NAME?`.
#[test]
fn unknown_functions_are_still_name_errors() {
    let got = values(&[r#"NOSUCHFUNCTION(1)"#]);
    assert_eq!(got[0], Value::Error(CellError::Name));
}
