//! .xls import tolerance: property sets whose declared size runs past the stream end ([MS-OLEPS] 2.21) and
//! XFExt records naming an XF that does not exist ([MS-XLS] 2.4.355). Every workbook is built in code.
use gridcraft_model::{Color, PatternType};

use super::binary_custom_props_tests::{all_types, custom_part, stream as user_stream};
use super::binary_doc_props::{DOCUMENT, stream as doc_stream, text, xls};
use super::xls_style_tests::{CELL, STYLE, XfData, default_fonts, number, plain, rec, style_at, workbook, xf};
use super::xls_tests::compound;
use crate::{ReadReport, read_xls, read_xlsx, write_xlsx};

const EXT_WARNING: &str = "Some extended cell colours (XFExt records) could not be read; the palette colours were kept.";
const PROPS_DROPPED: &str = "The custom document properties could not be read and were dropped.";

fn count(report: &ReadReport, text: &str) -> usize {
    report.warnings.iter().filter(|w| w.as_str() == text).count()
}

// ---- XFExt ----

fn frt(rt: u16) -> Vec<u8> {
    let mut v = rt.to_le_bytes().to_vec();
    v.extend_from_slice(&[0; 10]);
    v
}

/// An ExtProp of `cb` bytes holding a FullColorExt (cb 20 is the well-formed size).
fn ext_color(ext_type: u16, cb: u16, xclr_type: u16, value: u32) -> Vec<u8> {
    let mut v = ext_type.to_le_bytes().to_vec();
    v.extend_from_slice(&cb.to_le_bytes());
    if cb >= 20 {
        v.extend_from_slice(&xclr_type.to_le_bytes());
        v.extend_from_slice(&0i16.to_le_bytes());
        v.extend_from_slice(&value.to_le_bytes());
        v.extend_from_slice(&[0; 8]);
    }
    v
}

fn xf_ext(ixfe: u16, props: &[Vec<u8>]) -> Vec<u8> {
    let mut p = frt(0x087D);
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&ixfe.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&(props.len() as u16).to_le_bytes());
    p.extend(props.concat());
    rec(0x087D, &p)
}

/// Fonts, fifteen style XFs and cell XF 15 (16 XFs in all: indexes 0..=15), then `extra`; A1 uses XF 15.
fn book(extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut f = default_fonts();
    for _ in 0..15 {
        f.push(xf(0, 0, STYLE, plain()));
    }
    f.push(xf(0, 0, CELL, XfData { fls: 1, fore: 10, ..plain() }));
    f.extend(extra);
    compound("Workbook", &workbook(&f, &[number(0, 0, 15, 1.0)]))
}

/// LongRGBA bytes red, green, blue, reserved.
fn rgba(r: u8, g: u8, b: u8) -> u32 {
    u32::from_le_bytes([r, g, b, 0])
}

#[test]
fn orphan_xf_ext_records_do_not_warn() {
    let (wb, report) = read_xls(&book(vec![
        xf_ext(15, &[ext_color(0x04, 20, 2, rgba(0x12, 0x34, 0x56))]),
        xf_ext(16, &[ext_color(0x04, 20, 2, rgba(0xAA, 0, 0))]),
        xf_ext(400, &[ext_color(0x04, 20, 2, rgba(0, 0xAA, 0))]),
    ]))
    .unwrap();
    assert_eq!(count(&report, EXT_WARNING), 0, "{:?}", report.warnings);
    let s = style_at(&wb, "A1");
    assert_eq!(s.fill.pattern, PatternType::Solid);
    assert_eq!(s.fill.fg, Color::Rgb(0x123456));
}

#[test]
fn malformed_xf_ext_still_warns() {
    let (wb, report) = read_xls(&book(vec![xf_ext(15, &[ext_color(0x04, 2, 0, 0)])])).unwrap();
    assert_eq!(count(&report, EXT_WARNING), 1, "{:?}", report.warnings);
    // The palette fill (index 10, red) stays.
    assert_eq!(style_at(&wb, "A1").fill.fg, Color::Rgb(0xFF0000));
}

// ---- property sets ----

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

/// Adds `extra` to the Size field of the property set whose offset is stored at `offset_at`.
fn grow_size(stream: &mut [u8], offset_at: usize, extra: u32) {
    let off = u32_at(stream, offset_at) as usize;
    let size = u32_at(stream, off) + extra;
    stream[off..off + 4].copy_from_slice(&size.to_le_bytes());
}

#[test]
fn user_set_size_past_stream_end_is_clamped() {
    let mut doc = user_stream(Some(&all_types()));
    // The user-defined set is the last one in the stream; its declared size now runs 4 bytes past the end.
    let user_offset = u32_at(&doc, 64) as usize;
    assert_eq!(user_offset + u32_at(&doc, user_offset) as usize, doc.len());
    grow_size(&mut doc, 64, 4);
    let (wb, report) = read_xls(&xls(None, Some(&doc))).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("custom document properties")), "{:?}", report.warnings);
    assert_eq!(wb.props.category, "Finance");
    let part = String::from_utf8(custom_part(&wb).expect("custom properties kept")).unwrap();
    for name in ["Client", "Revision", "Rate", "Approved", "Due", "Note"] {
        assert!(part.contains(&format!("name=\"{name}\"")), "{name} missing in {part}");
    }
    assert!(part.contains("<vt:lpwstr>Acme</vt:lpwstr>"), "{part}");
    // Save As .xlsx keeps them.
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let saved = String::from_utf8(custom_part(&back).expect("custom properties saved")).unwrap();
    assert!(saved.contains("name=\"Client\"") && saved.contains("<vt:lpwstr>Acme</vt:lpwstr>"), "{saved}");
}

#[test]
fn standard_set_size_past_stream_end_is_clamped() {
    let mut doc = doc_stream(DOCUMENT, &[(2, text(0x1e, b"Finance\0"))]);
    assert_eq!(48 + u32_at(&doc, 48) as usize, doc.len());
    grow_size(&mut doc, 44, 4);
    let (wb, report) = read_xls(&xls(None, Some(&doc))).unwrap();
    assert_eq!(wb.props.category, "Finance", "{:?}", report.warnings);
}

#[test]
fn set_header_outside_stream_is_still_rejected() {
    let mut doc = user_stream(Some(&all_types()));
    // The user-defined set now starts 4 bytes before the stream end: its header can't be read.
    let at = (doc.len() - 4) as u32;
    doc[64..68].copy_from_slice(&at.to_le_bytes());
    let (wb, report) = read_xls(&xls(None, Some(&doc))).unwrap();
    assert_eq!(count(&report, PROPS_DROPPED), 1, "{:?}", report.warnings);
    assert!(custom_part(&wb).is_none());
}
