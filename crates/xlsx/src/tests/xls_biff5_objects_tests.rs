//! Excel 5.0/95 (BIFF5) drawing objects: text boxes and rectangles live in OBJ records whose
//! fixed header carries the anchor. Every workbook is assembled here from the BIFF5 record
//! layouts in a `Book` stream; no Excel-produced file is used.

use gridcraft_core::CellRef;
use gridcraft_model::ShapeKind;

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
fn book(records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 5], b"Sheet"].concat());
    let head = [bof(0x0500, 0x0005), rec(0x0042, &1252u16.to_le_bytes())].concat();
    let pos = (head.len() + boundsheet(0).len() + eof().len()) as u32;
    let globals = [head, boundsheet(pos), eof()].concat();
    let body = [bof(0x0500, 0x0010), records.to_vec(), eof()].concat();
    compound("Book", &[globals, body].concat())
}

/// The 34-byte BIFF5 OBJ header: cObj, ot, id, grbit, the anchor
/// (colL, dxL, rwT, dyT, colR, dxR, rwB, dyB), cbMacro 0 and 6 reserved bytes.
fn header(ot: u16, id: u16, anchor: [u16; 8]) -> Vec<u8> {
    [&1u32.to_le_bytes()[..], &u16s(&[ot, id, 0x0616]), &u16s(&anchor), &[0; 8]].concat()
}

/// A BIFF5 text box OBJ (ot 6): header, 10 colour/line bytes, cchText, reserved, cbRuns,
/// 20 reserved bytes, the text, a pad byte when the text length is odd, then the runs.
fn text_box(id: u16, anchor: [u16; 8], text: &[u8], runs: &[u8]) -> Vec<u8> {
    let mut p = header(6, id, anchor);
    p.extend_from_slice(&[0x08, 0x09, 0x01, 0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x00]);
    p.extend(u16s(&[text.len() as u16, 0x4FAF, runs.len() as u16]));
    p.extend_from_slice(&[0; 20]);
    assert_eq!(p.len(), 70);
    p.extend_from_slice(text);
    if text.len() % 2 == 1 {
        p.push(0);
    }
    p.extend_from_slice(runs);
    rec(0x005D, &p)
}

/// Two formatting runs (ich, ifnt, 4 reserved bytes each), as Excel 95 writes them.
const RUNS: [u8; 16] = [0, 0, 0x11, 0, 0xD7, 0, 0x2E, 0, 0x0A, 0, 0, 0, 0x0E, 0, 0x02, 0x52];

fn warned(report: &crate::ReadReport) -> bool {
    report.warnings.iter().any(|w| w.contains(SHAPES_WARNING))
}

#[test]
fn text_box_opens_with_its_text_at_its_place() {
    // Odd text length: a pad byte comes before the runs.
    let tb = text_box(1, [3, 561, 22, 31, 5, 369, 27, 10], b"Energy\nProduction\n\n(fuel)", &RUNS);
    let (wb, report) = read_xls(&book(&tb)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert_eq!(shapes[0].kind, ShapeKind::TextBox);
    assert_eq!(shapes[0].text, "Energy\nProduction\n\n(fuel)");
    assert_eq!(shapes[0].anchor.cell, CellRef::new(22, 3), "top-left cell is (rwT, colL)");
    assert!(shapes[0].anchor.dx > 0.0 && shapes[0].anchor.dy > 0.0, "{:?}", shapes[0].anchor);
    assert!(shapes[0].anchor.width > 0.0 && shapes[0].anchor.height > 0.0, "{:?}", shapes[0].anchor);
    assert!(!warned(&report), "{:?}", report.warnings);
}

#[test]
fn text_is_decoded_in_the_workbook_code_page() {
    // 0xB7 is a middle dot in code page 1252; even length, so no pad byte.
    let tb = text_box(1, [0, 0, 1, 0, 2, 0, 4, 0], b"  \xB7 Excise", &RUNS);
    let (wb, _) = read_xls(&book(&tb)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().shapes[0].text, "  \u{B7} Excise");
}

#[test]
fn several_text_boxes_keep_their_order_and_places() {
    let recs = [
        text_box(1, [1, 0, 2, 0, 3, 0, 4, 0], b"one", &RUNS),
        text_box(2, [4, 0, 7, 0, 6, 0, 9, 0], b"", &[]),
        text_box(3, [0, 0, 10, 0, 2, 0, 12, 0], b"three!", &[RUNS, RUNS].concat()),
    ]
    .concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    let got: Vec<_> = shapes.iter().map(|s| (s.kind, s.text.as_str(), s.anchor.cell)).collect();
    assert_eq!(
        got,
        [
            (ShapeKind::TextBox, "one", CellRef::new(2, 1)),
            (ShapeKind::TextBox, "", CellRef::new(7, 4)),
            (ShapeKind::TextBox, "three!", CellRef::new(10, 0)),
        ]
    );
    assert!(!warned(&report), "{:?}", report.warnings);
}

#[test]
fn rectangle_opens_as_a_shape() {
    let mut p = header(2, 1, [0, 683, 22, 15, 6, 171, 28, 15]);
    p.extend_from_slice(&[0x41, 0x40, 0x00, 0x00, 0x40, 0x00, 0x00, 0x01, 0x02, 0x00]);
    assert_eq!(p.len(), 44);
    let (wb, report) = read_xls(&book(&rec(0x005D, &p))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert_eq!((shapes[0].kind, shapes[0].text.as_str()), (ShapeKind::Rectangle, ""));
    assert_eq!(shapes[0].anchor.cell, CellRef::new(22, 0));
    assert!(!warned(&report), "{:?}", report.warnings);
}

#[test]
fn text_running_past_the_record_end_is_cut_not_a_panic() {
    let mut tb = text_box(1, [0, 0, 0, 0, 2, 0, 2, 0], b"abcdef", &[]);
    // Claim 40 characters but keep only the 6 that are there.
    tb[4 + 44] = 40;
    let (wb, report) = read_xls(&book(&tb)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert_eq!(shapes[0].text, "abcdef");
    assert!(report.warnings.iter().any(|w| w.contains("cut off")), "{:?}", report.warnings);
}

#[test]
fn short_or_garbled_obj_records_never_panic() {
    let full = text_box(1, [0, 0, 0, 0, 2, 0, 2, 0], b"hello", &RUNS);
    for cut in [4usize, 10, 20, 36, 40, 50, 69, 70, 72] {
        let mut r = full[..4 + cut].to_vec();
        r[2..4].copy_from_slice(&(cut as u16).to_le_bytes());
        let _ = read_xls(&book(&r));
    }
    // Other objects (a line, ot 1) keep today's warning.
    let mut line = header(1, 1, [0, 0, 0, 0, 2, 0, 2, 0]);
    line.extend_from_slice(&[0x08, 0x00, 0x01, 0x00, 0x10, 0x01, 0x00, 0x00]);
    let (wb, report) = read_xls(&book(&rec(0x005D, &line))).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
    assert!(warned(&report), "{:?}", report.warnings);
}
