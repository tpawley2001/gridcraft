//! `.xls` SELECTION records list their ranges as RefU ([MS-XLS] 2.5.207, 6 bytes each): real
//! files (`9 + 6 * cref` bytes) open on their saved active cell without a warning.
use gridcraft_core::CellRef;

use super::xls_tests::{one_sheet_with, rec, u16s};
use crate::read_xls;

/// SELECTION with `refs` RefU ranges written and `cref` claimed.
fn selection(row: u16, col: u16, cref: u16, refs: usize) -> Vec<u8> {
    let mut out = vec![3u8];
    out.extend(u16s(&[row, col, 0, cref]));
    for i in 0..refs {
        let r = row + i as u16;
        out.extend(u16s(&[r, r]));
        out.extend([col as u8, col as u8 + 1]);
    }
    rec(0x001D, &out)
}

fn open(records: &[u8]) -> (CellRef, Vec<String>) {
    let (wb, report) = read_xls(&one_sheet_with(records)).expect("xls opens");
    (wb.sheets[0].view_active, report.warnings)
}

fn selection_warning(w: &[String]) -> bool {
    w.iter().any(|w| w.to_ascii_lowercase().contains("selection"))
}

#[test]
fn a_15_byte_selection_sets_the_active_cell_without_warning() {
    let r = selection(4, 2, 1, 1);
    assert_eq!(r.len(), 4 + 15);
    let (at, w) = open(&r);
    assert_eq!(at, CellRef::parse("C5").unwrap());
    assert!(!selection_warning(&w), "{w:?}");
}

#[test]
fn a_selection_of_three_ranges_is_read() {
    let r = selection(9, 0, 3, 3);
    assert_eq!(r.len(), 4 + 27);
    let (at, w) = open(&r);
    assert_eq!(at, CellRef::parse("A10").unwrap());
    assert!(!selection_warning(&w), "{w:?}");
}

#[test]
fn a_selection_missing_a_claimed_range_is_ignored_with_a_warning() {
    let (at, w) = open(&selection(9, 0, 2, 1));
    assert_eq!(at, CellRef::parse("A1").unwrap());
    assert!(selection_warning(&w), "{w:?}");
}
