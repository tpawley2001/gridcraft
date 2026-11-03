//! Legacy `.xls` (BIFF8) workbook theme (Theme, ContinueFrt12) and full-fidelity cell colours
//! (XFExt). Every workbook is assembled here from the [MS-XLS] structures; no Excel-produced
//! file is used.

use gridcraft_model::{Color, PatternType, Theme};

use super::make_zip;
use super::xls_style_tests::{CELL, STYLE, XfData, default_fonts, number, plain, rec, style_at, workbook, xf};
use super::xls_tests::compound;
use crate::{read_xls, read_xlsx, write_xlsx};

const THEME_WARNING: &str = "The .xls workbook theme could not be read; theme colours and fonts use the default Office theme.";

const THEME_XML: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Synthetic"><a:themeElements><a:clrScheme name="Synthetic"><a:accent1><a:srgbClr val="112233"/></a:accent1></a:clrScheme><a:fontScheme name="Synthetic"><a:majorFont><a:latin typeface="Georgia"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="theme/theme/themeManager.xml"/></Relationships>"#;
const MANAGER_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme1.xml"/></Relationships>"#;
const MANAGER: &str = r#"<?xml version="1.0"?><a:themeManager xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"/>"#;

/// A theme package laid out as a theme manager part pointing at the theme part.
fn theme_zip() -> Vec<u8> {
    make_zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("theme/theme/themeManager.xml", MANAGER.as_bytes()),
        ("theme/theme/_rels/themeManager.xml.rels", MANAGER_RELS.as_bytes()),
        ("theme/theme/theme1.xml", THEME_XML.as_bytes()),
    ])
}

/// FrtHeader: the record type, then zero flags and reserved bytes.
fn frt(rt: u16) -> Vec<u8> {
    let mut v = rt.to_le_bytes().to_vec();
    v.extend_from_slice(&[0; 10]);
    v
}

/// A Theme record followed by ContinueFrt12 records: one per chunk after the first.
fn theme_records(version: u32, chunks: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut out = vec![];
    for (i, chunk) in chunks.iter().enumerate() {
        if i == 0 {
            out.push(rec(0x0896, &[frt(0x0896), version.to_le_bytes().to_vec(), chunk.to_vec()].concat()));
        } else {
            out.push(rec(0x087F, &[frt(0x087F), chunk.to_vec()].concat()));
        }
    }
    if chunks.is_empty() {
        out.push(rec(0x0896, &[frt(0x0896), version.to_le_bytes().to_vec()].concat()));
    }
    out
}

/// An ExtProp holding a FullColorExt.
fn ext_color(ext_type: u16, xclr_type: u16, tint: i16, value: u32) -> Vec<u8> {
    let mut v = ext_type.to_le_bytes().to_vec();
    v.extend_from_slice(&20u16.to_le_bytes());
    v.extend_from_slice(&xclr_type.to_le_bytes());
    v.extend_from_slice(&tint.to_le_bytes());
    v.extend_from_slice(&value.to_le_bytes());
    v.extend_from_slice(&[0; 8]);
    v
}

/// An XFExt record for XF `ixfe` with the given ExtProps.
fn xf_ext(ixfe: u16, props: &[Vec<u8>]) -> Vec<u8> {
    let mut p = frt(0x087D);
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&ixfe.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&(props.len() as u16).to_le_bytes());
    p.extend(props.concat());
    rec(0x087D, &p)
}

/// Fonts, fifteen style XFs and cell XF 15 (solid red fill from the palette), then `extra`.
fn formatting(extra: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut f = default_fonts();
    for _ in 0..15 {
        f.push(xf(0, 0, STYLE, plain()));
    }
    f.push(xf(0, 0, CELL, XfData { fls: 1, fore: 10, ..plain() }));
    f.extend(extra);
    f
}

fn book(extra: Vec<Vec<u8>>) -> Vec<u8> {
    compound("Workbook", &workbook(&formatting(extra), &[number(0, 0, 15, 1.0)]))
}

fn theme_warnings(warnings: &[String]) -> usize {
    warnings.iter().filter(|w| w.as_str() == THEME_WARNING).count()
}

#[test]
fn theme_in_one_record() {
    let zip = theme_zip();
    let (wb, report) = read_xls(&book(theme_records(0, &[&zip]))).unwrap();
    assert_eq!(wb.theme.colors[4], 0x112233);
    assert_eq!(wb.theme.major_font, "Georgia");
    assert_eq!(theme_warnings(&report.warnings), 0);
}

#[test]
fn theme_split_across_continue_records() {
    let zip = theme_zip();
    let (a, rest) = zip.split_at(zip.len() / 3);
    let (b, c) = rest.split_at(rest.len() / 2);
    let (wb, report) = read_xls(&book(theme_records(0, &[a, b, c]))).unwrap();
    assert_eq!(wb.theme.colors[4], 0x112233);
    assert_eq!(wb.theme.major_font, "Georgia");
    assert_eq!(theme_warnings(&report.warnings), 0);
}

#[test]
fn builtin_theme_keeps_default() {
    let (wb, report) = read_xls(&book(theme_records(124_226, &[]))).unwrap();
    assert_eq!(wb.theme, Theme::default());
    assert_eq!(theme_warnings(&report.warnings), 0);
}

#[test]
fn corrupt_theme_warns_once_and_cells_load() {
    let junk = vec![0x5A; 300];
    let (wb, report) = read_xls(&book(theme_records(0, &[&junk, &junk]))).unwrap();
    assert_eq!(wb.theme, Theme::default());
    assert_eq!(theme_warnings(&report.warnings), 1);
    assert_eq!(style_at(&wb, "A1").fill.pattern, PatternType::Solid);
}

/// 13107/32767 of full tint, in thousandths.
fn tint_40() -> i16 {
    (13107.0f64 / 32767.0 * 1000.0).round() as i16
}

fn coloured() -> Vec<u8> {
    // RGB as LongRGBA: red, green, blue, reserved.
    let rgb = u32::from_le_bytes([0xC0, 0x00, 0x00, 0x00]);
    book(vec![xf_ext(15, &[ext_color(0x04, 3, 13107, 4), ext_color(0x0D, 2, 0, rgb)])])
}

#[test]
fn xf_ext_theme_fill_and_rgb_text() {
    let (wb, _) = read_xls(&coloured()).unwrap();
    let s = style_at(&wb, "A1");
    assert_eq!(s.fill.pattern, PatternType::Solid);
    assert_eq!(s.fill.fg, Color::Theme(4, tint_40()));
    assert_eq!(s.fill.fg, Color::Theme(4, 400));
    assert_eq!(s.font.color, Color::Rgb(0xC00000));
}

#[test]
fn xf_ext_past_the_xf_table_is_ignored() {
    let (wb, _) = read_xls(&book(vec![xf_ext(400, &[ext_color(0x04, 3, 0, 4)])])).unwrap();
    let s = style_at(&wb, "A1");
    assert_eq!(s.fill.fg, Color::Rgb(0xFF0000));
    assert_eq!(wb.sheets[0].cell(gridcraft_core::CellRef::new(0, 0)).map(|c| c.value.clone()), Some(gridcraft_core::Value::Number(1.0)));
}

#[test]
fn theme_and_xf_ext_survive_xlsx_round_trip() {
    let zip = theme_zip();
    let mut extra = theme_records(0, &[&zip]);
    extra.push(xf_ext(15, &[ext_color(0x04, 3, 13107, 4), ext_color(0x0D, 2, 0, u32::from_le_bytes([0xC0, 0, 0, 0]))]));
    let (wb, _) = read_xls(&book(extra)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.theme.colors[4], 0x112233);
    assert_eq!(back.theme.major_font, "Georgia");
    let s = style_at(&back, "A1");
    assert_eq!(s.fill.fg, Color::Theme(4, 400));
    assert_eq!(s.font.color, Color::Rgb(0xC00000));
}
