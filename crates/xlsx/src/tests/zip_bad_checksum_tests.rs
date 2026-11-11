//! A package part whose ZIP CRC-32 doesn't match its (complete, well-formed) data opens with that
//! content, as Excel does, instead of the whole file being refused. A part whose compressed data
//! is really broken still fails, with its own error rather than a package-size one.

use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, Sheet, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

fn sample() -> Vec<u8> {
    let mut wb = Workbook::new();
    let first = wb.sheet_mut(0).unwrap();
    first.name = "Data".into();
    for r in 1..=20u32 {
        first.cells.set(at(&format!("A{r}")), Cell::value(Value::Number(f64::from(r) * 1.5)));
        first.cells.set(at(&format!("B{r}")), Cell::value(Value::text(format!("row {r}"))));
    }
    let mut second = Sheet::new("Second");
    second.cells.set(at("C3"), Cell::value(Value::text("kept")));
    second.cells.set(at("D4"), Cell::value(Value::Number(42.0)));
    wb.sheets.push(Arc::new(second));
    write_xlsx(&wb).unwrap()
}

/// (name, central directory header offset, local header offset) of every entry.
fn entries(bytes: &[u8]) -> Vec<(String, usize, usize)> {
    let u16_at = |i: usize| usize::from(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
    let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let eocd = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").expect("intact file has an EOCD");
    let mut i = u32_at(eocd + 16);
    let mut out = Vec::new();
    while bytes.get(i..i + 4) == Some(&b"PK\x01\x02"[..]) {
        let (name_len, extra_len, comment_len) = (u16_at(i + 28), u16_at(i + 30), u16_at(i + 32));
        let name = String::from_utf8_lossy(&bytes[i + 46..i + 46 + name_len]).into_owned();
        out.push((name, i, u32_at(i + 42)));
        i += 46 + name_len + extra_len + comment_len;
    }
    out
}

fn entry(bytes: &[u8], part: &str) -> (usize, usize) {
    let (_, cd, local) = entries(bytes).into_iter().find(|(n, _, _)| n == part).unwrap_or_else(|| panic!("no {part}"));
    (cd, local)
}

/// Alters the stored CRC-32 of `part` in its local header (offset 14) and its central directory
/// header (offset 16), leaving the data itself untouched.
fn break_crc(bytes: &mut [u8], part: &str) {
    let (cd, local) = entry(bytes, part);
    for at in [local + 14, cd + 16] {
        bytes[at] ^= 0x5A;
        bytes[at + 3] ^= 0xA5;
    }
}

/// Overwrites the start of `part`'s compressed data so it no longer inflates.
fn break_deflate(bytes: &mut [u8], part: &str) {
    let (_, local) = entry(bytes, part);
    let u16_at = |i: usize| usize::from(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
    assert_eq!(u16_at(local + 8), 8, "{part} is deflated");
    let data = local + 30 + u16_at(local + 26) + u16_at(local + 28);
    for b in &mut bytes[data..data + 8] {
        *b = 0xFF;
    }
}

fn sheet<'a>(wb: &'a Workbook, name: &str) -> &'a Sheet {
    wb.sheets.iter().find(|s| s.name.as_str() == name).unwrap_or_else(|| panic!("no sheet {name}"))
}

#[test]
fn a_sheet_with_a_wrong_checksum_opens_with_its_content() {
    let mut bytes = sample();
    break_crc(&mut bytes, "xl/worksheets/sheet1.xml");
    let (wb, report) = read_xlsx(&bytes).expect("Excel opens a part whose only damage is its checksum");
    assert_eq!(wb.sheets.len(), 2);
    let data = sheet(&wb, "Data");
    assert_eq!(data.value(at("A1")), Value::Number(1.5));
    assert_eq!(data.value(at("A20")), Value::Number(30.0));
    assert_eq!(data.value(at("B7")), Value::text("row 7"));
    assert_eq!(sheet(&wb, "Second").value(at("C3")), Value::text("kept"));
    let named: Vec<_> = report.warnings.iter().filter(|w| w.contains("xl/worksheets/sheet1.xml") && w.contains("checksum")).collect();
    assert_eq!(named.len(), 1, "one warning names the damaged part: {:?}", report.warnings);
}

#[test]
fn a_damaged_second_sheet_keeps_both_sheets() {
    let mut bytes = sample();
    break_crc(&mut bytes, "xl/worksheets/sheet2.xml");
    let (wb, report) = read_xlsx(&bytes).expect("opens");
    assert_eq!(sheet(&wb, "Data").value(at("A2")), Value::Number(3.0));
    let second = sheet(&wb, "Second");
    assert_eq!(second.value(at("C3")), Value::text("kept"));
    assert_eq!(second.value(at("D4")), Value::Number(42.0));
    assert!(report.warnings.iter().any(|w| w.contains("xl/worksheets/sheet2.xml") && w.contains("checksum")), "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("xl/worksheets/sheet1.xml")), "{:?}", report.warnings);
}

#[test]
fn the_package_lists_a_checksum_failure_once() {
    let mut bytes = sample();
    break_crc(&mut bytes, "xl/worksheets/sheet1.xml");
    let mut pkg = Package::open(&bytes).unwrap();
    let first = pkg.read("xl/worksheets/sheet1.xml").unwrap().expect("part read despite its checksum");
    assert!(first.starts_with(b"<?xml") || first.starts_with(b"<worksheet"), "the part's own bytes");
    let again = pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap();
    assert_eq!(first, again);
    assert_eq!(pkg.checksum_failures(), &["xl/worksheets/sheet1.xml".to_string()][..]);
    assert!(pkg.read("xl/workbook.xml").unwrap().is_some(), "later parts still read");
}

#[test]
fn a_broken_deflate_stream_still_fails_without_a_size_message() {
    let mut bytes = sample();
    break_deflate(&mut bytes, "xl/worksheets/sheet1.xml");
    let msg = match read_xlsx(&bytes) {
        Err(e) => e.to_string(),
        // Opening with the part dropped and a warning is acceptable too; it must not claim the
        // package is over-size.
        Ok((_, report)) => report.warnings.join(" | "),
    };
    assert!(!msg.contains("1 GB") && !msg.contains("too large"), "{msg}");
}

#[test]
fn an_intact_package_has_no_checksum_warning() {
    let bytes = sample();
    let (_, report) = read_xlsx(&bytes).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("checksum")), "{:?}", report.warnings);
    let mut pkg = Package::open(&bytes).unwrap();
    pkg.read("xl/worksheets/sheet1.xml").unwrap();
    assert!(pkg.checksum_failures().is_empty());
}
