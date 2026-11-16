//! `.xls` -> `.xlsx`: rows whose `.xls` ROW record ([MS-XLS] 2.4.221) stores a non-custom height below
//! the Normal font's default row height print at that height in the `.xls`. Excel re-fits `.xlsx` rows
//! without `customHeight` (ECMA-376 Part 1, 18.3.1.73) and never below the Normal default, so the writer
//! marks such short rows custom to keep the printed layout. Taller rows and non-`.xls` sheets are unchanged.

use super::xls_tests::{cat, one_sheet_with, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

/// ROW (0x0208): rw, colMic, colMac, miyRw (twips), reserved, unused, then the flags dword
/// (0x40 = fUnsynced, 0x100 = the always-set reserved bit).
fn row(r: u16, twips: u16, flags: u32) -> Vec<u8> {
    rec(0x0208, &cat(&[&u16s(&[r, 0, 1, twips, 0, 0]), &flags.to_le_bytes()]))
}

fn px(pt: f32) -> f32 {
    pt * 96.0 / 72.0
}

fn saved_xlsx_from_xls(records: &[u8]) -> gridcraft_model::Workbook {
    let (wb, _) = read_xls(&one_sheet_with(records)).unwrap();
    assert!(wb.sheets[0].fit_print_rows_without_default_floor, "precondition: .xls sheets print without the floor");
    read_xlsx(&write_xlsx(&wb).unwrap()).unwrap().0
}

#[test]
fn a_short_non_custom_xls_row_is_saved_with_custom_height() {
    // Row 2 (rw 1): 8 pt (160 twips), not custom: below any Normal default row height.
    let back = saved_xlsx_from_xls(&row(1, 160, 0x100));
    let r = back.sheets[0].rows.get(&1).expect("the short row is written");
    assert!(r.custom, "{r:?}");
    assert!((r.size.unwrap() - px(8.0)).abs() < 0.5, "{r:?}");
}

#[test]
fn a_tall_non_custom_xls_row_stays_auto_height() {
    // Row 2 (rw 1): 30 pt (600 twips), not custom: above the Normal default, Excel may re-fit it.
    let back = saved_xlsx_from_xls(&row(1, 600, 0x100));
    if let Some(r) = back.sheets[0].rows.get(&1) {
        assert!(!r.custom, "{r:?}");
    }
}

#[test]
fn a_custom_xls_row_stays_custom_at_its_height() {
    let back = saved_xlsx_from_xls(&row(1, 160, 0x140));
    let r = back.sheets[0].rows.get(&1).expect("the custom row is written");
    assert!(r.custom, "{r:?}");
    assert!((r.size.unwrap() - px(8.0)).abs() < 0.5, "{r:?}");
}

#[test]
fn a_hidden_short_xls_row_is_not_forced_custom() {
    // fDyZero (0x20) hides the row; the writer leaves hidden rows as they are today.
    let (wb, _) = read_xls(&one_sheet_with(&row(1, 160, 0x120))).unwrap();
    let before = wb.sheets[0].rows.get(&1).map(|r| r.custom);
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].rows.get(&1).map(|r| r.custom), before);
}

#[test]
fn xlsx_sheets_keep_short_auto_rows_auto() {
    // The same short non-custom row on a sheet that did not come from .xls is written as today.
    let (mut wb, _) = read_xls(&one_sheet_with(&row(1, 160, 0x100))).unwrap();
    std::sync::Arc::make_mut(&mut wb.sheets[0]).fit_print_rows_without_default_floor = false;
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    if let Some(r) = back.sheets[0].rows.get(&1) {
        assert!(!r.custom, "{r:?}");
    }
}
