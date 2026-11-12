//! Legacy `.xls` (BIFF8) Font records whose family byte is outside what a workbook can hold.
//! [MS-XLS] 2.4.122 limits `bFamily` to 0..5, but old writers stored the Windows LOGFONT
//! pitch-and-family byte there (0x20). Excel reads such a byte as 0 (measured: its own save writes
//! 0), so the font stays different from a twin with family 2 and empty cells formatted with the
//! twin survive a save. Every workbook is assembled here from the [MS-XLS] structures.

use gridcraft_core::CellRef;
use gridcraft_model::StyleId;

use super::xls_style_tests::{CELL, STYLE, default_fonts, font, number, plain, rec, style_at, u16s, workbook, xf};
use super::xls_tests::compound;
use crate::{read_xls, read_xlsx, write_xlsx};

/// Sets the family byte of a Font record built by `font` (after the 4-byte record header,
/// 10 bytes of fields and the underline byte).
fn with_family(mut font_record: Vec<u8>, family: u8) -> Vec<u8> {
    font_record[15] = family;
    font_record
}

fn blank(r: u16, c: u16, ixfe: u16) -> Vec<u8> {
    rec(0x0201, &u16s(&[r, c, ixfe]))
}

/// A workbook whose Normal font (ifnt 0) has family `normal`, with a twin font (ifnt 5) that
/// differs only by family 2. A1 is a number in the default cell format; B2 is an empty cell
/// formatted with the twin font.
fn twin_fonts(normal: u8) -> Vec<u8> {
    let mut f = default_fonts();
    f[0] = with_family(f[0].clone(), normal);
    f.push(with_family(font(200, 0, 0x7FFF, 400, 0, 0, "Arial"), 2));
    f.push(xf(0, 0, STYLE, plain()));
    f.push(xf(0, 0, CELL, plain()));
    f.push(xf(5, 0, CELL, plain()));
    compound("Workbook", &workbook(&f, &[number(0, 0, 1, 1.0), blank(1, 1, 2)]))
}

#[test]
fn out_of_range_family_reads_as_zero() {
    for (byte, expected) in [(0x20u8, 0u8), (0x30, 0), (0xF0, 0), (0, 0), (2, 2), (5, 5), (14, 14)] {
        let mut f = default_fonts();
        // ifnt 5 (index 4 is never written).
        f.push(with_family(font(200, 0, 0x7FFF, 400, 0, 0, "Courier New"), byte));
        f.push(xf(0, 0, STYLE, plain()));
        f.push(xf(5, 0, CELL, plain()));
        let (wb, _) = read_xls(&compound("Workbook", &workbook(&f, &[number(0, 0, 1, 1.0)]))).unwrap();
        assert_eq!(style_at(&wb, "A1").font.family, Some(expected), "bFamily {byte:#04x}");
    }
}

#[test]
fn empty_formatted_cells_survive_saving_as_xlsx() {
    let b2 = CellRef::parse("B2").unwrap();
    let (wb, _) = read_xls(&twin_fonts(0x20)).unwrap();
    assert_eq!(wb.styles.get(StyleId::DEFAULT).font.family, Some(0), "the Normal font's family");
    assert!(wb.sheets[0].cells.get(b2).is_some(), "B2 is stored after the .xls read");
    assert_ne!(wb.sheets[0].style_id(b2), StyleId::DEFAULT);
    let stored = wb.sheets[0].cells.len();

    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(back.sheets[0].cells.get(b2).is_some(), "B2 is still stored after saving as .xlsx");
    assert_ne!(back.sheets[0].style_id(b2), StyleId::DEFAULT, "B2 keeps a format of its own");
    assert_eq!(back.sheets[0].cells.len(), stored, "no cell is lost by the save");
}

#[test]
fn in_range_twin_of_the_normal_font_is_unchanged() {
    let b2 = CellRef::parse("B2").unwrap();
    let (wb, _) = read_xls(&twin_fonts(0)).unwrap();
    assert_eq!(wb.styles.get(StyleId::DEFAULT).font.family, Some(0));
    assert_eq!(style_at(&wb, "B2").font.family, Some(2));
    let stored = wb.sheets[0].cells.len();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_ne!(back.sheets[0].style_id(b2), StyleId::DEFAULT);
    assert_eq!(back.sheets[0].cells.len(), stored);
}
