//! Excel 5.0/95 (BIFF5) page headers and footers: HEADER / FOOTER hold a 1-byte length and text
//! in the workbook code page. The workbooks are assembled from the BIFF5 record layouts.

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

fn text(t: &[u8]) -> Vec<u8> {
    [&[t.len() as u8][..], t].concat()
}

fn hf_warning(w: &[String]) -> bool {
    w.iter().any(|w| w.contains("page setup"))
}

#[test]
fn biff5_header_and_footer_are_read() {
    let records = [rec(0x0014, &text(b"&CCAREER 1997&RREFERENCE CODING")), rec(0x0015, &text(b"Page &P \xE9t\xE9"))].concat();
    let (wb, report) = read_xls(&book(&records)).expect("xls opens");
    let pr = &wb.sheets[0].print;
    assert_eq!(pr.header, "&CCAREER 1997&RREFERENCE CODING");
    assert_eq!(pr.footer, "Page &P été");
    assert!(!hf_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn an_empty_biff5_header_is_no_header() {
    let (wb, report) = read_xls(&book(&rec(0x0014, &[]))).expect("xls opens");
    assert_eq!(wb.sheets[0].print.header, "");
    assert!(!hf_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn a_biff5_header_longer_than_its_record_is_skipped_with_a_warning() {
    let (wb, report) = read_xls(&book(&rec(0x0014, &[20, b'a', b'b']))).expect("xls opens");
    assert_eq!(wb.sheets[0].print.header, "");
    assert!(hf_warning(&report.warnings), "{:?}", report.warnings);
}
