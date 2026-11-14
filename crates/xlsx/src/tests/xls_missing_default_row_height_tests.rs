//! `.xls` worksheets without a DefaultRowHeight record ([MS-XLS] 2.4.90): Excel gives them the Normal
//! font's default row height, as for a record with fUnsynced clear. Measured on Excel's re-saves of files
//! without the record: Normal Arial 10 -> 255 twips, Arial 8 -> 225, Arial 12 -> 300.

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::read_xls;

/// A BIFF8 FONT record (8-bit name): height in twips, not bold, automatic colour.
fn font(twips: u16, name: &str) -> Vec<u8> {
    let head = u16s(&[twips, 0, 0x7FFF, 400, 0]);
    rec(0x0031, &cat(&[&head, &[0, 0, 0, 0], &[name.len() as u8, 0], name.as_bytes()]))
}

/// An XF record naming FONT `ifnt`, format 0, the given type/parent word, then 14 zero bytes.
fn xf(ifnt: u16, kind: u16) -> Vec<u8> {
    rec(0x00E0, &cat(&[&u16s(&[ifnt, 0, kind]), &[0; 14]]))
}

/// One worksheet "Sheet1": four FONT records all equal to `normal` (FONT 0 is the Normal font), a style
/// XF 0 and a cell XF 1 on font 0, an optional DefaultRowHeight record (flags, miyRw), and 1 in A1.
fn book(normal: (u16, &str), default_row: Option<(u16, u16)>) -> Vec<u8> {
    let fonts: Vec<u8> = (0..4).flat_map(|_| font(normal.0, normal.1)).collect();
    let xfs = cat(&[&xf(0, 0xFFF5), &xf(0, 0x0000)]);
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]));
    let globals = |pos: u32| cat(&[&bof(0x0005), &fonts, &xfs, &boundsheet(pos), &eof()]);
    let mut sheet = bof(0x0010);
    if let Some((flags, miy)) = default_row {
        sheet.extend_from_slice(&rec(0x0225, &u16s(&[flags, miy])));
    }
    sheet.extend_from_slice(&rec(0x0203, &cat(&[&cell(0, 0, 1), &1f64.to_le_bytes()])));
    sheet.extend_from_slice(&eof());
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

/// The first sheet's default row height in points.
fn default_pt(bytes: &[u8]) -> f32 {
    let (wb, _) = read_xls(bytes).unwrap();
    wb.sheets[0].default_row_height * 0.75
}

fn assert_pt(bytes: &[u8], want: f32) {
    let got = default_pt(bytes);
    assert!((got - want).abs() < 0.01, "default row height {got} pt, Excel {want} pt");
}

#[test]
fn no_record_arial_10_gives_12_75_pt() {
    assert_pt(&book((200, "Arial"), None), 12.75);
}

#[test]
fn no_record_arial_8_gives_11_25_pt() {
    assert_pt(&book((160, "Arial"), None), 11.25);
}

#[test]
fn no_record_arial_12_gives_15_pt() {
    assert_pt(&book((240, "Arial"), None), 15.0);
}

#[test]
fn no_record_font_outside_the_table_keeps_the_model_default() {
    // A font with no measured height: unchanged from today (the model default, 15 pt).
    let want = gridcraft_model::DEFAULT_ROW_HEIGHT * 0.75;
    assert_pt(&book((200, "Some Unknown Face"), None), want);
}

#[test]
fn synced_record_still_follows_the_normal_font() {
    // fUnsynced clear: Excel ignores miyRw (300) and uses Arial 10's 255 twips (today's behaviour).
    assert_pt(&book((200, "Arial"), Some((0, 300))), 12.75);
}

#[test]
fn unsynced_record_keeps_its_stored_height() {
    // fUnsynced set: miyRw 360 twips = 18 pt is kept (today's behaviour).
    assert_pt(&book((200, "Arial"), Some((1, 360))), 18.0);
}
