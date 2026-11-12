//! `.xls` import: a cell's text holds at most Excel's 32,767 characters (UTF-16 code units). A BIFF8
//! string can declare up to 65,535, and a workbook saved with such a cell is one Excel repairs on
//! open, so the reader cuts longer text to its first 32,767 units (never through a surrogate pair)
//! and says so once.

use super::xls_tests::{bof, cat, cell, compound, eof, rec, v};
use crate::{read_xls, read_xlsx, write_xlsx};
use gridcraft_core::Value;

/// Excel's limit for one cell's text, in UTF-16 code units.
const LIMIT: usize = 32_767;

/// The most data bytes one BIFF8 record holds.
const MAX_RECORD: usize = 8224;

/// An SST record followed by the CONTINUE records its strings spill into. Each string is
/// (stored as UTF-16, its UTF-16 code units). A string's header and first character stay in one
/// record; character data continued in a CONTINUE record starts with its own flags byte.
fn sst(strings: &[(bool, Vec<u16>)]) -> Vec<u8> {
    let n = strings.len() as u32;
    let mut parts: Vec<Vec<u8>> = vec![cat(&[&n.to_le_bytes(), &n.to_le_bytes()])];
    for (wide, units) in strings {
        let width = if *wide { 2 } else { 1 };
        if parts.last().is_some_and(|p| MAX_RECORD - p.len() < 3 + width) {
            parts.push(Vec::new());
        }
        if let Some(p) = parts.last_mut() {
            p.extend_from_slice(&(units.len() as u16).to_le_bytes());
            p.push(u8::from(*wide));
        }
        let mut at = 0;
        while at < units.len() {
            let room = parts.last().map_or(0, |p| (MAX_RECORD - p.len()) / width);
            if room == 0 {
                parts.push(vec![u8::from(*wide)]);
                continue;
            }
            let take = room.min(units.len() - at);
            if let (Some(p), Some(chunk)) = (parts.last_mut(), units.get(at..at + take)) {
                for u in chunk {
                    if *wide {
                        p.extend_from_slice(&u.to_le_bytes());
                    } else {
                        p.push(*u as u8);
                    }
                }
            }
            at += take;
        }
    }
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        out.extend_from_slice(&rec(if i == 0 { 0x00FC } else { 0x003C }, p));
    }
    out
}

/// A one-sheet BIFF8 workbook ("Sheet1") whose cells A1, A2, ... hold the shared strings in order.
fn book(strings: &[(bool, Vec<u16>)]) -> Vec<u8> {
    let globals = |pos: u32| cat(&[&bof(0x0005), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &sst(strings), &eof()]);
    let mut sheet = bof(0x0010);
    for i in 0..strings.len() {
        sheet.extend_from_slice(&rec(0x00FD, &cat(&[&cell(i as u16, 0, 0), &(i as u32).to_le_bytes()])));
    }
    sheet.extend_from_slice(&eof());
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}

fn narrow(s: &str) -> (bool, Vec<u16>) {
    (false, s.bytes().map(u16::from).collect())
}

fn wide(s: &str) -> (bool, Vec<u16>) {
    (true, s.encode_utf16().collect())
}

fn text(wb: &gridcraft_model::Workbook, a1: &str) -> String {
    match v(wb, 0, a1) {
        Value::Text(t) => t.to_string(),
        other => panic!("{a1} is not text: {other:?}"),
    }
}

fn units(s: &str) -> usize {
    s.encode_utf16().count()
}

fn limit_warnings(w: &[String]) -> Vec<&String> {
    w.iter().filter(|w| w.contains("32,767")).collect()
}

fn long_texts() -> (String, String, String) {
    let over = format!("8 {}", "x".repeat(32_768));
    let exact = "y".repeat(LIMIT);
    let emoji = "\u{1F600}".repeat(16_384);
    (over, exact, emoji)
}

#[test]
fn xls_long_text_is_cut_to_excels_cell_limit() {
    let (over, exact, emoji) = long_texts();
    assert_eq!((units(&over), units(&exact), units(&emoji)), (32_770, 32_767, 32_768));
    let (wb, report) = read_xls(&book(&[narrow(&over), narrow(&exact), wide(&emoji), narrow("short")])).unwrap();

    // Over the limit: the first 32,767 units are kept.
    let a1 = text(&wb, "A1");
    assert_eq!(units(&a1), LIMIT, "A1 length");
    assert_eq!(a1, over.chars().take(LIMIT).collect::<String>(), "A1 keeps its first 32,767 characters");
    // Exactly at the limit, and short text: unchanged.
    assert_eq!(text(&wb, "A2"), exact);
    assert_eq!(text(&wb, "A4"), "short");
    // 16,384 two-unit characters are 32,768 units: the last character goes whole, not half of it.
    let a3 = text(&wb, "A3");
    assert_eq!(a3, "\u{1F600}".repeat(16_383), "A3 is cut between characters");
    assert_eq!(units(&a3), 32_766);

    // One warning names the limit and the first cell that was cut.
    let warned = limit_warnings(&report.warnings);
    assert_eq!(warned.len(), 1, "one warning about the limit: {:?}", report.warnings);
    assert!(warned.iter().all(|w| w.contains("Sheet1!A1")), "the warning names the first cell: {warned:?}");
}

#[test]
fn xls_text_within_the_limit_is_kept_without_a_warning() {
    let (_, exact, _) = long_texts();
    let fits = "\u{1F600}".repeat(16_383);
    let (wb, report) = read_xls(&book(&[narrow(&exact), wide(&fits), narrow("short")])).unwrap();
    assert_eq!(text(&wb, "A1"), exact);
    assert_eq!(text(&wb, "A2"), fits);
    assert_eq!(text(&wb, "A3"), "short");
    assert!(limit_warnings(&report.warnings).is_empty(), "{:?}", report.warnings);
}

#[test]
fn xls_long_text_saved_as_xlsx_holds_no_cell_over_the_limit() {
    let (over, exact, emoji) = long_texts();
    let (wb, _) = read_xls(&book(&[narrow(&over), narrow(&exact), wide(&emoji)])).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for a1 in ["A1", "A2", "A3"] {
        let t = text(&back, a1);
        assert!(units(&t) <= LIMIT, "{a1} holds {} UTF-16 units after save", units(&t));
    }
    assert_eq!(text(&back, "A2"), exact);
}
