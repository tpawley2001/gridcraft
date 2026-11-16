//! Excel 5.0/95 (BIFF5) lines: an OBJ record with `ot` 1 whose fixed header carries the anchor and
//! whose line fields (colour, style, weight, auto, arrowheads, direction) follow it. Every workbook
//! is assembled here from the BIFF5 record layouts in a `Book` stream; no Excel-produced file is used.

use gridcraft_core::CellRef;
use gridcraft_model::{Color, ShapeKind};

use super::xls_tests::compound;
use crate::read_xls;

const SHAPES_WARNING: &str = "Shapes in .xls files are not imported yet";

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(vers: u16, dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[vers, dt, 0x0DBB, 0x07CC]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

/// A Book stream: globals (CodePage 1252, one BoundSheet) and one worksheet with `records`.
pub(super) fn book(records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 5], b"Sheet"].concat());
    let head = [bof(0x0500, 0x0005), rec(0x0042, &1252u16.to_le_bytes())].concat();
    let pos = (head.len() + boundsheet(0).len() + eof().len()) as u32;
    let globals = [head, boundsheet(pos), eof()].concat();
    let body = [bof(0x0500, 0x0010), records.to_vec(), eof()].concat();
    compound("Book", &[globals, body].concat())
}

/// A 42-byte BIFF5 line OBJ: the 34-byte header (cObj, ot 1, id, grbit, the anchor colL, dxL,
/// rwT, dyT, colR, dxR, rwB, dyB, cbMacro 0, 6 reserved bytes), then icv, lns, lnw, fAuto, the
/// arrow word, iqu and a reserved byte.
pub(super) fn line(id: u16, anchor: [u16; 8], fields: [u8; 4], arrow: u16, iqu: u8) -> Vec<u8> {
    let mut p = [&1u32.to_le_bytes()[..], &u16s(&[1, id, 0x0616]), &u16s(&anchor), &[0; 8]].concat();
    p.extend_from_slice(&fields);
    p.extend_from_slice(&arrow.to_le_bytes());
    p.extend_from_slice(&[iqu, 0]);
    assert_eq!(p.len(), 42);
    rec(0x005D, &p)
}

/// Black (icv 8), solid, single weight, not automatic: the fields Excel 95 wrote for every line
/// of a tree diagram.
pub(super) const BLACK: [u8; 4] = [0x08, 0x00, 0x01, 0x00];
/// The arrow word Excel 95 wrote for a line without arrowheads.
pub(super) const NO_ARROW: u16 = 0x0110;

fn warned(report: &crate::ReadReport) -> bool {
    report.warnings.iter().any(|w| w.contains(SHAPES_WARNING))
}

#[test]
fn horizontal_line_opens_as_a_line_at_its_place() {
    let (wb, report) = read_xls(&book(&line(1, [1, 508, 12, 16, 7, 516, 12, 16], BLACK, NO_ARROW, 0))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert_eq!(shapes[0].kind, ShapeKind::Line);
    assert_eq!(shapes[0].anchor.cell, CellRef::new(12, 1), "top-left cell is (rwT, colL)");
    assert!(shapes[0].anchor.width > 0.0, "{:?}", shapes[0].anchor);
    assert_eq!(shapes[0].anchor.height, 0.0, "same row and offset top and bottom: {:?}", shapes[0].anchor);
    assert_eq!(shapes[0].line, Color::Rgb(0x000000), "icv 8 is black in the default palette");
    assert!(!shapes[0].format.no_line);
    assert!(!warned(&report), "{:?}", report.warnings);
}

#[test]
fn tree_diagram_lines_all_open_in_order() {
    // One horizontal bar and four vertical drops below it, as in an Excel 95 overview sheet.
    let recs = [
        line(1, [1, 508, 12, 16, 7, 516, 12, 16], BLACK, NO_ARROW, 0),
        line(2, [7, 508, 12, 16, 7, 508, 13, 0], BLACK, NO_ARROW, 0),
        line(3, [5, 500, 12, 16, 5, 500, 13, 0], BLACK, NO_ARROW, 0),
        line(4, [3, 492, 11, 16, 3, 492, 13, 0], BLACK, NO_ARROW, 0),
        line(5, [1, 500, 12, 16, 1, 500, 13, 0], BLACK, NO_ARROW, 0),
    ]
    .concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    let got: Vec<_> = shapes.iter().map(|s| (s.kind, s.anchor.cell)).collect();
    assert_eq!(
        got,
        [
            (ShapeKind::Line, CellRef::new(12, 1)),
            (ShapeKind::Line, CellRef::new(12, 7)),
            (ShapeKind::Line, CellRef::new(12, 5)),
            (ShapeKind::Line, CellRef::new(11, 3)),
            (ShapeKind::Line, CellRef::new(12, 1)),
        ]
    );
    for s in &shapes[1..] {
        assert_eq!(s.anchor.width, 0.0, "vertical drop: {:?}", s.anchor);
        assert!(s.anchor.height > 0.0, "{:?}", s.anchor);
    }
    assert!(!warned(&report), "{:?}", report.warnings);
}

#[test]
fn palette_colour_and_automatic_colour() {
    let recs = [
        // icv 10 is red in the default palette.
        line(1, [0, 0, 0, 0, 2, 0, 2, 0], [0x0A, 0x00, 0x01, 0x00], NO_ARROW, 0),
        // Automatic: icv 0x40 (system colour) with fAuto set, hairline.
        line(2, [0, 0, 3, 0, 2, 0, 5, 0], [0x40, 0x00, 0x00, 0x01], NO_ARROW, 0),
        // fAuto set wins over a palette icv.
        line(3, [0, 0, 6, 0, 2, 0, 8, 0], [0x0A, 0x00, 0x01, 0x01], NO_ARROW, 0),
    ]
    .concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let lines: Vec<_> = wb.sheet(0).unwrap().shapes.iter().map(|s| s.line).collect();
    assert_eq!(lines, [Color::Rgb(0xFF0000), Color::Auto, Color::Auto], "{:?}", report.warnings);
}

#[test]
fn arrowhead_makes_an_arrow_and_style_5_has_no_line() {
    let recs = [
        // Arrowhead style 1 at the end (low nibble of the arrow word).
        line(1, [0, 0, 0, 0, 2, 0, 2, 0], BLACK, NO_ARROW | 0x0001, 0),
        // lns 5: no line.
        line(2, [0, 0, 3, 0, 2, 0, 5, 0], [0x08, 0x05, 0x01, 0x00], NO_ARROW, 0),
    ]
    .concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 2, "{:?}", report.warnings);
    assert_eq!(shapes[0].kind, ShapeKind::Arrow);
    assert!(!shapes[0].format.no_line);
    assert_eq!(shapes[1].kind, ShapeKind::Line);
    assert!(shapes[1].format.no_line);
    assert!(shapes.iter().all(|s| s.format.no_fill && s.text.is_empty()), "lines are never filled and hold no text");
}

#[test]
fn diagonal_line_keeps_its_box() {
    // A diagonal joining two boxes (iqu 0, top-left to bottom-right).
    let (wb, report) = read_xls(&book(&line(23, [6, 848, 3, 30, 7, 213, 4, 105], BLACK, NO_ARROW, 0))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert_eq!((shapes[0].kind, shapes[0].anchor.cell), (ShapeKind::Line, CellRef::new(3, 6)));
    assert!(shapes[0].anchor.width > 0.0 && shapes[0].anchor.height > 0.0, "{:?}", shapes[0].anchor);
}

#[test]
fn line_record_without_its_line_fields_is_skipped_with_the_warning() {
    // Only the 34-byte header: no line fields, nothing to draw.
    let mut short = line(1, [0, 0, 0, 0, 2, 0, 2, 0], BLACK, NO_ARROW, 0);
    short.truncate(4 + 34);
    short[2..4].copy_from_slice(&34u16.to_le_bytes());
    let (wb, report) = read_xls(&book(&short)).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
    assert!(warned(&report), "{:?}", report.warnings);
}
