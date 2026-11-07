//! Excel 5.0/95 (BIFF5) `.xls` cell formatting: Font (0x0031, 8-bit name) and the 16-byte XF
//! (0x00E0) with alignment, fill and borders. Every workbook is assembled here from the BIFF5
//! record layouts; no Excel-produced file is used.

use gridcraft_core::CellRef;
use gridcraft_model::{BorderLine, BorderStyle, Color, Fill, HAlign, Style, VAlign, Workbook};

use super::xls_tests::compound;
use crate::read_xls;

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[0x0500, dt, 0x0DBB, 0x07CC]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

fn number(r: u16, c: u16, xf: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&u16s(&[r, c, xf]), &n.to_le_bytes()]))
}

/// A BIFF5 Font: the BIFF8 fixed part, then a 1-byte count and 8-bit characters.
fn font(twips: u16, italic: bool, icv: u16, weight: u16, name: &[u8]) -> Vec<u8> {
    let grbit = if italic { 0x0002 } else { 0 };
    rec(0x0031, &cat(&[&u16s(&[twips, grbit, icv, weight, 0]), &[0, 0, 0, 0], &[name.len() as u8], name]))
}

/// The fields of a BIFF5 XF record.
#[derive(Clone, Copy)]
struct Xf5 {
    ifnt: u16,
    ifmt: u16,
    flags: u16,
    alc: u16,
    wrap: bool,
    alcv: u16,
    fore: u32,
    back: u32,
    fls: u32,
    /// (style, colour) of top, left, right and bottom.
    top: (u32, u32),
    left: (u32, u32),
    right: (u32, u32),
    bottom: (u32, u32),
}

impl Default for Xf5 {
    fn default() -> Self {
        // A plain cell XF as Excel 5 writes it: locked, bottom-aligned, system colours.
        Xf5 {
            ifnt: 0,
            ifmt: 0,
            flags: 0x0001,
            alc: 0,
            wrap: false,
            alcv: 2,
            fore: 0x40,
            back: 0x41,
            fls: 0,
            top: (0, 0),
            left: (0, 0),
            right: (0, 0),
            bottom: (0, 0),
        }
    }
}

fn xf(x: Xf5) -> Vec<u8> {
    let align = x.alc | if x.wrap { 0x08 } else { 0 } | (x.alcv << 4);
    let d8 = x.fore | (x.back << 7) | (x.fls << 16) | (x.bottom.0 << 22) | (x.bottom.1 << 25);
    let d12 = x.top.0 | (x.left.0 << 3) | (x.right.0 << 6) | (x.top.1 << 9) | (x.left.1 << 16) | (x.right.1 << 23);
    rec(0x00E0, &cat(&[&u16s(&[x.ifnt, x.ifmt, x.flags, align]), &d8.to_le_bytes(), &d12.to_le_bytes()]))
}

/// Palette for colour indexes 8-12: black, white, red, green and a custom 0x123456.
fn palette() -> Vec<u8> {
    let colors: [[u8; 4]; 5] = [[0, 0, 0, 0], [255, 255, 255, 0], [255, 0, 0, 0], [0, 255, 0, 0], [0x12, 0x34, 0x56, 0]];
    rec(0x0092, &cat(&[&5u16.to_le_bytes(), &colors.concat()]))
}

/// Five fonts (index 4 is skipped by XFs, so XF ifnt 5 names the fifth) then the given XFs.
fn styles(xfs: &[Xf5]) -> Vec<u8> {
    let fonts = cat(&[
        &font(200, false, 0x7FFF, 400, b"Arial"),
        &font(200, false, 0x7FFF, 700, b"Arial"),
        &font(200, false, 0x7FFF, 400, b"Courier New"),
        &font(160, false, 0x7FFF, 400, b"Arial"),
        &font(240, true, 10, 400, b"Caf\xe9 Sans"),
    ]);
    let x: Vec<u8> = xfs.iter().flat_map(|x| xf(*x)).collect();
    cat(&[&palette(), &fonts, &x])
}

/// A one-sheet Book stream in code page 1252 whose cells A1, A2, ... use XF 0, 1, ...
fn book(xfs: &[Xf5]) -> Vec<u8> {
    let head = cat(&[&bof(0x0005), &rec(0x0042, &1252u16.to_le_bytes()), &styles(xfs)]);
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 5], b"Sheet"]));
    let pos = (head.len() + boundsheet(0).len() + eof().len()) as u32;
    let cells: Vec<u8> = (0..xfs.len() as u16).flat_map(|i| number(i, 0, i, 1.0)).collect();
    let sheet = cat(&[&bof(0x0010), &cells, &eof()]);
    compound("Book", &cat(&[&head, &boundsheet(pos), &eof(), &sheet]))
}

fn style_of(wb: &Workbook, a1: &str) -> Style {
    wb.styles.get(wb.sheets[0].style_id(CellRef::parse(a1).unwrap())).clone()
}

fn read(xfs: &[Xf5]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xls(&book(xfs)).unwrap();
    (wb, report.warnings)
}

#[test]
fn fonts_are_read_with_8_bit_names() {
    let (wb, w) = read(&[Xf5::default(), Xf5 { ifnt: 1, ..Xf5::default() }, Xf5 { ifnt: 5, ..Xf5::default() }]);
    let a1 = style_of(&wb, "A1");
    assert_eq!(a1.font.name, "Arial", "{w:?}");
    assert_eq!(a1.font.size, 10.0);
    assert!(!a1.font.bold);
    let a2 = style_of(&wb, "A2");
    assert_eq!(a2.font.name, "Arial");
    assert!(a2.font.bold, "weight 700 is bold");
    // FontIndex 5 is the fifth Font record; the name is decoded in the workbook's code page.
    let a3 = style_of(&wb, "A3");
    assert_eq!(a3.font.name, "Café Sans");
    assert_eq!(a3.font.size, 12.0);
    assert!(a3.font.italic);
    assert!(!a3.font.bold);
    assert_eq!(a3.font.color, Color::Rgb(0xFF0000));
}

#[test]
fn solid_fill_uses_the_foreground_colour() {
    let (wb, _) =
        read(&[Xf5::default(), Xf5 { fls: 1, fore: 13, back: 0x40, ..Xf5::default() }, Xf5 { fls: 1, fore: 12, back: 0x40, ..Xf5::default() }]);
    assert_eq!(style_of(&wb, "A1").fill, Fill::default());
    assert_eq!(style_of(&wb, "A2").fill, Fill::solid(Color::Rgb(0xFFFF00)));
    // Index 12 comes from the Palette record.
    assert_eq!(style_of(&wb, "A3").fill, Fill::solid(Color::Rgb(0x123456)));
}

#[test]
fn borders_are_read_per_side() {
    let (wb, _) = read(&[
        Xf5::default(),
        Xf5 { top: (1, 10), left: (1, 0x40), right: (3, 12), bottom: (2, 8), ..Xf5::default() },
        Xf5 { bottom: (7, 8), ..Xf5::default() },
    ]);
    let b = style_of(&wb, "A2").borders;
    assert_eq!(b.top, BorderLine { style: BorderStyle::Thin, color: Color::Rgb(0xFF0000) });
    assert_eq!(b.left, BorderLine { style: BorderStyle::Thin, color: Color::Auto });
    assert_eq!(b.right, BorderLine { style: BorderStyle::Dashed, color: Color::Rgb(0x123456) });
    assert_eq!(b.bottom, BorderLine { style: BorderStyle::Medium, color: Color::Rgb(0x000000) });
    let b3 = style_of(&wb, "A3").borders;
    assert_eq!(b3.bottom, BorderLine { style: BorderStyle::Hair, color: Color::Rgb(0x000000) });
    assert_eq!(b3.top, BorderLine::default());
    assert_eq!(style_of(&wb, "A1").borders, Default::default());
}

#[test]
fn alignment_and_protection_are_read() {
    let (wb, _) =
        read(&[Xf5::default(), Xf5 { alc: 2, wrap: true, alcv: 1, ..Xf5::default() }, Xf5 { alc: 3, alcv: 0, flags: 0x0002, ..Xf5::default() }]);
    let a2 = style_of(&wb, "A2").align;
    assert_eq!((a2.h, a2.v, a2.wrap), (HAlign::Center, VAlign::Center, true));
    let a3 = style_of(&wb, "A3");
    assert_eq!((a3.align.h, a3.align.v, a3.align.wrap), (HAlign::Right, VAlign::Top, false));
    assert!(!a3.protection.locked);
    assert!(a3.protection.hidden);
}

#[test]
fn import_warning_no_longer_lists_formatting_as_missing() {
    let (_, w) = read(&[Xf5::default()]);
    assert!(w[0].contains("Excel 5.0/95"), "{w:?}");
    assert!(w[0].contains("cell formatting"), "{w:?}");
    assert!(!w[0].contains("Fonts, fills, borders"), "{w:?}");
}

#[test]
fn default_xf_stays_plain_and_keeps_its_number_format() {
    // Guard: a default XF with a built-in format (ifmt 10 = 0.00%) is unformatted apart from it.
    let (wb, _) = read(&[Xf5 { ifmt: 10, ..Xf5::default() }]);
    let s = style_of(&wb, "A1");
    assert_eq!(s.num_fmt.as_str(), "0.00%");
    assert_eq!((s.align.h, s.align.v, s.align.wrap), (HAlign::General, VAlign::Bottom, false));
    assert_eq!(s.fill, Fill::default());
    assert!(s.protection.locked);
}
