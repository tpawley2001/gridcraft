//! SYLK (`.slk`) import: records, R1C1 formulas, number formats and column widths.
use crate::sylk::r1c1_to_a1;
use crate::{is_sylk, read_sylk};
use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_model::{Workbook, points_to_chars};

const SAMPLE: &str = "ID;PWXL;N;E\r\nP;PGeneral\r\nP;P0.00\r\nP;P#,##0;;-#,##0\r\nF;W1 1 20\r\nB;Y4;X3;D0 0 3 2\r\n\
C;Y1;X1;K\"Name\"\r\nC;X2;K\"Amount\"\r\nC;Y2;X1;K\"North\"\r\nC;X2;K12.5\r\nF;P1;Y2;X2\r\nC;Y3;X1;K\"South\"\r\nC;X2;K-3\r\n\
C;Y4;X1;K\"Total\"\r\nC;X2;K9.5;ER[-2]C+R[-1]C\r\nC;X3;KTRUE\r\nC;Y1;X3;K#N/A;ENA()\r\nE\r\n";

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

fn sample() -> Workbook {
    read_sylk(SAMPLE.as_bytes()).unwrap().0
}

fn formula(wb: &Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.to_string())
}

#[test]
fn sylk_is_recognised_by_its_id_record() {
    assert!(is_sylk(SAMPLE.as_bytes()));
    assert!(is_sylk(b"\xEF\xBB\xBFID;PWXL;N;E\r\nE\r\n"));
    assert!(!is_sylk(b"Name,Amount\r\nNorth,12.5\r\n"));
    assert!(!is_sylk(b"PK\x03\x04"));
}

#[test]
fn sylk_cells_read_text_numbers_booleans_and_errors() {
    let wb = sample();
    assert_eq!(wb.sheets.len(), 1);
    let sh = &wb.sheets[0];
    assert_eq!(sh.value(at("A1")), Value::from("Name"));
    assert_eq!(sh.value(at("B1")), Value::from("Amount"));
    assert_eq!(sh.value(at("A2")), Value::from("North"));
    assert_eq!(sh.value(at("B2")), Value::Number(12.5));
    assert_eq!(sh.value(at("A3")), Value::from("South"));
    assert_eq!(sh.value(at("B3")), Value::Number(-3.0));
    assert_eq!(sh.value(at("C4")), Value::Bool(true));
    assert_eq!(sh.value(at("C1")), Value::Error(CellError::NA));
}

#[test]
fn sylk_formulas_convert_from_r1c1_and_keep_cached_values() {
    let wb = sample();
    assert_eq!(formula(&wb, "B4").as_deref(), Some("B2+B3"));
    assert_eq!(wb.sheets[0].value(at("B4")), Value::Number(9.5));
    assert_eq!(formula(&wb, "C1").as_deref(), Some("NA()"));
    assert_eq!(formula(&wb, "B2"), None);
}

#[test]
fn sylk_number_formats_and_column_widths_apply() {
    let wb = sample();
    let sh = &wb.sheets[0];
    assert_eq!(wb.styles.get(sh.style_id(at("B2"))).num_fmt.as_str(), "0.00");
    assert_eq!(wb.styles.get(sh.style_id(at("B3"))).num_fmt.as_str(), "General");
    let a = sh.cols.get(&0).and_then(|c| c.size).expect("column A has a width");
    assert!((points_to_chars(f64::from(a)) - 20.0).abs() < 1.0, "column A is about 20 characters wide: {}", points_to_chars(f64::from(a)));
}

#[test]
fn sylk_escaped_semicolons_in_formats_and_text() {
    let src = "ID;P\r\nP;P#,##0;;-#,##0\r\nC;Y1;X1;K-1234\r\nF;P0;Y1;X1\r\nC;Y2;X1;K\"a;;b\"\r\nE\r\n";
    let wb = read_sylk(src.as_bytes()).unwrap().0;
    let sh = &wb.sheets[0];
    assert_eq!(wb.styles.get(sh.style_id(at("A1"))).num_fmt.as_str(), "#,##0;-#,##0");
    assert_eq!(sh.value(at("A1")), Value::Number(-1234.0));
    assert_eq!(sh.value(at("A2")), Value::from("a;b"));
}

#[test]
fn sylk_r1c1_references_convert_to_a1() {
    let c3 = at("C3");
    for (r1c1, a1) in [
        ("R[-1]C", "C2"),
        ("RC[-2]", "A3"),
        ("R1C1", "$A$1"),
        ("R[-2]C[-2]:R[-1]C[-1]", "A1:B2"),
        ("SUM(R1C1:R2C1)", "SUM($A$1:$A$2)"),
        ("R1C[1]", "D$1"),
        ("\"R1C1\"&RC", "\"R1C1\"&C3"),
        ("ROUND(RC[-1],2)", "ROUND(B3,2)"),
    ] {
        assert_eq!(r1c1_to_a1(r1c1, c3).as_deref(), Some(a1), "{r1c1}");
    }
}

#[test]
fn sylk_malformed_input_never_panics() {
    for src in [
        "ID;P\r\nC;Y0;X0;K1\r\nE\r\n",
        "ID;P\r\nC;Y99999999;X99999;K1\r\nE\r\n",
        "ID;P\r\nC;K\"unterminated\r\n",
        "ID;P\r\nF;W\r\nF;W9 1 x\r\nP;P\r\nF;P42;Y1;X1\r\nC;Y1;X1;E)(\r\n",
        "ID;",
    ] {
        let _ = read_sylk(src.as_bytes());
    }
}
