//! `.xls` Normal font: Excel takes the workbook's Normal font from FONT record 0 ([MS-XLS] 2.4.122),
//! even when XF 0 (the XF the built-in Normal STYLE names) points at another FONT record. Measured on
//! files re-saved by Excel: the re-save rewrites XF 0's font index to 0, and a DefaultRowHeight whose
//! fUnsynced bit is clear becomes FONT 0's default row height (SimSun 12 -> 14.25 pt, Arial 12 -> 15 pt).

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::read_xls;
use gridcraft_core::CellRef;
use gridcraft_model::StyleId;

/// A BIFF8 FONT record: height in twips, not bold, automatic colour, then the name
/// (8-bit when it is ASCII, else UTF-16LE with fHighByte set).
fn font(twips: u16, name: &str) -> Vec<u8> {
    let head = u16s(&[twips, 0, 0x7FFF, 400, 0]);
    if name.is_ascii() {
        rec(0x0031, &cat(&[&head, &[0, 0, 0, 0], &[name.len() as u8, 0], name.as_bytes()]))
    } else {
        let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        rec(0x0031, &cat(&[&head, &[0, 0, 0, 0], &[name.encode_utf16().count() as u8, 1], &wide]))
    }
}

/// An XF record: font index, format 0, then the type/protection/parent word and 14 zero bytes.
fn xf(ifnt: u16, kind: u16) -> Vec<u8> {
    rec(0x00E0, &cat(&[&u16s(&[ifnt, 0, kind]), &[0; 14]]))
}

/// STYLE for the built-in Normal style (istyBuiltIn 0) on XF `ixf`.
fn normal_style(ixf: u16) -> Vec<u8> {
    rec(0x0293, &cat(&[&u16s(&[ixf | 0x8000]), &[0, 0xFF]]))
}

/// One worksheet "Sheet1" with the given FONT records (in file order; FONT index 4 is skipped by
/// the format, so the fifth record is font 5), XF 0 naming `xf0_font`, a cell XF 1 naming font 0,
/// a cell XF 2 naming `cell_font`, an optional Normal STYLE record, a synced DefaultRowHeight of 255
/// twips, the number 1 in A1 (XF 1) and 2 in B1 (XF 2).
fn book(fonts: &[(u16, &str)], xf0_font: u16, cell_font: u16, with_style: bool) -> Vec<u8> {
    let font_recs: Vec<u8> = fonts.iter().flat_map(|&(tw, n)| font(tw, n)).collect();
    let mut styles = cat(&[&xf(xf0_font, 0xFFF5), &xf(0, 0x0000), &xf(cell_font, 0x0000)]);
    if with_style {
        styles.extend_from_slice(&normal_style(0));
    }
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]));
    let globals = |pos: u32| cat(&[&bof(0x0005), &font_recs, &styles, &boundsheet(pos), &eof()]);
    let mut sheet = bof(0x0010);
    sheet.extend_from_slice(&rec(0x0225, &u16s(&[0, 255])));
    sheet.extend_from_slice(&rec(0x0203, &cat(&[&cell(0, 0, 1), &1f64.to_le_bytes()])));
    sheet.extend_from_slice(&rec(0x0203, &cat(&[&cell(0, 1, 2), &2f64.to_le_bytes()])));
    sheet.extend_from_slice(&eof());
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

/// (Normal font name, size, sheet default row height in points).
fn normal(bytes: &[u8]) -> (String, f32, f32) {
    let (wb, _) = read_xls(bytes).unwrap();
    let f = &wb.styles.get(StyleId::DEFAULT).font;
    (f.name.clone(), f.size, wb.sheets[0].default_row_height * 0.75)
}

fn assert_normal(bytes: &[u8], name: &str, size: f32, height_pt: f32) {
    let (n, s, h) = normal(bytes);
    assert_eq!(n, name, "Normal font name");
    assert!((s - size).abs() < 0.01, "Normal font size {s}, Excel {size}");
    assert!((h - height_pt).abs() < 0.01, "default row height {h} pt, Excel {height_pt}");
}

/// FONT 0..3 = `first` then three Arial 10s, then font 5 = Helv 10, font 6 = Arial 9, font 7 = Arial 10.
fn fonts_with(first: (u16, &'static str)) -> Vec<(u16, &'static str)> {
    vec![first, (200, "Arial"), (200, "Arial"), (200, "Arial"), (200, "Helv"), (180, "Arial"), (200, "Arial")]
}

#[test]
fn xf0_naming_another_font_still_gives_font_0_as_normal() {
    // XF 0 -> font 5 (Helv 10); FONT 0 = Arial 12: Excel's Normal is Arial 12, rows 300 twips.
    assert_normal(&book(&fonts_with((240, "Arial")), 5, 0, true), "Arial", 12.0, 15.0);
}

#[test]
fn east_asian_font_0_is_normal_when_xf0_names_arial() {
    // XF 0 -> font 7 (Arial 10); FONT 0 = 宋体 12: Excel's Normal is 宋体 12, rows 285 twips.
    assert_normal(&book(&fonts_with((240, "宋体")), 7, 0, true), "宋体", 12.0, 14.25);
}

#[test]
fn without_a_style_record_font_0_is_normal() {
    assert_normal(&book(&fonts_with((240, "Arial")), 5, 0, false), "Arial", 12.0, 15.0);
}

#[test]
fn an_ordinary_file_keeps_arial_10() {
    // Control: XF 0 -> font 0 = Arial 10, the usual case.
    assert_normal(&book(&fonts_with((200, "Arial")), 0, 0, true), "Arial", 10.0, 12.75);
}

#[test]
fn a_cell_xf_keeps_its_own_font() {
    // B1's XF names font 5 (Helv 10); that cell still shows Helv 10 while Normal is Arial 12.
    let (wb, _) = read_xls(&book(&fonts_with((240, "Arial")), 5, 5, true)).unwrap();
    let font_at = |a1: &str| wb.styles.get(wb.sheets[0].style_id(CellRef::parse(a1).unwrap())).font.clone();
    let f = font_at("B1");
    assert_eq!((f.name.as_str(), f.size), ("Helv", 10.0));
    let f = font_at("A1");
    assert_eq!((f.name.as_str(), f.size), ("Arial", 12.0));
}
