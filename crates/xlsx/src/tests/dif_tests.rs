//! DIF (`.dif`) import and export: typed values, quoted text, booleans, errors and empty cells.
use crate::{read_dif, write_dif};
use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_model::Workbook;

const SAMPLE: &str = "TABLE\r\n0,1\r\n\"EXCEL\"\r\nVECTORS\r\n0,3\r\n\"\"\r\nTUPLES\r\n0,3\r\n\"\"\r\nDATA\r\n0,0\r\n\"\"\r\n\
-1,0\r\nBOT\r\n1,0\r\n\"Name\"\r\n1,0\r\n\"Amount\"\r\n1,0\r\n\"Paid\"\r\n\
-1,0\r\nBOT\r\n1,0\r\n\"Say \"\"hi\"\"\"\r\n0,12.5\r\nV\r\n0,1\r\nTRUE\r\n\
-1,0\r\nBOT\r\n1,0\r\n\"\"\r\n0,-3\r\nV\r\n0,0\r\nNA\r\n-1,0\r\nEOD\r\n";

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

fn check_sample(wb: &Workbook) {
    let sh = &wb.sheets[0];
    assert_eq!(sh.value(at("A1")), Value::from("Name"));
    assert_eq!(sh.value(at("B1")), Value::from("Amount"));
    assert_eq!(sh.value(at("C1")), Value::from("Paid"));
    assert_eq!(sh.value(at("A2")), Value::from("Say \"hi\""));
    assert_eq!(sh.value(at("B2")), Value::Number(12.5));
    assert_eq!(sh.value(at("C2")), Value::Bool(true));
    assert!(sh.cell(at("A3")).is_none_or(|c| c.value.is_empty()), "an empty string is an empty cell");
    assert_eq!(sh.value(at("B3")), Value::Number(-3.0));
    assert_eq!(sh.value(at("C3")), Value::Error(CellError::NA));
}

#[test]
fn dif_reads_typed_values() {
    let wb = read_dif(SAMPLE.as_bytes()).unwrap().0;
    assert_eq!(wb.sheets.len(), 1);
    check_sample(&wb);
}

#[test]
fn dif_reads_lf_line_ends_and_skips_unknown_header_topics() {
    let src = SAMPLE.replace("\r\n", "\n").replace("DATA\n", "LABEL\n1,0\n\"x\"\nDATA\n");
    check_sample(&read_dif(src.as_bytes()).unwrap().0);
}

#[test]
fn dif_writes_header_counts_and_values() {
    let wb = read_dif(SAMPLE.as_bytes()).unwrap().0;
    let out = String::from_utf8(write_dif(&wb.sheets[0]).unwrap()).unwrap();
    assert!(out.starts_with("TABLE\r\n0,1\r\n"), "{out}");
    assert!(out.contains("VECTORS\r\n0,3\r\n\"\"\r\n"), "{out}");
    assert!(out.contains("TUPLES\r\n0,3\r\n\"\"\r\n"), "{out}");
    assert!(out.contains("DATA\r\n0,0\r\n\"\"\r\n"), "{out}");
    assert!(out.contains("0,12.5\r\nV\r\n"), "{out}");
    assert!(out.contains("1,0\r\n\"Say \"\"hi\"\"\"\r\n"), "{out}");
    assert!(out.contains("0,1\r\nTRUE\r\n"), "{out}");
    assert!(out.contains("0,0\r\nNA\r\n"), "{out}");
    assert!(out.ends_with("-1,0\r\nEOD\r\n"), "{out}");
    assert_eq!(out.matches("-1,0\r\nBOT\r\n").count(), 3);
}

#[test]
fn dif_round_trips_values_exactly() {
    let wb = read_dif(SAMPLE.as_bytes()).unwrap().0;
    let again = read_dif(&write_dif(&wb.sheets[0]).unwrap()).unwrap().0;
    check_sample(&again);
    let src =
        "TABLE\r\n0,1\r\n\"\"\r\nDATA\r\n0,0\r\n\"\"\r\n-1,0\r\nBOT\r\n0,0.30000000000000004\r\nV\r\n0,0\r\nERROR\r\n0,0\r\nFALSE\r\n-1,0\r\nEOD\r\n";
    let wb = read_dif(src.as_bytes()).unwrap().0;
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(0.1 + 0.2));
    assert_eq!(wb.sheets[0].value(at("B1")), Value::Error(CellError::Value));
    assert_eq!(wb.sheets[0].value(at("C1")), Value::Bool(false));
    let again = read_dif(&write_dif(&wb.sheets[0]).unwrap()).unwrap().0;
    assert_eq!(again.sheets[0].value(at("A1")), Value::Number(0.1 + 0.2));
    assert_eq!(again.sheets[0].value(at("B1")), Value::Error(CellError::Value));
    assert_eq!(again.sheets[0].value(at("C1")), Value::Bool(false));
}

#[test]
fn dif_malformed_input_never_panics() {
    for src in [
        "",
        "TABLE",
        "TABLE\r\n0,1\r\n",
        "DATA\r\n0,0\r\n\"\"\r\n0,abc\r\nV\r\n1,0\r\n\"open\r\n",
        "DATA\r\n0,0\r\n\"\"\r\n-1,0\r\nBOT\r\n9,9\r\n?\r\n",
    ] {
        let _ = read_dif(src.as_bytes());
    }
}
