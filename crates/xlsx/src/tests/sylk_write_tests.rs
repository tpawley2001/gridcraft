//! SYLK (`.slk`) save: values, R1C1 formulas, number formats and column widths read back the same.
use crate::sylk::{a1_to_r1c1, r1c1_to_a1};
use crate::{read_sylk, write_sylk};
use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_model::Workbook;

const SAMPLE: &str = "ID;PWXL;N;E\r\nP;PGeneral\r\nP;P0.00\r\nF;W2 2 20\r\n\
C;Y1;X1;K\"a;;b c\"\r\nC;X2;K0\r\nC;X3;K-1.5\r\nC;X4;K1E-07\r\nC;X5;K123456789012\r\n\
C;Y2;X1;K3;ER[-1]C*3\r\nF;P1;Y2;X2\r\nC;X2;K12.5\r\nC;X3;KTRUE\r\nC;X4;KFALSE\r\nC;X5;K#DIV/0!\r\n\
C;Y3;X2;K3;ESUM(R1C1:R[-1]C[-1])\r\nE\r\n";

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

fn formula(wb: &Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.to_string())
}

fn round_trip() -> (Vec<u8>, Workbook) {
    let (wb, _) = read_sylk(SAMPLE.as_bytes()).unwrap();
    let bytes = write_sylk(&wb, 0).unwrap();
    let (back, _) = read_sylk(&bytes).unwrap();
    (bytes, back)
}

#[test]
fn sylk_save_starts_with_id_and_ends_with_e() {
    let (bytes, _) = round_trip();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("ID;"), "{text}");
    assert!(text.trim_end().ends_with("\r\nE"), "{text}");
}

#[test]
fn sylk_save_round_trips_values() {
    let (_, back) = round_trip();
    let sh = &back.sheets[0];
    assert_eq!(sh.value(at("A1")), Value::from("a;b c"));
    assert_eq!(sh.value(at("B1")), Value::Number(0.0));
    assert_eq!(sh.value(at("C1")), Value::Number(-1.5));
    assert_eq!(sh.value(at("D1")), Value::Number(1e-7));
    assert_eq!(sh.value(at("E1")), Value::Number(123456789012.0));
    assert_eq!(sh.value(at("B2")), Value::Number(12.5));
    assert_eq!(sh.value(at("C2")), Value::Bool(true));
    assert_eq!(sh.value(at("D2")), Value::Bool(false));
    assert_eq!(sh.value(at("E2")), Value::Error(CellError::Div0));
}

#[test]
fn sylk_save_round_trips_formulas_and_cached_values() {
    let (_, back) = round_trip();
    assert_eq!(formula(&back, "A2").as_deref(), Some("A1*3"));
    assert_eq!(back.sheets[0].value(at("A2")), Value::Number(3.0));
    assert_eq!(formula(&back, "B3").as_deref(), Some("SUM($A$1:A2)"));
    assert_eq!(formula(&back, "B2"), None);
}

#[test]
fn sylk_save_round_trips_number_formats_and_widths() {
    let (wb, _) = read_sylk(SAMPLE.as_bytes()).unwrap();
    let (_, back) = round_trip();
    let sh = &back.sheets[0];
    assert_eq!(back.styles.get(sh.style_id(at("B2"))).num_fmt.as_str(), "0.00");
    assert_eq!(back.styles.get(sh.style_id(at("C2"))).num_fmt.as_str(), "General");
    assert_eq!(sh.col_width(1), wb.sheets[0].col_width(1));
    assert_eq!(sh.col_width(0), wb.sheets[0].col_width(0));
}

#[test]
fn sylk_save_refuses_a_missing_sheet() {
    let (wb, _) = read_sylk(SAMPLE.as_bytes()).unwrap();
    assert!(write_sylk(&wb, 5).is_err());
}

#[test]
fn a1_to_r1c1_is_the_inverse_of_r1c1_to_a1() {
    // (A1 text, cell it sits in, R1C1 text)
    let table = [
        ("$A$1", "C5", "R1C1"),
        ("B3", "C5", "R[-2]C[-1]"),
        ("C5", "C5", "RC"),
        ("$B3", "C5", "R[-2]C2"),
        ("B$3", "C5", "R3C[-1]"),
        ("SUM(A1:B2)", "C5", "SUM(R[-4]C[-2]:R[-3]C[-1])"),
        ("\"A1\"&A1", "B2", "\"A1\"&R[-1]C[-1]"),
        ("'My Sheet'!D10*2", "A1", "'My Sheet'!R[9]C[3]*2"),
    ];
    for (a1, cell, r1c1) in table {
        assert_eq!(a1_to_r1c1(a1, at(cell)).as_deref(), Some(r1c1), "{a1} at {cell}");
        assert_eq!(r1c1_to_a1(r1c1, at(cell)).as_deref(), Some(a1), "{r1c1} at {cell}");
    }
}
