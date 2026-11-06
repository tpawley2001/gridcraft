//! Defined names in Excel 97-2003 workbooks that aren't valid names in an `.xlsx` workbook
//! (a space, a leading `?` or digit, a `-`). Written as-is into `<definedName>`, Excel refuses
//! the saved file until it repairs it, so they are renamed on open: invalid characters become
//! `_`, a bad first character gets a `_` in front, and a clash with another name of the same
//! scope gets more `_` in front. Valid names never move; formulas keep using the name.

use gridcraft_core::CellRef;

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::read_xls;

/// Lbl (0x0018) with workbook scope, 8-bit name text.
fn lbl(name: &str, rgce: &[u8]) -> Vec<u8> {
    rec(0x0018, &cat(&[&u16s(&[0]), &[0, name.len() as u8], &u16s(&[rgce.len() as u16, 0, 0]), &[0; 4], &[0], name.as_bytes(), rgce]))
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

/// The names in file order (one-based index), each a reference into Sheet1.
const NAMES: [&str; 9] = ["Rate", "Titles 3 1", "?", "1st", "A.B-C", "Total 2", "Total_2", "\\a", "On2"];

/// B1 = `Titles 3 1`+1, B2 = `Total 2`*2, B3 = Total_2.
fn workbook() -> (gridcraft_model::Workbook, Vec<String>) {
    let globals = |pos: u32| {
        let mut parts = vec![
            bof(0x0005),
            rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            rec(0x01AE, &u16s(&[1, 0x0401])),
            rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
        ];
        for (i, n) in NAMES.iter().enumerate() {
            parts.push(lbl(n, &ref3d(i as u16, 0)));
        }
        parts.push(eof());
        parts.concat()
    };
    let sheet = cat(&[
        &bof(0x0010),
        &rec(0x0203, &cat(&[&cell(0, 0, 0), &10.0f64.to_le_bytes()])),
        &formula(0, 1, &cat(&[&name_ptg(2), &int(1), &[0x03]])),
        &formula(1, 1, &cat(&[&name_ptg(6), &int(2), &[0x05]])),
        &formula(2, 1, &name_ptg(7)),
        &eof(),
    ]);
    let pos = globals(0).len() as u32;
    let (wb, report) = read_xls(&compound("Workbook", &cat(&[&globals(pos), &sheet]))).unwrap();
    let warnings = report.warnings;
    (wb, warnings)
}

fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

fn name_for(wb: &gridcraft_model::Workbook, formula: &str) -> Option<String> {
    wb.names.iter().find(|n| n.scope.is_none() && n.formula == formula).map(|n| n.name.clone())
}

#[test]
fn invalid_names_get_valid_ones() {
    let (wb, _) = workbook();
    let want = [
        ("Sheet1!$A$1", "Rate"),
        ("Sheet1!$A$2", "Titles_3_1"),
        ("Sheet1!$A$3", "_?"),
        ("Sheet1!$A$4", "_1st"),
        ("Sheet1!$A$5", "A.B_C"),
        ("Sheet1!$A$6", "_Total_2"),
        ("Sheet1!$A$7", "Total_2"),
        ("Sheet1!$A$8", "\\a"),
        ("Sheet1!$A$9", "_On2"),
    ];
    for (formula, name) in want {
        assert_eq!(name_for(&wb, formula).as_deref(), Some(name), "name for {formula}");
    }
}

#[test]
fn formulas_use_the_renamed_names() {
    let (wb, _) = workbook();
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("Titles_3_1+1"));
    assert_eq!(ftext(&wb, "B2").as_deref(), Some("_Total_2*2"));
    assert_eq!(ftext(&wb, "B3").as_deref(), Some("Total_2"));
}

#[test]
fn every_name_is_unique_and_saves_without_spaces() {
    let (wb, _) = workbook();
    let mut seen = std::collections::HashSet::new();
    for n in wb.names.iter().filter(|n| n.scope.is_none()) {
        assert!(seen.insert(n.name.to_ascii_lowercase()), "duplicate name {}", n.name);
        assert!(!n.name.contains(' ') && !n.name.contains('-'), "invalid name {}", n.name);
    }
    assert_eq!(seen.len(), NAMES.len());
}

#[test]
fn renaming_is_reported_once() {
    let (_, warnings) = workbook();
    let renamed = warnings.iter().filter(|w| w.contains("renamed")).count();
    assert_eq!(renamed, 1, "{warnings:?}");
}
