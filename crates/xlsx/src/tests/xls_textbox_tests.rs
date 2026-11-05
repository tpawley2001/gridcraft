//! `.xls` text boxes ([MS-XLS] 2.4.329 TxO, 2.5.195 OfficeArtClientTextbox): text split over
//! `Continue` records, malformed and truncated runs, several objects on one sheet and form
//! controls that carry a TxO but are not text boxes.

use gridcraft_core::CellRef;
use gridcraft_model::{ShapeKind, ShapeTextAlign, ShapeTextAnchor};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

/// The drawing shape of a text box, its anchor (from `(colL, rwT)` to `(colR, rwB)`), and its
/// OfficeArtClientTextbox.
fn shape(col_l: u16, row_t: u16, col_r: u16, row_b: u16) -> Vec<u8> {
    let sp = art(2, 202, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, col_l, 0, row_t, 0, col_r, 0, row_b, 0]));
    let data = art(0, 0, 0xF011, &[]);
    cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &anchor, &data]))), &rec(0x00EC, &art(0, 0, 0xF00D, &[]))])
}

/// A text box shape whose FOPT carries a name and the hidden flag.
fn named_hidden_shape(col_l: u16, row_t: u16, col_r: u16, row_b: u16, name: &str) -> Vec<u8> {
    let sp = art(2, 202, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let mut units: Vec<u8> = name.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    units.extend_from_slice(&[0, 0]);
    // Two properties: the complex name, then GroupShapeBooleanProperties with fHidden|fUsefHidden.
    let props = cat(&[&u16s(&[0x8000 | 0x0380]), &(units.len() as u32).to_le_bytes(), &u16s(&[0x03BF]), &0x0002_0002u32.to_le_bytes(), &units]);
    let fopt = art(2, 2, 0xF00B, &props);
    let anchor = art(0, 0, 0xF010, &u16s(&[0, col_l, 0, row_t, 0, col_r, 0, row_b, 0]));
    let data = art(0, 0, 0xF011, &[]);
    cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &fopt, &anchor, &data]))), &rec(0x00EC, &art(0, 0, 0xF00D, &[]))])
}

/// A TxO with `cch` characters and `cb_runs` bytes of runs, then the given `Continue` parts.
fn txo(cch: u16, cb_runs: u16, flags: u16, rot: u16, parts: &[&[u8]]) -> Vec<u8> {
    let mut t = vec![0u8; 18];
    t[0..2].copy_from_slice(&flags.to_le_bytes());
    t[2..4].copy_from_slice(&rot.to_le_bytes());
    t[10..12].copy_from_slice(&cch.to_le_bytes());
    t[12..14].copy_from_slice(&cb_runs.to_le_bytes());
    let mut v = rec(0x01B6, &t);
    for p in parts {
        v.extend(rec(0x003C, p));
    }
    v
}

/// A terminator run: the minimum TxORuns, its one run covering `cch` characters.
fn runs(cch: u16) -> Vec<u8> {
    let mut r = vec![0u8; 16];
    r[8..10].copy_from_slice(&cch.to_le_bytes());
    r
}

fn xls(records: &[u8]) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), records, &eof()]);
    let globals = |pos: u32| cat(&[&bof(0x0005), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}

#[test]
fn a_text_box_cut_off_by_its_continue_records_keeps_the_text_it_read() {
    let drawing = cat(&[&shape(1, 2, 3, 6), &obj(6)]);
    let text = txo(5, 16, 0, 0, &[&[0, b'a', b'b'], &[0], &runs(5)]);
    let (wb, report) = read_xls(&xls(&cat(&[&drawing, &text]))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].text, "ab", "only the characters that were there");
    assert!(report.warnings.iter().any(|w| w.contains("cut off")), "{:?}", report.warnings);
}

#[test]
fn malformed_runs_are_reported_and_never_read_as_text() {
    // cbRuns 10 is neither 16 nor a multiple of 8; the bytes after the text are its runs.
    let drawing = cat(&[&shape(1, 2, 3, 6), &obj(6)]);
    let text = txo(2, 10, 0, 0, &[&[0, b'h', b'i'], &[b'\0'; 10]]);
    let (wb, report) = read_xls(&xls(&cat(&[&drawing, &text]))).unwrap();
    assert_eq!(wb.sheet(0).unwrap().shapes[0].text, "hi");
    assert!(report.warnings.iter().any(|w| w.contains("malformed formatting runs")), "{:?}", report.warnings);
}

#[test]
fn an_empty_text_box_is_a_shape_with_no_text() {
    let drawing = cat(&[&shape(1, 2, 3, 6), &obj(6)]);
    let text = txo(0, 0, 0, 0, &[]);
    let (wb, _) = read_xls(&xls(&cat(&[&drawing, &text]))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!((shapes[0].kind, shapes[0].text.as_str()), (ShapeKind::TextBox, ""));
}

#[test]
fn multi_line_text_keeps_every_line() {
    let drawing = cat(&[&shape(1, 2, 3, 6), &obj(6)]);
    let text = "first\nsecond";
    let mut chars = vec![0u8];
    chars.extend(text.as_bytes());
    let text = txo(12, 16, 0, 0, &[&chars, &runs(12)]);
    let (wb, _) = read_xls(&xls(&cat(&[&drawing, &text]))).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes[0].text, "first\nsecond");
}

#[test]
fn a_later_text_box_does_not_take_an_earlier_picture_s_text() {
    // A picture, then a text box: the picture has no text, and the box keeps its own.
    let picture = cat(&[&shape(0, 0, 2, 4), &obj(8), &shape(1, 2, 3, 6), &obj(6), &txo(3, 16, 0, 0, &[&[0, b'b', b'o', b'x'], &runs(3)])]);
    let (wb, _) = read_xls(&xls(&picture)).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.shapes.len(), 1, "only the text box is a shape");
    assert_eq!(sheet.shapes[0].text, "box");
    // The picture's blip is missing from the store, so no picture is imported; the box's
    // anchor is its own.
    assert_eq!(sheet.shapes[0].anchor.cell, CellRef::new(2, 1));
}

#[test]
fn two_text_boxes_keep_their_own_text() {
    let records = cat(&[
        &shape(1, 2, 3, 6),
        &obj(6),
        &txo(3, 16, 0, 0, &[&[0, b'o', b'n', b'e'], &runs(3)]),
        &shape(4, 7, 6, 9),
        &obj(6),
        &txo(3, 16, 0, 0, &[&[0, b't', b'w', b'o'], &runs(3)]),
    ]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 2);
    assert_eq!((shapes[0].text.as_str(), shapes[0].anchor.cell), ("one", CellRef::new(2, 1)));
    assert_eq!((shapes[1].text.as_str(), shapes[1].anchor.cell), ("two", CellRef::new(7, 4)));
}

#[test]
fn a_form_control_s_txo_is_not_a_text_box() {
    // An edit box (Obj `ot` 12) also carries a TxO; it is not imported as a shape.
    let records = cat(&[&shape(1, 2, 3, 6), &obj(12), &txo(4, 16, 0, 0, &[&[0, b'e', b'd', b'i', b't'], &runs(4)])]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
}

#[test]
fn a_txo_without_its_obj_record_is_ignored() {
    let records = txo(2, 16, 0, 0, &[&[0, b'n', b'o'], &runs(2)]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
}

#[test]
fn alignment_and_hidden_state_are_kept_in_the_saved_drawing() {
    // hAlignment 2 (centred), vAlignment 3 (bottom).
    let flags = (2 << 1) | (3 << 4);
    let records = cat(&[&shape(1, 2, 3, 6), &obj(6), &txo(2, 16, flags, 0, &[&[0, b'o', b'k'], &runs(2)])]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shape = &wb.sheet(0).unwrap().shapes[0];
    assert_eq!(shape.format.align, Some(ShapeTextAlign::Center));
    assert_eq!(shape.format.anchor, Some(ShapeTextAnchor::Bottom));
    let out = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&out).unwrap();
    let shape = &back.sheet(0).unwrap().shapes[0];
    assert_eq!((shape.kind, shape.text.as_str()), (ShapeKind::TextBox, "ok"));
    assert_eq!(shape.format.align, Some(ShapeTextAlign::Center));
    assert_eq!(shape.format.anchor, Some(ShapeTextAnchor::Bottom));
}

#[test]
fn rotation_is_reported_and_the_text_is_kept() {
    let records = cat(&[&shape(1, 2, 3, 6), &obj(6), &txo(2, 16, 0, 2, &[&[0, b'u', b'p'], &runs(2)])]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().shapes[0].text, "up");
    assert!(report.warnings.iter().any(|w| w.contains("rotation of text boxes")), "{:?}", report.warnings);
}

#[test]
fn a_named_hidden_text_box_keeps_its_name_and_hidden_state() {
    let records = cat(&[&named_hidden_shape(1, 2, 3, 6, "Caption"), &obj(6), &txo(3, 16, 0, 0, &[&[0, b'h', b'i', b'!'], &runs(3)])]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shape = &wb.sheet(0).unwrap().shapes[0];
    assert_eq!(shape.name.as_deref(), Some("Caption"));
    assert!(shape.hidden);
    let out = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&out).unwrap();
    let shape = &back.sheet(0).unwrap().shapes[0];
    assert_eq!(shape.name.as_deref(), Some("Caption"));
    assert!(shape.hidden);
    assert_eq!(shape.text, "hi!");
}

#[test]
fn imported_text_boxes_get_unique_nonzero_ids() {
    // 0 is never an object's id: a shape the model can select and edit needs a real one.
    let records = cat(&[
        &shape(1, 2, 3, 6),
        &obj(6),
        &txo(3, 16, 0, 0, &[&[0, b'o', b'n', b'e'], &runs(3)]),
        &shape(4, 7, 6, 9),
        &obj(6),
        &txo(3, 16, 0, 0, &[&[0, b't', b'w', b'o'], &runs(3)]),
    ]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert!(shapes.iter().all(|s| s.id != 0), "{:?}", shapes.iter().map(|s| s.id).collect::<Vec<_>>());
    assert_ne!(shapes[0].id, shapes[1].id);
}

#[test]
fn a_truncated_txo_record_is_reported_and_skipped() {
    let records = cat(&[&shape(1, 2, 3, 6), &obj(6), &rec(0x01B6, &[0, 0, 0, 0])]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("damaged")), "{:?}", report.warnings);
}
