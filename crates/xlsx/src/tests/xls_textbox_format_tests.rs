//! `.xls` text boxes and drawn shapes keep the fill and outline of their Office Art properties
//! and the font of their TxO's first formatting run, as Excel shows them ([MS-ODRAW] 2.3.7 fill
//! and 2.3.8 line style properties; [MS-XLS] 2.4.329 TxO and 2.5.129 FontIndex).

use gridcraft_model::{Color, Shape, ShapeKind};

use crate::read_xls;

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;

const FILL_COLOR: u16 = 0x0181;
const FILL_BOOLEANS: u16 = 0x01BF;
const LINE_COLOR: u16 = 0x01C0;
const LINE_BOOLEANS: u16 = 0x01FF;

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

/// An OfficeArtFOPT with simple (non-complex) properties.
fn fopt(props: &[(u16, u32)]) -> Vec<u8> {
    let body: Vec<u8> = props.iter().flat_map(|(id, op)| cat(&[&id.to_le_bytes(), &op.to_le_bytes()])).collect();
    art(3, props.len() as u16, 0xF00B, &body)
}

/// A shape of type `spt` with Obj kind `ot`, anchored B2:D5, its FOPT, then (when `text` is
/// given) the text-box client data and a TxO whose single run uses font index `ifnt`.
fn shape(spt: u16, ot: u16, props: &[(u16, u32)], text: Option<(&str, u16)>) -> Vec<u8> {
    let sp = art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 0, 1, 0, 3, 0, 4, 0]));
    let data = art(0, 0, 0xF011, &[]);
    let mut out = cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &fopt(props), &anchor, &data]))), &obj(ot)]);
    if let Some((text, ifnt)) = text {
        out.extend(rec(0x00EC, &art(0, 0, 0xF00D, &[])));
        out.extend(txo(text, ifnt));
    }
    out
}

/// A TxO holding `text` (8-bit characters): a run at character 0 with font `ifnt`, then the
/// terminating run.
fn txo(text: &str, ifnt: u16) -> Vec<u8> {
    let cch = text.len() as u16;
    let mut t = vec![0u8; 18];
    t[10..12].copy_from_slice(&cch.to_le_bytes());
    t[12..14].copy_from_slice(&16u16.to_le_bytes());
    let mut chars = vec![0u8];
    chars.extend(text.as_bytes());
    let runs = cat(&[&u16s(&[0, ifnt, 0, 0]), &u16s(&[cch, 0, 0, 0])]);
    cat(&[&rec(0x01B6, &t), &rec(0x003C, &chars), &rec(0x003C, &runs)])
}

/// A BIFF8 FONT record: height in twips, grbit, icv, bls, then the name (8-bit characters).
fn font(twips: u16, grbit: u16, icv: u16, bls: u16, name: &str) -> Vec<u8> {
    rec(0x0031, &cat(&[&u16s(&[twips, grbit, icv, bls, 0]), &[0, 0, 0, 0], &[name.len() as u8, 0], name.as_bytes()]))
}

/// Fonts 0-3 and 5-6 (index 4 is never written): Calibri, Arial (index 1), Calibri, Calibri,
/// Times New Roman 10 pt bold italic red (index 5), Courier New.
fn fonts() -> Vec<u8> {
    cat(&[
        &font(220, 0, 0x7FFF, 400, "Calibri"),
        &font(240, 0, 0x7FFF, 400, "Arial"),
        &font(220, 0, 0x7FFF, 400, "Calibri"),
        &font(220, 0, 0x7FFF, 400, "Calibri"),
        &font(200, 0x0002, 10, 700, "Times New Roman"),
        &font(180, 0, 0x7FFF, 400, "Courier New"),
    ])
}

fn xls(records: &[u8]) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), records, &eof()]);
    let globals = |pos: u32| cat(&[&bof(0x0005), &fonts(), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}

fn one_shape(records: &[u8]) -> Shape {
    let (wb, report) = read_xls(&xls(records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    shapes[0].clone()
}

const TEXT_BOX: (u16, u16) = (202, 0x0006);

#[test]
fn a_filled_text_box_keeps_its_fill_and_outline_colours() {
    // fillColor 0x0099FFFF is red FF, green FF, blue 99: the light yellow FFFF99.
    let props = [(FILL_COLOR, 0x0099_FFFF), (FILL_BOOLEANS, 0x0010_0010), (LINE_COLOR, 0), (LINE_BOOLEANS, 0x0018_0008)];
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &props, Some(("Note", 0))));
    assert_eq!(s.kind, ShapeKind::TextBox);
    assert_eq!((s.fill, s.line), (Color::Rgb(0xFFFF99), Color::Rgb(0)));
    assert!(!s.format.no_fill && !s.format.no_line, "{:?}", s.format);
    assert_eq!(s.text, "Note");
}

#[test]
fn a_text_box_without_fill_or_line_says_so() {
    let props = [(FILL_COLOR, 0x00FF_FFFF), (FILL_BOOLEANS, 0x0010_0000), (LINE_BOOLEANS, 0x0008_0000)];
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &props, Some(("Clear", 0))));
    assert!(s.format.no_fill, "{:?}", s.format);
    assert!(s.format.no_line, "{:?}", s.format);
}

#[test]
fn a_text_box_without_colour_properties_keeps_automatic_colours() {
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &[(0x0080, 0)], Some(("Plain", 0))));
    assert_eq!((s.fill, s.line), (Color::Auto, Color::Auto));
    assert!(!s.format.no_fill && !s.format.no_line, "{:?}", s.format);
}

#[test]
fn a_palette_fill_colour_resolves_through_the_colour_table() {
    // fSchemeIndex (flags 0x08) with icv 13: the default palette's yellow.
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &[(FILL_COLOR, 0x0800_000D)], Some(("Pal", 0))));
    assert_eq!(s.fill, Color::Rgb(0xFFFF00));
}

#[test]
fn the_first_run_font_above_index_four_is_the_record_before_it() {
    // ifnt 5 is the fifth FONT record (index 4 is skipped): Times New Roman 10 pt, bold
    // (bls 700), italic, icv 10 (red in the default palette).
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &[], Some(("Styled", 5))));
    assert_eq!(s.format.font.as_deref(), Some("Times New Roman"));
    assert_eq!(s.format.font_size, Some(10.0));
    assert!(s.format.bold && s.format.italic, "{:?}", s.format);
    assert_eq!(s.format.text_color, Some(Color::Rgb(0xFF0000)));
}

#[test]
fn a_run_font_below_index_four_is_the_record_itself() {
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &[], Some(("Arial", 1))));
    assert_eq!(s.format.font.as_deref(), Some("Arial"));
    assert_eq!(s.format.font_size, Some(12.0));
    assert!(!s.format.bold && !s.format.italic, "{:?}", s.format);
    // icv 0x7FFF is the automatic colour.
    assert_eq!(s.format.text_color, None);
}

#[test]
fn a_drawn_shape_with_text_takes_its_run_font_too() {
    let s = one_shape(&shape(1, 0x0002, &[(FILL_COLOR, 0x0000_00FF)], Some(("Box", 5))));
    assert_eq!(s.kind, ShapeKind::Rectangle);
    assert_eq!(s.fill, Color::Rgb(0xFF0000));
    assert_eq!(s.format.font.as_deref(), Some("Times New Roman"));
    assert_eq!(s.format.font_size, Some(10.0));
}

#[test]
fn an_out_of_range_run_font_sets_nothing() {
    let s = one_shape(&shape(TEXT_BOX.0, TEXT_BOX.1, &[], Some(("Odd", 40))));
    assert_eq!(s.text, "Odd");
    assert_eq!((s.format.font.as_deref(), s.format.font_size), (None, None));
}
