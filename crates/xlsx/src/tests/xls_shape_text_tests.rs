//! `.xls` drawn shapes with text, in the record order Excel writes: the shape's MsoDrawing, its
//! Obj, a second MsoDrawing holding the text-box client data, then the TxO.

use gridcraft_model::ShapeKind;

use crate::read_xls;

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

/// Shape type `spt` anchored from `(col_l, row_t)` to `(col_r, row_b)`, its Obj of kind `ot`,
/// the text-box client data and (when given) a TxO holding `text`.
fn shape(spt: u16, ot: u16, (col_l, row_t, col_r, row_b): (u16, u16, u16, u16), text: Option<&str>) -> Vec<u8> {
    let sp = art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, col_l, 0, row_t, 0, col_r, 0, row_b, 0]));
    let data = art(0, 0, 0xF011, &[]);
    let mut out = cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &anchor, &data]))), &obj(ot)]);
    if let Some(text) = text {
        out.extend(rec(0x00EC, &art(0, 0, 0xF00D, &[])));
        out.extend(txo(text));
    }
    out
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

#[test]
fn shape_text_after_the_text_box_drawing_record_is_kept() {
    let records =
        cat(&[&shape(1, 0x0002, (1, 1, 3, 4), Some("Total")), &shape(3, 0x0003, (4, 1, 6, 4), None), &shape(1, 0x001E, (7, 1, 9, 4), Some("Notes"))]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    let got: Vec<(ShapeKind, &str)> = shapes.iter().map(|s| (s.kind, s.text.as_str())).collect();
    assert_eq!(got, [(ShapeKind::Rectangle, "Total"), (ShapeKind::Ellipse, ""), (ShapeKind::Rectangle, "Notes")], "{:?}", report.warnings);
}

#[test]
fn a_txo_after_a_line_is_ignored() {
    let records = cat(&[&shape(20, 0x0001, (0, 0, 3, 3), Some("x")), &shape(1, 0x0002, (4, 0, 6, 2), None)]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    let got: Vec<(ShapeKind, &str)> = shapes.iter().map(|s| (s.kind, s.text.as_str())).collect();
    assert_eq!(got, [(ShapeKind::Line, ""), (ShapeKind::Rectangle, "")]);
}
