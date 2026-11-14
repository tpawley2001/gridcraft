//! Legacy `.xls` ROW records ([MS-XLS] 2.4.221 Row): a worksheet with ROW records prints its
//! non-custom rows at their largest cell font's default row height with no floor at the Normal
//! default, so the reader sets both print-fit flags. `.xlsx` sheets keep the floor.

use super::xls_tests::{cat, one_sheet_with, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

/// ROW (0x0208): rw, colMic, colMac, miyRw (twips), reserved, unused, then the flags dword
/// (0x40 = fUnsynced, 0x100 = the always-set reserved bit).
fn row(r: u16, twips: u16, flags: u32) -> Vec<u8> {
    rec(0x0208, &cat(&[&u16s(&[r, 0, 1, twips, 0, 0]), &flags.to_le_bytes()]))
}

#[test]
fn xls_row_records_turn_on_print_fitting_without_the_default_floor() {
    // Row 2 stored at 13.55 pt (271 twips) not custom; row 3 custom at 13.55 pt.
    let records = cat(&[&row(1, 271, 0x100), &row(2, 271, 0x140)]);
    let (wb, _) = read_xls(&one_sheet_with(&records)).unwrap();
    let sh = &wb.sheets[0];
    assert!(sh.fit_auto_row_heights_in_print);
    assert!(sh.fit_print_rows_without_default_floor);
    let custom = sh.rows.get(&2).expect("the custom row is kept");
    assert!(custom.custom);
    assert!((custom.size.unwrap() - 271.0 / 20.0 * 96.0 / 72.0).abs() < 0.5, "{custom:?}");
    if let Some(auto) = sh.rows.get(&1) {
        assert!(!auto.custom, "{auto:?}");
    }
}

#[test]
fn xlsx_sheets_keep_the_default_floor() {
    let records = row(1, 271, 0x100);
    let (wb, _) = read_xls(&one_sheet_with(&records)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let sh = &back.sheets[0];
    assert!(sh.fit_auto_row_heights_in_print, "the .xlsx reader fits rows");
    assert!(!sh.fit_print_rows_without_default_floor, "only the .xls reader drops the floor");
}
