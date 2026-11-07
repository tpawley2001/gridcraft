//! Excel 5.0/95 (BIFF5) manual page breaks: HORIZONTALPAGEBREAKS / VERTICALPAGEBREAKS hold a count
//! and 2-byte row (column) numbers, where BIFF8 has 6-byte entries. Record sizes measured on a real
//! Excel 95 workbook (22 bytes for 10 breaks, 14 for 6).

use super::xls_tests::compound;
use crate::read_xls;

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &[0x00, 0x05, dt as u8, (dt >> 8) as u8, 0xBB, 0x0D, 0xCC, 0x07])
}

/// A one-sheet Book stream in code page 1252 whose sheet holds `records` and a NUMBER in A1.
fn book(records: &[u8]) -> Vec<u8> {
    let eof = rec(0x000A, &[]);
    let head = [bof(0x0005), rec(0x0042, &1252u16.to_le_bytes())].concat();
    let boundsheet = |pos: u32| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 4], b"Data"].concat());
    let pos = (head.len() + boundsheet(0).len() + eof.len()) as u32;
    let number = rec(0x0203, &[&[0u8; 6][..], &1.0f64.to_le_bytes()].concat());
    let body = [bof(0x0010), records.to_vec(), number, eof.clone()].concat();
    compound("Book", &[head, boundsheet(pos), eof, body].concat())
}

/// A BIFF5 break record: cbrk, then cbrk 2-byte row or column numbers.
fn breaks(id: u16, at: &[u16]) -> Vec<u8> {
    let mut p = (at.len() as u16).to_le_bytes().to_vec();
    for a in at {
        p.extend_from_slice(&a.to_le_bytes());
    }
    rec(id, &p)
}

fn break_warning(w: &[String]) -> bool {
    w.iter().any(|w| w.contains("page breaks"))
}

#[test]
fn biff5_row_breaks_are_read() {
    let rows = [34, 45, 74, 83, 111, 140, 197, 321, 335, 354];
    let (wb, report) = read_xls(&book(&breaks(0x001B, &rows))).expect("xls opens");
    let expected: Vec<u32> = rows.iter().map(|&r| u32::from(r)).collect();
    assert_eq!(wb.sheets[0].print.row_breaks, expected);
    assert!(!break_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn biff5_column_breaks_are_read() {
    let (wb, report) = read_xls(&book(&breaks(0x001A, &[5, 9]))).expect("xls opens");
    assert_eq!(wb.sheets[0].print.col_breaks, vec![5, 9]);
    assert!(wb.sheets[0].print.row_breaks.is_empty());
    assert!(!break_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn biff5_row_and_column_breaks_together() {
    let records = [breaks(0x001B, &[41, 82]), breaks(0x001A, &[7])].concat();
    let (wb, report) = read_xls(&book(&records)).expect("xls opens");
    assert_eq!(wb.sheets[0].print.row_breaks, vec![41, 82]);
    assert_eq!(wb.sheets[0].print.col_breaks, vec![7]);
    assert!(!break_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn biff5_break_zero_and_duplicates_are_ignored() {
    let (wb, report) = read_xls(&book(&breaks(0x001B, &[0, 20, 20, 30]))).expect("xls opens");
    assert_eq!(wb.sheets[0].print.row_breaks, vec![20, 30]);
    assert!(!break_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn a_biff5_break_record_shorter_than_its_count_is_skipped_with_a_warning() {
    // cbrk = 3 but only two row numbers follow.
    let (wb, report) = read_xls(&book(&rec(0x001B, &[3, 0, 10, 0, 20, 0]))).expect("xls opens");
    assert!(wb.sheets[0].print.row_breaks.is_empty());
    assert!(break_warning(&report.warnings), "{:?}", report.warnings);
}
