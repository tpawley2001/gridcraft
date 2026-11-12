//! Legacy `.xlsb` cell formulas are read with Excel's implicit intersection operator `@`, exactly
//! as the same formula is read from an `.xlsx` saved without dynamic-array metadata, so opening an
//! `.xlsb` and saving it as `.xlsx` does not change any formula's text.
use super::xlsb_tests::{file, ftext, ptg_area, ptg_int, record, row, worksheet};
use crate::{FunctionInfo, read_xlsb, read_xlsb_with, read_xlsx_with, write_xlsx};

fn array_ptg() -> Vec<u8> {
    [vec![0x60], vec![0; 14]].concat()
}
fn numbers(values: &[f64]) -> Vec<u8> {
    let mut out = [1u32.to_le_bytes(), (values.len() as u32).to_le_bytes()].concat();
    for v in values {
        out.push(0);
        out.extend(v.to_le_bytes());
    }
    out
}
fn fmla(col: u32, rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    let parsed = [(rgce.len() as u32).to_le_bytes().to_vec(), rgce.to_vec(), (rgcb.len() as u32).to_le_bytes().to_vec(), rgcb.to_vec()].concat();
    record(9, &[col.to_le_bytes().to_vec(), vec![0; 4], 99.0f64.to_le_bytes().to_vec(), vec![0; 2], parsed].concat())
}
/// `INDEX({10,20,30},2)`.
fn index_formula() -> (Vec<u8>, Vec<u8>) {
    ([array_ptg(), ptg_int(2), vec![0x22, 2, 29, 0]].concat(), numbers(&[10.0, 20.0, 30.0]))
}
/// `SUM({1,2,3})`.
fn sum_formula() -> (Vec<u8>, Vec<u8>) {
    ([array_ptg(), vec![0x22, 1, 4, 0]].concat(), numbers(&[1.0, 2.0, 3.0]))
}
/// `SQRT(C1:C3)`: a range where one value is expected.
fn sqrt_formula() -> (Vec<u8>, Vec<u8>) {
    ([ptg_area(0, 2, 2, 2), vec![0x21, 20, 0]].concat(), vec![])
}
fn book(formula: &(Vec<u8>, Vec<u8>)) -> Vec<u8> {
    file(&worksheet(&[row(0), fmla(0, &formula.0, &formula.1)].concat(), &[]))
}
/// The function library's facts for a workbook where `SUM` stands in for a VBA / add-in function.
fn sum_is_user_defined() -> FunctionInfo {
    FunctionInfo { is_builtin: |name| name != "SUM", ..FunctionInfo::default() }
}

#[test]
fn xlsb_legacy_index_reads_with_the_at_excel_shows() {
    let (wb, _) = read_xlsb(&book(&index_formula())).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("@INDEX({10,20,30},2)"));
}

#[test]
fn xlsb_legacy_range_in_a_single_value_argument_reads_with_at() {
    let (wb, _) = read_xlsb(&book(&sqrt_formula())).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("SQRT(@C1:C3)"));
}

#[test]
fn xlsb_legacy_user_defined_function_call_reads_with_at() {
    let (wb, _) = read_xlsb_with(&book(&sum_formula()), sum_is_user_defined()).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("@SUM({1,2,3})"));
}

#[test]
fn xlsb_legacy_formula_without_intersection_is_unchanged() {
    let (wb, _) = read_xlsb(&book(&sum_formula())).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("SUM({1,2,3})"));
}

#[test]
fn xlsb_formula_text_is_the_same_after_saving_as_xlsx() {
    for (formula, functions) in [
        (index_formula(), FunctionInfo::default()),
        (sqrt_formula(), FunctionInfo::default()),
        (sum_formula(), FunctionInfo::default()),
        (sum_formula(), sum_is_user_defined()),
        (index_formula(), sum_is_user_defined()),
    ] {
        let (wb, _) = read_xlsb_with(&book(&formula), functions).unwrap();
        let (back, _) = read_xlsx_with(&write_xlsx(&wb).unwrap(), functions).unwrap();
        assert_eq!(ftext(&wb, "A1"), ftext(&back, "A1"));
    }
}
