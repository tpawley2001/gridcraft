//! What-If data tables in Excel 97-2003 files: the BIFF8 TABLE record ([MS-XLS] 2.4.319) is read
//! into `Sheet::data_tables`, its result cells (FORMULA records holding a single PtgTbl) keep their
//! cached values without a formula, and the table survives a save as `.xlsx`.

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{DataTable, Workbook};

use super::xls_tests::{cat, cell, one_sheet_with, ptg_ref, rec, u16s, v};
use crate::package::Package;
use crate::{read_xls, read_xlsx, write_xlsx};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rr(a: &str) -> RangeRef {
    RangeRef::parse(a).unwrap()
}

/// A FORMULA record with a numeric cached result.
fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut p = cell(r, c, 0);
    p.extend_from_slice(&cached.to_le_bytes());
    p.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // grbit, chn
    p.extend_from_slice(&(rgce.len() as u16).to_le_bytes());
    p.extend_from_slice(rgce);
    rec(0x0006, &p)
}

/// PtgTbl: the cell is part of the data table whose top-left result cell is (r, c).
fn ptg_tbl(r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x02], &u16s(&[r, c])])
}

fn number(r: u16, c: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&cell(r, c, 0), &n.to_le_bytes()]))
}

/// TABLE record: result range, flags, then (row, col) of the first and second input cells.
fn table(rows: (u16, u16), cols: (u8, u8), flags: u16, inp1: (u16, u16), inp2: (u16, u16)) -> Vec<u8> {
    rec(0x0236, &cat(&[&u16s(&[rows.0, rows.1]), &[cols.0, cols.1], &u16s(&[flags, inp1.0, inp1.1, inp2.0, inp2.1])]))
}

const F_ALWAYS_CALC: u16 = 0x0001;
const F_RW: u16 = 0x0004;
const F_TBL2: u16 = 0x0008;
const F_DELETED1: u16 = 0x0010;
const F_DELETED2: u16 = 0x0020;

/// One-variable column table: input cell A1 (7 from `one_sheet_with`), inputs A3:A5 = 1, 2, 3,
/// formula B2 = A1*2, results B3:B5 = 2, 4, 6.
fn column_table(flags: u16) -> Vec<u8> {
    let b2 = cat(&[&ptg_ref(0, 0), &[0x1E], &2u16.to_le_bytes(), &[0x05]]);
    cat(&[
        &number(2, 0, 1.0),
        &number(3, 0, 2.0),
        &number(4, 0, 3.0),
        &formula(1, 1, 14.0, &b2),
        &formula(2, 1, 2.0, &ptg_tbl(2, 1)),
        &table((2, 4), (1, 1), flags, (0, 0), (0, 0)),
        &formula(3, 1, 4.0, &ptg_tbl(2, 1)),
        &formula(4, 1, 6.0, &ptg_tbl(2, 1)),
    ])
}

fn read(sheet_records: &[u8]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xls(&one_sheet_with(sheet_records)).unwrap();
    (wb, report.warnings.clone())
}

fn data_table_warnings(warnings: &[String]) -> usize {
    warnings.iter().filter(|w| w.to_lowercase().contains("data table")).count()
}

fn sheet_xml(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
}

#[test]
fn one_variable_column_table_is_read_and_values_kept() {
    let (wb, warnings) = read(&column_table(0));
    assert_eq!(
        wb.sheets[0].data_tables,
        vec![DataTable { range: rr("B3:B5"), two_d: false, row: false, r1: Some(at("A1")), r2: None, del1: false, del2: false, ca: false }]
    );
    assert_eq!(data_table_warnings(&warnings), 0, "{warnings:?}");
    for (a1, n) in [("B3", 2.0), ("B4", 4.0), ("B5", 6.0)] {
        assert_eq!(v(&wb, 0, a1), Value::Number(n), "{a1}");
        assert!(wb.sheets[0].cell(at(a1)).is_some_and(|c| c.formula.is_none()), "{a1} has no formula");
    }
}

#[test]
fn one_variable_row_table_with_always_calc() {
    // Inputs C1:E1, formula B2 = A1*2, results C2:E2; input cell A1.
    let b2 = cat(&[&ptg_ref(0, 0), &[0x1E], &2u16.to_le_bytes(), &[0x05]]);
    let records = cat(&[
        &number(0, 2, 1.0),
        &number(0, 3, 2.0),
        &number(0, 4, 3.0),
        &formula(1, 1, 14.0, &b2),
        &formula(1, 2, 2.0, &ptg_tbl(1, 2)),
        &table((1, 1), (2, 4), F_RW | F_ALWAYS_CALC, (0, 0), (0, 0)),
        &formula(1, 3, 4.0, &ptg_tbl(1, 2)),
        &formula(1, 4, 6.0, &ptg_tbl(1, 2)),
    ]);
    let (wb, warnings) = read(&records);
    assert_eq!(
        wb.sheets[0].data_tables,
        vec![DataTable { range: rr("C2:E2"), two_d: false, row: true, r1: Some(at("A1")), r2: None, del1: false, del2: false, ca: true }]
    );
    assert_eq!(data_table_warnings(&warnings), 0, "{warnings:?}");
    assert_eq!(v(&wb, 0, "E2"), Value::Number(6.0));
}

#[test]
fn two_variable_table_reads_both_input_cells() {
    // Row input cell A1, column input cell C1 (5). Formula B3 = A1*C1, row inputs C3:D3, column
    // inputs B4:B5, results C4:D5.
    let b3 = cat(&[&ptg_ref(0, 0), &ptg_ref(0, 2), &[0x05]]);
    let records = cat(&[
        &number(0, 2, 5.0),
        &formula(2, 1, 35.0, &b3),
        &number(2, 2, 1.0),
        &number(2, 3, 2.0),
        &number(3, 1, 10.0),
        &number(4, 1, 20.0),
        &formula(3, 2, 10.0, &ptg_tbl(3, 2)),
        &table((3, 4), (2, 3), F_TBL2 | F_RW, (0, 0), (0, 2)),
        &formula(3, 3, 20.0, &ptg_tbl(3, 2)),
        &formula(4, 2, 20.0, &ptg_tbl(3, 2)),
        &formula(4, 3, 40.0, &ptg_tbl(3, 2)),
    ]);
    let (wb, warnings) = read(&records);
    assert_eq!(
        wb.sheets[0].data_tables,
        vec![DataTable { range: rr("C4:D5"), two_d: true, row: true, r1: Some(at("A1")), r2: Some(at("C1")), del1: false, del2: false, ca: false }]
    );
    assert_eq!(data_table_warnings(&warnings), 0, "{warnings:?}");
    assert_eq!(v(&wb, 0, "D5"), Value::Number(40.0));
}

#[test]
fn deleted_input_flags_are_kept() {
    let (wb, _) = read(&column_table(F_TBL2 | F_DELETED1 | F_DELETED2));
    let t = &wb.sheets[0].data_tables[0];
    assert!(t.two_d && t.del1 && t.del2, "{t:?}");
}

#[test]
fn data_table_survives_saving_as_xlsx() {
    let (wb, _) = read(&column_table(0));
    let xml = sheet_xml(&wb);
    assert!(xml.contains(r#"t="dataTable""#) && xml.contains(r#"ref="B3:B5""#) && xml.contains(r#"r1="A1""#), "{xml}");
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].data_tables, wb.sheets[0].data_tables);
    assert_eq!(back.sheets[0].value(at("B4")), Value::Number(4.0));
}

#[test]
fn truncated_or_out_of_range_records_are_skipped_with_one_warning() {
    let short = rec(0x0236, &u16s(&[2, 4, 0x0101]));
    let reversed = table((4, 2), (1, 1), 0, (0, 0), (0, 0));
    let wide = table((2, 4), (1, 1), 0, (0, 300), (0, 0));
    let records = cat(&[&formula(2, 1, 2.0, &ptg_tbl(2, 1)), &short, &reversed, &wide]);
    let (wb, warnings) = read(&records);
    assert!(wb.sheets[0].data_tables.is_empty());
    assert_eq!(data_table_warnings(&warnings), 1, "{warnings:?}");
    assert_eq!(v(&wb, 0, "B3"), Value::Number(2.0));
}

#[test]
fn duplicate_table_records_keep_the_first() {
    let records = cat(&[&column_table(0), &table((2, 4), (1, 1), F_ALWAYS_CALC, (0, 0), (0, 0))]);
    let (wb, _) = read(&records);
    assert_eq!(wb.sheets[0].data_tables.len(), 1);
    assert!(!wb.sheets[0].data_tables[0].ca);
}
