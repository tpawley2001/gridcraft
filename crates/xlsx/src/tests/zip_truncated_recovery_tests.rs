//! A package whose ZIP lost its end (no central directory, e.g. a truncated download) still opens
//! from its local file headers, as Excel's repair does; the incomplete last part is dropped.

use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, Sheet, Workbook};

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
    wb.sheets.push(Arc::new(second));
    write_xlsx(&wb).unwrap()
}

/// (name, local header offset) of every entry, from the central directory of an intact file.
fn entries(bytes: &[u8]) -> Vec<(String, usize)> {
    let u16_at = |i: usize| usize::from(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
    let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let mut out = Vec::new();
    let mut i = central_directory(bytes);
    while bytes.get(i..i + 4) == Some(&b"PK\x01\x02"[..]) {
        let (name_len, extra_len, comment_len) = (u16_at(i + 28), u16_at(i + 30), u16_at(i + 32));
        let name = String::from_utf8_lossy(&bytes[i + 46..i + 46 + name_len]).into_owned();
        out.push((name, u32_at(i + 42)));
        i += 46 + name_len + extra_len + comment_len;
    }
    out
}

/// Offset of the first central directory header (from the end of central directory record).
fn central_directory(bytes: &[u8]) -> usize {
    let eocd = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").expect("intact file has an EOCD");
    u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap()) as usize
}

fn values(wb: &Workbook) -> Vec<(String, Vec<Value>)> {
    wb.sheets
        .iter()
        .map(|s| {
            let vals = (1..=20u32).flat_map(|r| [s.value(at(&format!("A{r}"))), s.value(at(&format!("B{r}")))]).collect();
            (s.name.to_string(), vals)
        })
        .collect()
}

#[test]
fn intact_file_opens_without_damage_warning() {
    let (wb, rep) = read_xlsx(&sample()).unwrap();
    assert_eq!(wb.sheets.len(), 2);
    assert!(!rep.warnings.iter().any(|w| w.to_lowercase().contains("damaged")), "{:?}", rep.warnings);
}

#[test]
fn missing_central_directory_recovers_every_part() {
    let good = sample();
    let (expected, _) = read_xlsx(&good).unwrap();
    let cut = &good[..central_directory(&good)];
    let (wb, rep) = read_xlsx(cut).expect("a ZIP without its central directory opens from local headers");
    assert_eq!(values(&wb), values(&expected));
    assert_eq!(wb.sheets[1].value(at("C3")), Value::text("kept"));
    let damaged: Vec<_> = rep.warnings.iter().filter(|w| w.to_lowercase().contains("damaged")).collect();
    assert_eq!(damaged.len(), 1, "one damage warning: {:?}", rep.warnings);
}

#[test]
fn truncated_last_part_is_dropped_and_the_rest_kept() {
    let good = sample();
    let list = entries(&good);
    let (last, start) = list.iter().max_by_key(|(_, off)| *off).cloned().unwrap();
    let essential = last == "[Content_Types].xml" || last == "xl/workbook.xml" || last.starts_with("xl/worksheets/") || last.contains("_rels/");
    assert!(!essential, "test assumes the writer's last entry is not essential, it is {last}: pick another");
    // Cut halfway through the last entry's data (its local header is 30 bytes + name).
    let data_start = start + 30 + last.len();
    let cd = central_directory(&good);
    let cut = &good[..data_start + (cd - data_start) / 2];
    let (wb, rep) = read_xlsx(cut).expect("parts before the incomplete one are recovered");
    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(wb.sheets[0].value(at("A20")), Value::Number(30.0));
    assert!(rep.warnings.iter().any(|w| w.to_lowercase().contains("damaged")), "{:?}", rep.warnings);
}

#[test]
fn losing_the_workbook_part_is_still_an_error() {
    let good = sample();
    let list = entries(&good);
    let start = list.iter().find(|(n, _)| n == "xl/workbook.xml").map(|(_, o)| *o).unwrap();
    let today = read_xlsx(&good[..start + 40]).err().map(|e| e.to_string());
    assert!(today.is_some(), "a package cut inside xl/workbook.xml can't open");
}

#[test]
fn tiny_prefix_and_non_zip_bytes_are_errors() {
    let good = sample();
    assert!(read_xlsx(&good[..100]).is_err());
    let noise: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    let err = read_xlsx(&noise).err().expect("random bytes are not a workbook").to_string();
    assert!(err.contains("not a valid zip package"), "{err}");
}
