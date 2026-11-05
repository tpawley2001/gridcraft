//! `.xls` drawn shapes ([MS-XLS] 2.4.181 Obj, [MS-ODRAW] 2.2.40 OfficeArtFSP and the fill and
//! line properties): rectangles, ovals, lines, arrows and Office Art shapes open as shapes with
//! their colours, outline-only shapes stay unfilled through Save As `.xlsx`, and form controls
//! are reported as form controls rather than shapes.

use gridcraft_core::CellRef;
use gridcraft_model::{Color, ShapeKind};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

/// An MsoDrawing shape of OfficeArt shape type `spt`, anchored from `(col_l, row_t)` to
/// `(col_r, row_b)`, with simple FOPT properties `(id, value)`.
fn shape(spt: u16, props: &[(u16, u32)], col_l: u16, row_t: u16, col_r: u16, row_b: u16) -> Vec<u8> {
    let sp = art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let mut body = vec![];
    for (id, value) in props {
        body.extend(u16s(&[*id]));
        body.extend(value.to_le_bytes());
    }
    let fopt = art(3, props.len() as u16, 0xF00B, &body);
    let anchor = art(0, 0, 0xF010, &u16s(&[0, col_l, 0, row_t, 0, col_r, 0, row_b, 0]));
    let data = art(0, 0, 0xF011, &[]);
    cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &fopt, &anchor, &data]))), &rec(0x00EC, &art(0, 0, 0xF00D, &[]))])
}

/// A TxO holding `text` (8-bit characters) with one terminating run.
fn txo(text: &str) -> Vec<u8> {
    let cch = text.len() as u16;
    let mut t = vec![0u8; 18];
    t[10..12].copy_from_slice(&cch.to_le_bytes());
    t[12..14].copy_from_slice(&16u16.to_le_bytes());
    let mut chars = vec![0u8];
    chars.extend(text.as_bytes());
    let mut runs = vec![0u8; 16];
    runs[8..10].copy_from_slice(&cch.to_le_bytes());
    cat(&[&rec(0x01B6, &t), &rec(0x003C, &chars), &rec(0x003C, &runs)])
}

fn xls(records: &[u8]) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), records, &eof()]);
    let globals = |pos: u32| cat(&[&bof(0x0005), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}

// OfficeArt property ids.
const FILL_COLOR: u16 = 0x0181;
const FILL_BOOLEANS: u16 = 0x01BF;
const LINE_COLOR: u16 = 0x01C0;
const LINE_END_ARROWHEAD: u16 = 0x01D1;
const LINE_BOOLEANS: u16 = 0x01FF;

// Obj kinds (FtCmo ot).
const OT_LINE: u16 = 0x0001;
const OT_RECTANGLE: u16 = 0x0002;
const OT_OVAL: u16 = 0x0003;
const OT_BUTTON: u16 = 0x0007;
const OT_DROP_DOWN: u16 = 0x0014;
const OT_OFFICE_ART: u16 = 0x001E;

/// Values measured on real files: an outline-only oval (fUsefFilled set, fFilled clear) with
/// its line drawn (fUsefLine and fLine set).
const NOT_FILLED: u32 = 0x001E_000C;
const LINE_DRAWN: u32 = 0x000E_000E;

#[test]
fn a_rectangle_opens_as_a_shape_with_its_rgb_fill_and_palette_line() {
    // Fill: OfficeArtCOLORREF red 0x44, green 0x88, blue 0xCC, no flags. Line: fSchemeIndex,
    // palette index 10 (red in the BIFF8 default palette).
    let props = [(FILL_COLOR, 0x00CC_8844), (LINE_COLOR, 0x0800_000A)];
    let drawing = cat(&[&shape(1, &props, 1, 2, 4, 6), &obj(OT_RECTANGLE)]);
    let (wb, report) = read_xls(&xls(&drawing)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    let s = &shapes[0];
    assert_eq!(s.kind, ShapeKind::Rectangle);
    assert_eq!(s.anchor.cell, CellRef::new(2, 1));
    assert_eq!(s.fill, Color::Rgb(0x4488CC));
    assert_eq!(s.line, Color::Rgb(0xFF0000));
    assert!(!s.format.no_fill && !s.format.no_line);
    assert!(!report.warnings.iter().any(|w| w.contains("Shapes in .xls")), "{:?}", report.warnings);
}

#[test]
fn an_outline_only_oval_stays_unfilled_through_save_as_xlsx() {
    // Line colour: palette index 12 (blue in the BIFF8 default palette).
    let props = [(FILL_COLOR, 0x0800_0050), (FILL_BOOLEANS, NOT_FILLED), (LINE_COLOR, 0x0800_000C), (LINE_BOOLEANS, LINE_DRAWN)];
    let drawing = cat(&[&shape(3, &props, 0, 1, 2, 3), &obj(OT_OVAL)]);
    let (wb, _) = read_xls(&xls(&drawing)).unwrap();
    let s = &wb.sheet(0).unwrap().shapes[0];
    assert_eq!(s.kind, ShapeKind::Ellipse);
    assert!(s.format.no_fill, "fFilled is clear: the oval only circles the cells");
    assert!(!s.format.no_line);
    assert_eq!(s.line, Color::Rgb(0x0000FF));

    let out = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&out).unwrap();
    let s = &back.sheet(0).unwrap().shapes[0];
    assert_eq!(s.kind, ShapeKind::Ellipse);
    assert!(s.format.no_fill, "a:noFill is written and read back");
    assert!(!s.format.no_line);
}

#[test]
fn lines_and_arrows_open_unfilled() {
    let line = cat(&[&shape(20, &[(LINE_COLOR, 0x0800_0040)], 0, 0, 3, 3), &obj(OT_LINE)]);
    let arrow = cat(&[&shape(20, &[(LINE_END_ARROWHEAD, 1)], 4, 0, 6, 2), &obj(OT_LINE)]);
    let (wb, _) = read_xls(&xls(&cat(&[&line, &arrow]))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    let kinds: Vec<ShapeKind> = shapes.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [ShapeKind::Line, ShapeKind::Arrow], "file order");
    assert_eq!(shapes[0].line, Color::Auto, "palette index 0x40 is the system window-text colour");
    assert!(shapes.iter().all(|s| s.format.no_fill));
}

#[test]
fn a_rectangle_keeps_the_text_of_its_txo() {
    let drawing = cat(&[&shape(1, &[], 1, 1, 3, 4), &obj(OT_RECTANGLE), &txo("Total")]);
    let (wb, _) = read_xls(&xls(&drawing)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!((shapes[0].kind, shapes[0].text.as_str()), (ShapeKind::Rectangle, "Total"));
}

#[test]
fn office_art_objects_take_their_kind_from_the_shape_type() {
    // A triangle (shape type 5), then a rectangle (1) whose fill is a system colour.
    let triangle = cat(&[&shape(5, &[], 0, 0, 2, 2), &obj(OT_OFFICE_ART)]);
    let rect = cat(&[&shape(1, &[(FILL_COLOR, 0x0800_0041)], 3, 0, 5, 2), &obj(OT_OFFICE_ART)]);
    let (wb, _) = read_xls(&xls(&cat(&[&triangle, &rect]))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    let kinds: Vec<ShapeKind> = shapes.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [ShapeKind::Triangle, ShapeKind::Rectangle]);
    assert_eq!(shapes[1].fill, Color::Auto, "palette index 0x41 is the system window colour");
}

#[test]
fn form_controls_are_reported_as_form_controls_not_shapes() {
    let button = cat(&[&shape(201, &[], 0, 0, 2, 1), &obj(OT_BUTTON)]);
    let drop_down = cat(&[&shape(201, &[], 3, 0, 4, 1), &obj(OT_DROP_DOWN)]);
    let (wb, report) = read_xls(&xls(&cat(&[&button, &drop_down]))).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("Form controls in .xls files are not imported yet.")), "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("Shapes in .xls")), "{:?}", report.warnings);
}

#[test]
fn a_shape_obj_without_a_drawing_record_is_skipped() {
    let (wb, _) = read_xls(&xls(&obj(OT_RECTANGLE))).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
}
