//! `.xls` formulas that name the workbook's own defined names through PtgNameX ([MS-XLS]
//! 2.5.198.77) whose XTI points to the self-referencing SupBook (cch 0x0401) with no ExternName
//! records after it: `nameindex` is then the 1-based Lbl (NAME) index, as for PtgName. Excel
//! calculates these (measured: BOB + JIM through PtgNameX gives 30, like the PtgName form), so
//! the formulas keep their text instead of being frozen to their saved values.
use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

const MISSING: &str = "missing name";

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

/// Lbl (NAME) record with an 8-bit name: flags 0, chKey 0, cch, cce, reserved, itab (0 = workbook,
/// else the 1-based sheet), four zero text lengths, the string flags byte, the name, rgce.
fn lbl(name: &str, itab: u16, rgce: &[u8]) -> Vec<u8> {
    let head = cat(&[&u16s(&[0]), &[0, name.len() as u8], &u16s(&[rgce.len() as u16, 0, itab]), &[0; 4], &[0]]);
    rec(0x0018, &cat(&[&head, name.as_bytes(), rgce]))
}

/// PtgRef3d (reference class), absolute row and column, through XTI 0 (Sheet1).
fn ref3d(r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &u16s(&[0, r, c])])
}

/// PtgNameX, value class: XTI index and 1-based name index.
fn namex(ixti: u16, index: u32) -> Vec<u8> {
    cat(&[&[0x59], &ixti.to_le_bytes(), &index.to_le_bytes()])
}

/// PtgName, value class: 1-based Lbl index.
fn name(index: u32) -> Vec<u8> {
    cat(&[&[0x43], &index.to_le_bytes()])
}

const ADD: u8 = 0x03;

fn number(r: u16, c: u16, v: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&cell(r, c, 0), &v.to_le_bytes()]))
}

fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// Globals: Sheet1, the self SupBook (no ExternName after it), XTI 0 = Sheet1, XTI 1 = this
/// workbook at workbook level, names 1 BOB = Sheet1!$A$1, 2 JIM = Sheet1!$B$1, 3 LOCAL (sheet-level
/// on Sheet1) = Sheet1!$A$1. Sheet1: A1 10, B1 20, then `cells`.
fn file(cells: &[u8]) -> Vec<u8> {
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &rec(0x0017, &cat(&[&u16s(&[2]), &u16s(&[0, 0, 0]), &u16s(&[0, 0xFFFE, 0xFFFE])])),
            &lbl("BOB", 0, &ref3d(0, 0)),
            &lbl("JIM", 0, &ref3d(0, 1)),
            &lbl("LOCAL", 1, &ref3d(0, 0)),
            &eof(),
        ])
    };
    let sheet = cat(&[&bof(0x0010), &number(0, 0, 10.0), &number(0, 1, 20.0), cells, &eof()]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

/// C1 = BOB+JIM with PtgName, D1 = the same with PtgNameX through the own book, E1 = LOCAL
/// (sheet-level) through PtgNameX, F1 = JIM*2 through PtgNameX.
fn cells() -> Vec<u8> {
    cat(&[
        &formula(0, 2, 30.0, &cat(&[&name(1), &name(2), &[ADD]])),
        &formula(0, 3, 30.0, &cat(&[&namex(1, 1), &namex(1, 2), &[ADD]])),
        &formula(0, 4, 10.0, &namex(1, 3)),
        &formula(0, 5, 40.0, &cat(&[&namex(1, 2), &[0x1E, 2, 0], &[0x05]])),
    ])
}

#[test]
fn own_book_name_x_reads_as_the_workbooks_own_names() {
    let (wb, report) = read_xls(&file(&cells())).unwrap();
    assert_eq!(ftext(&wb, "C1").as_deref(), Some("BOB+JIM"), "PtgName control: {:?}", report.warnings);
    assert_eq!(ftext(&wb, "D1").as_deref(), Some("BOB+JIM"), "{:?}", report.warnings);
    assert_eq!(ftext(&wb, "E1").as_deref(), Some("LOCAL"), "a sheet-level name: {:?}", report.warnings);
    assert_eq!(ftext(&wb, "F1").as_deref(), Some("JIM*2"), "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains(MISSING)), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("D1")), Value::Number(30.0));
    assert_eq!(wb.sheets[0].value(at("F1")), Value::Number(40.0));
}

#[test]
fn own_book_name_x_formulas_survive_save_as_xlsx() {
    let (wb, _) = read_xls(&file(&cells())).unwrap();
    let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(ftext(&back, "D1").as_deref(), Some("BOB+JIM"), "{:?}", report.warnings);
    assert_eq!(ftext(&back, "E1").as_deref(), Some("LOCAL"), "{:?}", report.warnings);
    assert_eq!(ftext(&back, "F1").as_deref(), Some("JIM*2"), "{:?}", report.warnings);
}

#[test]
fn an_index_past_the_last_name_still_keeps_the_saved_value() {
    let (wb, report) = read_xls(&file(&formula(0, 6, 7.0, &namex(1, 9)))).unwrap();
    assert_eq!(ftext(&wb, "G1"), None, "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("G1")), Value::Number(7.0));
    assert!(report.warnings.iter().any(|w| w.contains(MISSING)), "{:?}", report.warnings);
}
