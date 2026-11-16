//! `.xls` ROW records ([MS-XLS] 2.4.221) whose `miyRw` is outside the allowed 2..=8192 twips
//! (seen as 0x8000 in real files) are not a row height: the row opens at the sheet's default
//! height, as Excel shows it, instead of at height 0 (which hid every row of such a sheet).

use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

/// ROW record: row `r`, columns 0..1, height `miy`, then the 32-bit flags word.
fn row(r: u16, miy: u16, flags: u32) -> Vec<u8> {
    rec(0x0208, &cat(&[&u16s(&[r, 0, 1, miy, 0, 0]), &flags.to_le_bytes()]))
}

/// One worksheet "Sheet1" with DEFAULTROWHEIGHT 255 twips (12.75 pt), the given ROW records and
/// the number `r + 1` in column A of every row 0..6.
fn book(rows: &[Vec<u8>]) -> Vec<u8> {
    let xf = rec(0x00E0, &cat(&[&u16s(&[0, 0, 0]), &[0; 14]]));
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]));
    let globals = |pos: u32| cat(&[&bof(0x0005), &xf, &boundsheet(pos), &eof()]);
    let mut sheet = cat(&[&bof(0x0010), &rec(0x0225, &u16s(&[0, 255]))]);
    for r in rows {
        sheet.extend_from_slice(r);
    }
    for r in 0..6u16 {
        sheet.extend_from_slice(&rec(0x0203, &cat(&[&cell(r, 0, 0), &f64::from(r + 1).to_le_bytes()])));
    }
    sheet.extend_from_slice(&eof());
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn size(wb: &gridcraft_model::Workbook, r: u32) -> Option<f32> {
    wb.sheets[0].rows.get(&r).and_then(|i| i.size)
}

#[test]
fn out_of_range_row_height_opens_at_the_default_height() {
    // Flags 0x0100: the reserved bit Excel always sets; not custom, not hidden.
    let (wb, _) = read_xls(&book(&[row(0, 0x8000, 0x0100), row(1, 0x8000, 0x0100), row(2, 0, 0x0100)])).unwrap();
    for r in 0..3 {
        assert_eq!(size(&wb, r), None, "row {r} must use the default height");
        assert!(!wb.sheets[0].rows.get(&r).is_some_and(|i| i.hidden), "row {r} stays visible");
    }
    assert_eq!(wb.sheets[0].value(CellRef::new(1, 0)), Value::Number(2.0));
}

#[test]
fn in_range_heights_and_hidden_rows_are_unchanged() {
    // 600 twips custom = 30 pt; 8192 twips custom (the maximum, 409.6 pt); a hidden row whose
    // stored height is 0x8000 stays hidden.
    let (wb, _) = read_xls(&book(&[row(0, 600, 0x0140), row(1, 8192, 0x0140), row(2, 0x8000, 0x0120)])).unwrap();
    assert_eq!(size(&wb, 0), Some(40.0));
    assert!(size(&wb, 1).is_some_and(|px| px > 500.0), "{:?}", size(&wb, 1));
    assert!(wb.sheets[0].rows.get(&2).is_some_and(|i| i.hidden));
}

#[test]
fn saved_as_xlsx_the_rows_keep_the_default_height() {
    let (wb, _) = read_xls(&book(&[row(0, 0x8000, 0x0100), row(3, 0x8000, 0x0100)])).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for r in [0, 3] {
        assert_eq!(size(&back, r), None, "row {r} after .xlsx round trip");
    }
    assert_eq!(back.sheets[0].value(CellRef::new(3, 0)), Value::Number(4.0));
}

#[test]
fn ex_asc_and_ex_des_bits_read_as_thick_top_and_bottom_and_save_to_xlsx() {
    // [MS-XLS] 2.4.221: fExAsc is bit 28 and fExDes bit 29 of the flags dword (ixfe in 16..=27).
    // 315 twips = 15.75 pt, a stored height different from the 12.75 pt default.
    let (wb, _) = read_xls(&book(&[row(0, 315, 0x1000_0100), row(1, 315, 0x2000_0100), row(2, 315, 0x3000_0100), row(3, 315, 0x0FFF_0100)])).unwrap();
    let flags = |wb: &gridcraft_model::Workbook, r: u32| wb.sheets[0].rows.get(&r).map(|i| (i.thick_top, i.thick_bottom));
    let expected = [Some((true, false)), Some((false, true)), Some((true, true)), Some((false, false))];
    for (r, want) in expected.iter().enumerate() {
        assert_eq!(flags(&wb, r as u32), *want, "row {r}");
    }
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for (r, want) in expected.iter().take(3).enumerate() {
        assert_eq!(flags(&back, r as u32), *want, "row {r} after .xlsx round trip");
    }
}
