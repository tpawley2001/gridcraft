//! `.xls` sheet selections (SELECTION) and scroll positions (WINDOW2): each sheet opens on its
//! saved active cell and top-left cell. The files are assembled from the [MS-CFB] and [MS-XLS]
//! structures.
use gridcraft_core::CellRef;
use gridcraft_model::Sheet;

use super::xls_tests::{cat, one_sheet_with, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

/// SELECTION ([MS-XLS] 2.4.248): pnn, rwAct, colAct, irefAct, cref and one RefU (2.5.207:
/// rwFirst, rwLast, colFirst and colLast as bytes, 6 bytes) per range.
fn selection(pnn: u8, row: u16, col: u16) -> Vec<u8> {
    let mut out = vec![pnn];
    out.extend(u16s(&[row, col, 0, 1]));
    out.extend(u16s(&[row, row]));
    out.extend([col as u8, col as u8]);
    rec(0x001D, &out)
}

/// A SELECTION record whose payload is `payload` bytes long, cut short on purpose.
fn short_selection(payload: &[u8]) -> Vec<u8> {
    rec(0x001D, payload)
}

/// WINDOW2 ([MS-XLS] 2.4.346): flags, rwTop, colLeft, then the remaining fields.
fn window2(flags: u16, top: u16, left: u16) -> Vec<u8> {
    rec(0x023E, &u16s(&[flags, top, left, 64, 0, 0, 0, 0, 0]))
}

fn open(records: &[u8]) -> Sheet {
    let (wb, _) = read_xls(&one_sheet_with(records)).expect("xls opens");
    wb.sheets[0].as_ref().clone()
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).expect("cell reference")
}

/// Saves the sheet as XLSX and reads it back: the view survives the round trip.
fn roundtrip(s: &Sheet) -> Sheet {
    let mut wb = gridcraft_model::Workbook::new();
    wb.sheets.clear();
    let mut sheet = s.clone();
    sheet.name = "Data".into();
    wb.sheets.push(Box::new(sheet).into());
    let (back, _) = read_xlsx(&write_xlsx(&wb).expect("xlsx saves")).expect("xlsx opens");
    back.sheets.first().map(|s| s.as_ref().clone()).expect("one sheet")
}

#[test]
fn xls_selection_sets_the_active_cell() {
    let s = open(&selection(3, 6, 1));
    assert_eq!(s.view_active, at("B7"));
    assert_eq!(roundtrip(&s).view_active, at("B7"));
}

#[test]
fn xls_window2_sets_the_scroll_position() {
    let s = open(&window2(0x0002, 20, 3));
    assert_eq!(s.view_top_left, at("D21"));
    assert_eq!(roundtrip(&s).view_top_left, at("D21"));
}

#[test]
fn xls_truncated_selection_is_ignored() {
    // pnn, rwAct and half of colAct: the record ends inside its fixed fields.
    let s = open(&short_selection(&[3, 6, 0, 1, 0]));
    assert_eq!(s.view_active, at("A1"));
    assert_eq!(roundtrip(&s).view_active, at("A1"));
}

#[test]
fn xls_selection_of_the_top_left_pane_wins() {
    let s = open(&cat(&[&selection(2, 9, 3), &selection(3, 6, 1)]));
    assert_eq!(s.view_active, at("B7"));
    assert_eq!(roundtrip(&s).view_active, at("B7"));
}
