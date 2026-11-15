//! `.xls` sheet default column width: DefColWidth ([MS-XLS] 2.4.89, characters) and DxGCol (2.4.98,
//! 1/256 character, the ColInfo unit; it wins over DefColWidth). Measured on Excel's re-saves of real
//! files: a DefColWidth 10 sheet and a DxGCol 2925 sheet are both re-saved with every unlisted column at
//! ColInfo width 2925, which is 80 px; DefColWidth 8 alone stays at the standard 64 px.

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::read_xls;

/// A BIFF8 FONT record (8-bit name): Arial 10, not bold, automatic colour.
fn font() -> Vec<u8> {
    let head = u16s(&[200, 0, 0x7FFF, 400, 0]);
    rec(0x0031, &cat(&[&head, &[0, 0, 0, 0], &[5, 0], b"Arial"]))
}

/// An XF record naming FONT 0, format 0, the given type/parent word, then 14 zero bytes.
fn xf(kind: u16) -> Vec<u8> {
    rec(0x00E0, &cat(&[&u16s(&[0, 0, kind]), &[0; 14]]))
}

/// A ColInfo record for one column: width in 1/256 character, XF 1, the given flags.
fn colinfo(col: u16, coldx: u16, flags: u16) -> Vec<u8> {
    rec(0x007D, &u16s(&[col, col, coldx, 1, flags, 0]))
}

/// One worksheet "Sheet1" whose substream holds `records` (in order) before a number in A1.
fn book(records: &[Vec<u8>]) -> Vec<u8> {
    let fonts: Vec<u8> = (0..4).flat_map(|_| font()).collect();
    let xfs = cat(&[&xf(0xFFF5), &xf(0x0000)]);
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]));
    let globals = |pos: u32| cat(&[&bof(0x0005), &fonts, &xfs, &boundsheet(pos), &eof()]);
    let mut sheet = bof(0x0010);
    for r in records {
        sheet.extend_from_slice(r);
    }
    sheet.extend_from_slice(&rec(0x0203, &cat(&[&cell(0, 0, 1), &1f64.to_le_bytes()])));
    sheet.extend_from_slice(&eof());
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn def_col_width(chars: u16) -> Vec<u8> {
    rec(0x0055, &u16s(&[chars]))
}

fn dxgcol(w: u16) -> Vec<u8> {
    rec(0x0099, &u16s(&[w]))
}

/// The first sheet's width (px) of column `col` (0-based).
fn width(bytes: &[u8], col: u32) -> f32 {
    let (wb, _) = read_xls(bytes).unwrap();
    wb.sheets[0].col_width(col)
}

fn assert_px(bytes: &[u8], col: u32, want: f32) {
    let got = width(bytes, col);
    assert!((got - want).abs() < 0.01, "column {col}: {got} px, Excel {want} px");
}

#[test]
fn no_records_keep_the_standard_64_px() {
    assert_px(&book(&[]), 2, 64.0);
}

#[test]
fn def_col_width_8_keeps_64_px() {
    assert_px(&book(&[def_col_width(8)]), 2, 64.0);
}

#[test]
fn def_col_width_10_gives_80_px() {
    let b = book(&[def_col_width(10)]);
    assert_px(&b, 0, 80.0);
    assert_px(&b, 5, 80.0);
    assert_px(&b, 255, 80.0);
}

#[test]
fn dxgcol_2925_gives_80_px() {
    assert_px(&book(&[dxgcol(2925)]), 3, 80.0);
}

#[test]
fn dxgcol_wins_over_def_col_width() {
    // Excel's re-save of DefColWidth 8 + DxGCol 2925 lists unlisted columns at 2925 (80 px).
    assert_px(&book(&[def_col_width(8), dxgcol(2925)]), 3, 80.0);
    // ... and a narrower DxGCol wins over a wider DefColWidth: 2340 -> 64 px.
    assert_px(&book(&[def_col_width(10), dxgcol(2340)]), 3, 64.0);
}

#[test]
fn dxgcol_zero_is_ignored() {
    assert_px(&book(&[def_col_width(10), dxgcol(0)]), 3, 80.0);
}

#[test]
fn colinfo_before_a_later_dxgcol_keeps_its_own_width() {
    // ColInfo B = 2340 (64 px), not user-set, read before DxGCol 2925: B stays 64 px, C gets 80 px.
    let b = book(&[def_col_width(8), colinfo(1, 2340, 0), dxgcol(2925)]);
    assert_px(&b, 1, 64.0);
    assert_px(&b, 2, 80.0);
}

#[test]
fn user_set_colinfo_keeps_its_width_on_a_wide_default() {
    // ColInfo D = 0x11B6 (4534/256 = 17.71 ch -> 124 px), fUserSet; the rest follow DefColWidth 10.
    let b = book(&[def_col_width(10), colinfo(3, 0x11B6, 0x0002)]);
    assert_px(&b, 3, 124.0);
    assert_px(&b, 4, 80.0);
}
