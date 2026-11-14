//! `.xls` print defaults Excel applies when the file does not state them ([MS-XLS] 2.4):
//! a worksheet substream with no LeftMargin / RightMargin / TopMargin / BottomMargin record has
//! margins of 0.75 in left and right and 1.0 in top and bottom (Excel writes exactly these when
//! it re-saves such a file), and a DefaultRowHeight record (2.4.90) whose fUnsynced bit is clear
//! does not set the default row height: Excel recomputes it from the Normal font (Arial 10:
//! 255 twips = 12.75 pt) and drops the stored `miyRw`.

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::read_xls;

/// A BIFF8 FONT record: height in twips, not bold, automatic colour, then the 8-bit name.
fn font(twips: u16, name: &str) -> Vec<u8> {
    rec(0x0031, &cat(&[&u16s(&[twips, 0, 0x7FFF, 400, 0]), &[0, 0, 0, 0], &[name.len() as u8, 0], name.as_bytes()]))
}

/// One worksheet "Sheet1" whose Normal font (FONT 0, used by XF 0) is Arial 10, with the given
/// records at the start of the sheet substream and the number 1 in A1.
fn book(sheet_records: &[Vec<u8>]) -> Vec<u8> {
    let xf = rec(0x00E0, &cat(&[&u16s(&[0, 0, 0]), &[0; 14]]));
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]));
    let globals = |pos: u32| cat(&[&bof(0x0005), &font(200, "Arial"), &xf, &boundsheet(pos), &eof()]);
    let mut sheet = bof(0x0010);
    for r in sheet_records {
        sheet.extend_from_slice(r);
    }
    sheet.extend_from_slice(&rec(0x0203, &cat(&[&cell(0, 0, 0), &1f64.to_le_bytes()])));
    sheet.extend_from_slice(&eof());
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

/// DefaultRowHeight: flags (bit 0 fUnsynced), then miyRw in twips.
fn default_row_height(flags: u16, twips: u16) -> Vec<u8> {
    rec(0x0225, &u16s(&[flags, twips]))
}

/// The four page margins in inches: left, right, top, bottom.
fn margins(bytes: &[u8]) -> [f32; 4] {
    let (wb, _) = read_xls(bytes).unwrap();
    let m = wb.sheets[0].print.margins;
    [m[0], m[1], m[2], m[3]]
}

/// The sheet's default row height in points.
fn default_height_pt(bytes: &[u8]) -> f32 {
    let (wb, _) = read_xls(bytes).unwrap();
    wb.sheets[0].default_row_height * 0.75
}

#[test]
fn a_sheet_without_margin_records_has_excels_old_default_margins() {
    assert_eq!(margins(&book(&[])), [0.75, 0.75, 1.0, 1.0]);
}

#[test]
fn a_stated_margin_is_kept_and_the_others_get_the_defaults() {
    // TopMargin (0x0028) 0.5 in; BottomMargin (0x0029) 0.25 in.
    let top = rec(0x0028, &0.5f64.to_le_bytes());
    assert_eq!(margins(&book(std::slice::from_ref(&top))), [0.75, 0.75, 0.5, 1.0]);
    let bottom = rec(0x0029, &0.25f64.to_le_bytes());
    assert_eq!(margins(&book(&[top, bottom])), [0.75, 0.75, 0.5, 0.25]);
    // All four stated: nothing is defaulted (0.7 / 0.75 are what an .xlsx-born file carries).
    let all: Vec<Vec<u8>> =
        [(0x0026u16, 0.7f64), (0x0027, 0.7), (0x0028, 0.75), (0x0029, 0.75)].iter().map(|&(id, v)| rec(id, &v.to_le_bytes())).collect();
    assert_eq!(margins(&book(&all)), [0.7, 0.7, 0.75, 0.75]);
}

#[test]
fn a_synced_default_row_height_is_recomputed_from_the_normal_font() {
    // fUnsynced clear: Excel ignores the stored 1026 twips (51.3 pt); Arial 10 gives 12.75 pt.
    let h = default_height_pt(&book(&[default_row_height(0, 1026)]));
    assert!((h - 12.75).abs() < 0.01, "default row height {h} pt, Excel 12.75");
    // The usual stored value for Arial 10 stays what it is.
    let h = default_height_pt(&book(&[default_row_height(0, 255)]));
    assert!((h - 12.75).abs() < 0.01, "default row height {h} pt, Excel 12.75");
}

#[test]
fn an_unsynced_default_row_height_is_the_stored_one() {
    // fUnsynced set: the user changed the default row height; 1026 twips = 51.3 pt.
    let h = default_height_pt(&book(&[default_row_height(1, 1026)]));
    assert!((h - 51.3).abs() < 0.01, "default row height {h} pt, stored 51.3");
}

/// The Normal font's default row height covers the newer measured fonts and the localized names.
#[test]
fn normal_font_row_twips_knows_the_newer_fonts_and_localized_names() {
    use crate::xls::normal_font_row_twips;
    assert_eq!(normal_font_row_twips("新細明體", 12.0), Some(300));
    assert_eq!(normal_font_row_twips("宋体", 11.0), Some(270));
    assert_eq!(normal_font_row_twips("Trebuchet MS", 11.0), Some(330));
    assert_eq!(normal_font_row_twips("Calibri", 14.0), Some(375));
    assert_eq!(normal_font_row_twips("Some Unknown Font", 11.0), None);
}
