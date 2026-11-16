//! `.xls` unused supporting links ([MS-XLS] SupBook 2.4.271 whose `virtPath` is a single SPACE
//! character, with one single-space placeholder sheet name per `ctab`). Excel writes XCT records
//! (2.4.353, normally with no CRN rows) after them. No reference may use such a link, so its XCT and
//! CRN records are skipped without a warning, while real links before or after it keep their
//! cached cells and their `[n]` numbering.
use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, write_xlsx};

const CACHE_WARNING: &str = "cached cells of linked workbooks";

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn xlstr(s: &str) -> Vec<u8> {
    cat(&[&(s.len() as u16).to_le_bytes(), &[0], s.as_bytes()])
}

/// SupBook with an 8-bit `path` and sheet names.
fn supbook(path: &str, sheets: &[&str]) -> Vec<u8> {
    let mut p = cat(&[&u16s(&[sheets.len() as u16, path.len() as u16]), &[0], path.as_bytes()]);
    for s in sheets {
        p.extend(xlstr(s));
    }
    rec(0x01AE, &p)
}

fn xct(ccrn: u16, itab: u16) -> Vec<u8> {
    rec(0x0059, &u16s(&[ccrn, itab]))
}

/// CRN with one number at (`row`, `col`).
fn crn_num(row: u16, col: u8, x: f64) -> Vec<u8> {
    rec(0x005A, &cat(&[&[col, col], &row.to_le_bytes(), &[0x01], &x.to_le_bytes()]))
}

/// The real link: other.xls with sheets Data and Totals, Data!B2 = 42.
fn real_link() -> Vec<u8> {
    cat(&[&supbook("other.xls", &["Data", "Totals"]), &xct(1, 0), &crn_num(1, 1, 42.0)])
}

/// An unused SupBook with `n` placeholder sheets and `tail` (its XCT/CRN records) after it.
fn unused(n: usize, tail: &[u8]) -> Vec<u8> {
    cat(&[&supbook(" ", &vec![" "; n]), tail])
}

fn empty_xcts(n: u16) -> Vec<u8> {
    (0..n).flat_map(|i| xct(0, i)).collect()
}

fn ref3d(ixti: u16, r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &u16s(&[ixti, r, c | 0xC000])])
}

fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// Globals: Sheet1, the self SupBook (index 0), then `links` (SupBooks 1.. in order); XTI 0 =
/// Sheet1, XTI 1 = Data of the real link at SupBook index `real`. Sheet1!A1 = [1]Data!B2*2 (84).
fn file(links: &[u8], real: u16) -> Vec<u8> {
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            links,
            &rec(0x0017, &cat(&[&u16s(&[2]), &u16s(&[0, 0, 0]), &u16s(&[real, 0, 0])])),
            &eof(),
        ])
    };
    let sheet = cat(&[&bof(0x0010), &formula(0, 0, 84.0, &cat(&[&ref3d(1, 1, 1), &[0x1E, 2, 0], &[0x05]])), &eof()]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn saved_part(bytes: &[u8], name: &str) -> Option<String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    Some(s)
}

/// Reads `bytes`, checks the real link is kept as `[1]` with its cached cell, and returns the warnings.
fn check_real_link_kept(bytes: &[u8], case: &str) -> Vec<String> {
    let (wb, report) = read_xls(bytes).unwrap();
    let text = wb.sheets[0].cell(at("A1")).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(text.as_deref(), Some("[1]Data!B2*2"), "{case}: {:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(84.0), "{case}");
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml").unwrap_or_else(|| panic!("{case}: no externalLink1"));
    assert!(link.contains(r#"<cell r="B2"><v>42</v></cell>"#), "{case}: {link}");
    assert!(saved_part(&saved, "xl/externalLinks/externalLink2.xml").is_none(), "{case}: the unused link became a part");
    report.warnings
}

fn no_cache_warning(warnings: &[String], case: &str) {
    assert!(!warnings.iter().any(|w| w.contains(CACHE_WARNING)), "{case}: {warnings:?}");
}

#[test]
fn unused_link_before_a_real_link_reads_without_a_cache_warning() {
    // SupBook 1 = unused (40 placeholder sheets, an empty XCT each), SupBook 2 = other.xls.
    let bytes = file(&cat(&[&unused(40, &empty_xcts(40)), &real_link()]), 2);
    no_cache_warning(&check_real_link_kept(&bytes, "unused first"), "unused first");
}

#[test]
fn unused_link_after_a_real_link_reads_without_a_cache_warning() {
    let bytes = file(&cat(&[&real_link(), &unused(2, &empty_xcts(2))]), 1);
    no_cache_warning(&check_real_link_kept(&bytes, "unused last"), "unused last");
}

#[test]
fn rows_cached_under_an_unused_link_are_skipped_silently() {
    // An XCT with a CRN row after an unused link: nothing to keep, nothing to warn about, and
    // the real link after it still reads its own cache.
    let bytes = file(&cat(&[&unused(1, &cat(&[&xct(1, 0), &crn_num(0, 0, 7.0)])), &real_link()]), 2);
    no_cache_warning(&check_real_link_kept(&bytes, "unused with rows"), "unused with rows");
}

#[test]
fn unused_link_without_xct_records_reads_cleanly() {
    let bytes = file(&cat(&[&unused(3, &[]), &real_link()]), 2);
    no_cache_warning(&check_real_link_kept(&bytes, "no xct"), "no xct");
}

#[test]
fn xct_after_this_workbooks_own_supbook_still_warns() {
    // Guard: an XCT straight after the self SupBook is malformed and keeps today's warning.
    let bytes = file(&cat(&[&xct(0, 0), &real_link()]), 1);
    let warnings = check_real_link_kept(&bytes, "xct after self");
    assert!(warnings.iter().any(|w| w.contains(CACHE_WARNING)), "{warnings:?}");
}
