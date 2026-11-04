//! Defined names that are legal in an Excel 97-2003 workbook (256 columns) but are cell
//! addresses in the 16,384-column grid (`On2`, `Fy2008`, `IW1`). Excel renames them with a
//! leading underscore when it converts the workbook, and formulas keep using the name.

use gridcraft_core::CellRef;

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::read_xls;

/// Lbl (0x0018) with workbook scope (`itab` 0) or local to the one-based sheet `itab`.
fn lbl(name: &str, itab: u16, rgce: &[u8]) -> Vec<u8> {
    rec(0x0018, &cat(&[&u16s(&[0]), &[0, name.len() as u8], &u16s(&[rgce.len() as u16, 0, itab]), &[0; 4], &[0], name.as_bytes(), rgce]))
}

/// PtgRef3d through XTI 0 (Sheet1), absolute row and column.
fn ref3d(r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &0u16.to_le_bytes(), &u16s(&[r, c])])
}

/// PtgName (value class) with a one-based name index.
fn name_ptg(i: u32) -> Vec<u8> {
    cat(&[&[0x43], &i.to_le_bytes()])
}

fn int(n: u16) -> Vec<u8> {
    cat(&[&[0x1E], &n.to_le_bytes()])
}

/// FORMULA record with a cached 0.
fn formula(r: u16, c: u16, rgce: &[u8]) -> Vec<u8> {
    let p = cat(&[&cell(r, c, 0), &0f64.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]);
    rec(0x0006, &p)
}

/// Names: 1 On2 = Sheet1!$A$3, 2 Fy2008 = Sheet1!$A$1, 3 Rate = Sheet1!$A$2, 4 Twice = On2*2,
/// 5 IW1 (local to Sheet1) = Sheet1!$A$1. B1 = SUM(On2), B2 = Fy2008*2, B3 = Rate+1.
fn workbook() -> gridcraft_model::Workbook {
    let twice = cat(&[&name_ptg(1), &int(2), &[0x05]]);
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
            &lbl("On2", 0, &ref3d(2, 0)),
            &lbl("Fy2008", 0, &ref3d(0, 0)),
            &lbl("Rate", 0, &ref3d(1, 0)),
            &lbl("Twice", 0, &twice),
            &lbl("IW1", 1, &ref3d(0, 0)),
            &eof(),
        ])
    };
    let sum = cat(&[&name_ptg(1), &[0x22, 1], &4u16.to_le_bytes()]);
    let times2 = cat(&[&name_ptg(2), &int(2), &[0x05]]);
    let plus1 = cat(&[&name_ptg(3), &int(1), &[0x03]]);
    let sheet = cat(&[
        &bof(0x0010),
        &rec(0x0203, &cat(&[&cell(0, 0, 0), &10.0f64.to_le_bytes()])),
        &formula(0, 1, &sum),
        &formula(1, 1, &times2),
        &formula(2, 1, &plus1),
        &eof(),
    ]);
    let pos = globals(0).len() as u32;
    read_xls(&compound("Workbook", &cat(&[&globals(pos), &sheet]))).unwrap().0
}

fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

#[test]
fn names_that_are_cell_addresses_get_an_underscore() {
    let wb = workbook();
    let names: Vec<_> = wb.names.iter().filter(|n| n.scope.is_none()).map(|n| (n.name.as_str(), n.formula.as_str())).collect();
    assert_eq!(names, [("_On2", "Sheet1!$A$3"), ("_Fy2008", "Sheet1!$A$1"), ("Rate", "Sheet1!$A$2"), ("Twice", "_On2*2")]);
}

#[test]
fn formulas_use_the_renamed_names() {
    let wb = workbook();
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("SUM(_On2)"));
    assert_eq!(ftext(&wb, "B2").as_deref(), Some("_Fy2008*2"));
    assert_eq!(ftext(&wb, "B3").as_deref(), Some("Rate+1"));
}

#[test]
fn sheet_scoped_names_are_renamed_too() {
    let wb = workbook();
    let local: Vec<_> = wb.names.iter().filter(|n| n.scope.is_some()).map(|n| (n.name.as_str(), n.scope, n.formula.as_str())).collect();
    assert_eq!(local, [("_IW1", Some(0), "Sheet1!$A$1")]);
}
