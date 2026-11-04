//! BIFF8 LABEL records as some third-party writers save them: an empty string without the
//! XLUnicodeString flags byte, and characters followed by unused trailing bytes. Excel opens both.

use gridcraft_core::Value;

use super::xls_tests::{cat, cell, one_sheet_with, rec, v};
use crate::read_xls;

/// A LABEL (0x0204) whose payload after rw/col/ixfe is `tail` exactly.
fn label(c: u16, tail: &[u8]) -> Vec<u8> {
    rec(0x0204, &cat(&[&cell(0, c, 0), tail]))
}

#[test]
fn empty_label_without_flags_byte_opens_as_empty_text() {
    let file = one_sheet_with(&label(0, &0u16.to_le_bytes()));
    let (wb, _) = read_xls(&file).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Text("".into()));
}

#[test]
fn label_with_trailing_bytes_keeps_its_characters() {
    let utf16: Vec<u8> = "abc".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let records = cat(&[
        &label(0, &0u16.to_le_bytes()),
        &label(1, &cat(&[&3u16.to_le_bytes(), &[1], &utf16, &[0, 0]])),
        &label(2, &cat(&[&3u16.to_le_bytes(), &[0], b"xyz"])),
    ]);
    let (wb, _) = read_xls(&one_sheet_with(&records)).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Text("".into()));
    assert_eq!(v(&wb, 0, "B1"), Value::Text("abc".into()));
    assert_eq!(v(&wb, 0, "C1"), Value::Text("xyz".into()));
}

#[test]
fn label_missing_declared_characters_is_still_refused() {
    let file = one_sheet_with(&label(3, &cat(&[&4u16.to_le_bytes(), &[0], b"ab"])));
    assert!(read_xls(&file).is_err());
}
