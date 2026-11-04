//! BIFF12 shared strings whose phonetic (fExtStr) tail is not the documented PhRun size, assembled in
//! Rust from the public format schema. Phonetic data is not imported, so the main text must still open.
use gridcraft_core::{CellRef, Value};

use super::xlsb_tests::{RELS, ROOT, TYPES, book, cell, package, record, row, wide, worksheet};
use crate::read_xlsb;

/// flags, main text, then the raw bytes that follow it in the record.
fn item(flags: u8, text: &str, tail: &[u8]) -> Vec<u8> {
    let mut out = vec![flags];
    out.extend(wide(text));
    out.extend(tail);
    record(19, &out)
}

fn strings(items: &[Vec<u8>]) -> Vec<u8> {
    let n = items.len() as u32;
    let mut header = n.to_le_bytes().to_vec();
    header.extend(n.to_le_bytes());
    let mut out = record(159, &header);
    for i in items {
        out.extend(i);
    }
    out.extend(record(160, &[]));
    out
}

fn open(items: &[Vec<u8>]) -> Option<gridcraft_model::Workbook> {
    let mut data = row(0);
    for col in 0..items.len() as u32 {
        data.extend(cell(7, col, &col.to_le_bytes()));
    }
    let bytes = package(&book(false), &worksheet(&data, &[]), &strings(items), ROOT, RELS, TYPES);
    read_xlsb(&bytes).ok().map(|(wb, _)| wb)
}

#[test]
fn phonetic_tails_of_any_size_keep_the_main_text() {
    let mut a = wide("");
    a.extend(0u32.to_le_bytes());
    a.extend([1, 0, 0x35, 0]);
    let mut b = wide("トウキョウ");
    b.extend(2u32.to_le_bytes());
    b.extend([0, 0, 0, 0, 1, 0, 2, 0, 1, 0, 1, 0, 1, 0, 0x35, 0]); // 16 bytes, short of 2 x 10.
    let mut c = wide("オオサカ");
    c.extend(0u32.to_le_bytes());
    c.extend([0, 0, 0, 0, 1, 0, 0x35, 0]);
    let mut d = 1u32.to_le_bytes().to_vec();
    d.extend([0, 0, 0, 0]); // One formatting run.
    d.extend(wide(""));
    d.extend(5u32.to_le_bytes());
    d.extend([1, 0, 0x35, 0]);
    let items = [item(2, "メモ", &a), item(2, "東京", &b), item(2, "大阪", &c), item(3, "名古屋", &d), item(0, "plain", &[])];
    let wb = open(&items).expect("the workbook opens");
    for (col, expected) in ["メモ", "東京", "大阪", "名古屋", "plain"].iter().enumerate() {
        assert_eq!(wb.sheets[0].value(CellRef::new(0, col as u32)), Value::from(*expected), "column {col}");
    }
}

#[test]
fn a_truncated_main_text_still_refuses() {
    let mut payload = vec![0u8];
    payload.extend(10u32.to_le_bytes());
    payload.extend([0x61, 0, 0x62, 0]); // Two of the ten UTF-16 units.
    assert!(open(&[record(19, &payload)]).is_none());
}
