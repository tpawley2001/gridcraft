//! Excel 4.0 command-equivalent functions (PtgFuncVar with `fCeFunc`, [MS-XLS] 2.5.198.63) are
//! read by name only on macro sheets. In an ordinary worksheet formula the cell keeps its saved
//! value, as a command can't be written into a worksheet `<f>` (Excel repairs such a file).
//! The BIFF8 streams and XLSB packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec};
use super::xlsb_tests::{file, ptg_int, record, row, worksheet};
use crate::{read_xls, read_xlsb, write_xlsx};

const KEPT: &str = "Some formulas use macro sheet commands; their saved results were kept as values.";

fn boundsheet(pos: u32, name: &str) -> Vec<u8> {
    rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, name.len() as u8, 0], name.as_bytes()]))
}

fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// PtgFuncVar (value class) with `cparams` (argument count, bit 7 = fPrompt) and `tab`.
fn func_var(cparams: u8, tab: u16) -> Vec<u8> {
    cat(&[&[0x42, cparams], &tab.to_le_bytes()])
}

/// WINDOW.RESTORE: Cetab 0x00A3 with fCeFunc.
fn window_restore() -> Vec<u8> {
    func_var(0, 0x8000 | 0x00A3)
}

/// One worksheet `Data` holding `records`.
fn xls(records: &[u8]) -> Vec<u8> {
    let globals = |pos: u32| cat(&[&bof(0x0005), &boundsheet(pos, "Data"), &eof()]);
    let data = cat(&[&bof(0x0010), records, &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &data]))
}

fn sheet_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            parts.insert(f.name().to_string(), s);
        }
    }
    parts.remove("xl/worksheets/sheet1.xml").expect("sheet1.xml")
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

#[test]
fn xls_worksheet_command_keeps_its_saved_value() {
    let (wb, report) = read_xls(&xls(&formula(0, 0, 1.0, &window_restore()))).unwrap();
    let c = wb.sheets[0].cell(at("A1")).expect("A1 is kept");
    assert!(c.formula.is_none(), "no formula: {:?}", c.formula);
    assert_eq!(c.value, Value::Number(1.0));
    assert!(report.warnings.iter().any(|w| w == KEPT), "{:?}", report.warnings);
    let xml = sheet_xml(&write_xlsx(&wb).unwrap());
    assert!(!xml.contains("<f>") && !xml.contains("WINDOW.RESTORE"), "{xml}");
    assert!(xml.contains("<c r=\"A1\"><v>1</v></c>"), "{xml}");
}

#[test]
fn xls_worksheet_dialog_form_command_keeps_its_saved_value() {
    // fPrompt (cparams bit 7) with Cetab 5 (SAVE.AS): the dialog form, still a command.
    let (wb, _) = read_xls(&xls(&formula(0, 0, 0.0, &func_var(0x80, 0x8000 | 0x0005)))).unwrap();
    let c = wb.sheets[0].cell(at("A1")).expect("A1 is kept");
    assert!(c.formula.is_none(), "no formula: {:?}", c.formula);
    let xml = sheet_xml(&write_xlsx(&wb).unwrap());
    assert!(!xml.contains("SAVE.AS"), "{xml}");
}

#[test]
fn xls_worksheet_commands_warn_once_and_ordinary_functions_stay_formulas() {
    let sum = cat(&[&[0x1E], &1u16.to_le_bytes(), &[0x1E], &2u16.to_le_bytes(), &func_var(2, 4)]);
    let records = cat(&[&formula(0, 0, 1.0, &window_restore()), &formula(1, 0, 0.0, &window_restore()), &formula(2, 0, 3.0, &sum)]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.as_str() == KEPT).count(), 1, "{:?}", report.warnings);
    let a3 = wb.sheets[0].cell(at("A3")).expect("A3");
    assert_eq!(a3.formula.as_ref().map(|f| f.text.as_str()), Some("SUM(1,2)"));
    let xml = sheet_xml(&write_xlsx(&wb).unwrap());
    assert!(xml.contains("<f>SUM(1,2)</f>"), "{xml}");
    assert!(!xml.contains("WINDOW.RESTORE"), "{xml}");
}

/// A BrtFmlaNum cell (record 9) in column `col` with its cached number and rgce.
fn fmla_num(col: u32, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(cached.to_le_bytes());
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(9, &data)
}

#[test]
fn xlsb_worksheet_command_keeps_its_saved_value() {
    let mut d = row(0);
    d.extend(fmla_num(0, 1.0, &window_restore()));
    d.extend(row(1));
    d.extend(fmla_num(0, 3.0, &[ptg_int(1), ptg_int(2), func_var(2, 4)].concat()));
    let (wb, _) = read_xlsb(&file(&worksheet(&d, &[]))).unwrap();
    let c = wb.sheets[0].cell(at("A1")).expect("A1 is kept");
    assert!(c.formula.is_none(), "no formula: {:?}", c.formula);
    assert_eq!(c.value, Value::Number(1.0));
    let a2 = wb.sheets[0].cell(at("A2")).expect("A2");
    assert_eq!(a2.formula.as_ref().map(|f| f.text.as_str()), Some("SUM(1,2)"));
    let xml = sheet_xml(&write_xlsx(&wb).unwrap());
    assert!(!xml.contains("WINDOW.RESTORE"), "{xml}");
}
