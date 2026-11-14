//! `.xls` drawing data that Excel writes in CONTINUE records after a TxO ([MS-XLS] 2.4.58
//! Continue, 2.4.168 MsoDrawing, 2.4.329 TxO; [MS-ODRAW] 2.2.14 OfficeArtSpContainer): when a
//! text box's TxO and its text / formatting-run CONTINUE records are followed by more CONTINUE
//! records, those hold the NEXT shape's OfficeArtSpContainer (the MsoDrawing data, continued).
//! Excel 2007+ writes .xls files this way; the shape whose Obj follows must open at its anchor.

use gridcraft_model::{ShapeKind, Sheet};

use crate::read_xls;

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;

const OT_RECTANGLE: u16 = 0x0002;
const OT_TEXT_BOX: u16 = 0x0006;
const OT_OFFICE_ART: u16 = 0x001E;

const SPT_RECTANGLE: u16 = 1;
const SPT_TEXT_BOX: u16 = 202;

const F_HAVE_ANCHOR: u32 = 0x0200;
const F_HAVE_SPT: u32 = 0x0800;

const SHAPES_WARNING: &str = "Shapes in .xls files are not imported yet";

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

fn fsp(spt: u16, flags: u32) -> Vec<u8> {
    art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &flags.to_le_bytes()]))
}

fn client_anchor(a: (u16, u16, u16, u16)) -> Vec<u8> {
    art(0, 0, 0xF010, &u16s(&[0, a.0, 0, a.1, 0, a.2, 0, a.3, 0]))
}

/// A top-level shape's OfficeArtSpContainer (shape record, client anchor, client data).
fn sp(spt: u16, a: (u16, u16, u16, u16)) -> Vec<u8> {
    art(15, 0, 0xF004, &cat(&[&fsp(spt, F_HAVE_ANCHOR | F_HAVE_SPT), &client_anchor(a), &art(0, 0, 0xF011, &[])]))
}

/// A TxO holding `text` (8-bit characters) with one terminating run: TxO, chars CONTINUE,
/// runs CONTINUE.
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

/// A text box in Excel's usual order: SpContainer MsoDrawing, Obj, ClientTextbox MsoDrawing, TxO.
fn text_box(ot: u16, a: (u16, u16, u16, u16), text: &str) -> Vec<u8> {
    cat(&[&rec(0x00EC, &sp(SPT_TEXT_BOX, a)), &obj(ot), &rec(0x00EC, &art(0, 0, 0xF00D, &[])), &txo(text)])
}

fn xls(records: &[u8]) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), records, &eof()]);
    let globals = |pos: u32| cat(&[&bof(0x0005), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}

fn open(records: &[u8]) -> (Sheet, Vec<String>) {
    let (wb, report) = read_xls(&xls(records)).unwrap();
    (wb.sheet(0).unwrap().clone(), report.warnings.clone())
}

const BOX: (u16, u16, u16, u16) = (1, 0, 4, 3);
const RECT: (u16, u16, u16, u16) = (6, 1, 8, 3);

/// The rectangle at RECT written the ordinary way (own MsoDrawing record): the reference anchor.
fn plain_rectangle() -> gridcraft_model::Shape {
    let (sheet, _) = open(&cat(&[&rec(0x00EC, &sp(SPT_RECTANGLE, RECT)), &obj(OT_RECTANGLE)]));
    assert_eq!(sheet.shapes.len(), 1);
    sheet.shapes[0].clone()
}

#[test]
fn a_shape_whose_drawing_is_in_a_continue_after_a_text_box_opens() {
    // Text box, then the rectangle's SpContainer in ONE CONTINUE record right after the TxO's
    // runs, then the rectangle's Obj.
    let records = cat(&[&text_box(OT_TEXT_BOX, BOX, "How to"), &rec(0x003C, &sp(SPT_RECTANGLE, RECT)), &obj(OT_RECTANGLE)]);
    let (sheet, warnings) = open(&records);
    assert!(!warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{warnings:?}");
    assert_eq!(sheet.shapes.len(), 2, "{warnings:?}");
    let tb = sheet.shapes.iter().find(|s| s.kind == ShapeKind::TextBox).expect("the text box");
    assert_eq!(tb.text, "How to");
    let r = sheet.shapes.iter().find(|s| s.kind == ShapeKind::Rectangle).expect("the rectangle");
    assert_eq!(r.anchor, plain_rectangle().anchor);
}

#[test]
fn a_continued_drawing_split_over_several_continue_records_opens() {
    // Excel splits a large SpContainer (a picture's, 37 KB in the corpus) over several CONTINUEs.
    let full = sp(SPT_RECTANGLE, RECT);
    let (a, b) = full.split_at(full.len() / 2);
    let (b1, b2) = b.split_at(b.len() / 2);
    let records = cat(&[&text_box(OT_TEXT_BOX, BOX, "How to"), &rec(0x003C, a), &rec(0x003C, b1), &rec(0x003C, b2), &obj(OT_RECTANGLE)]);
    let (sheet, warnings) = open(&records);
    assert!(!warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{warnings:?}");
    assert_eq!(sheet.shapes.len(), 2, "{warnings:?}");
    let r = sheet.shapes.iter().find(|s| s.kind == ShapeKind::Rectangle).expect("the rectangle");
    assert_eq!(r.anchor, plain_rectangle().anchor);
}

#[test]
fn a_second_text_box_in_a_continue_keeps_its_own_text() {
    // Office Art text box (ot 0x1E, spt 202) whose SpContainer follows the first box's TxO in a
    // CONTINUE; its own ClientTextbox MsoDrawing and TxO follow its Obj as usual.
    let second =
        cat(&[&rec(0x003C, &sp(SPT_TEXT_BOX, RECT)), &obj(OT_OFFICE_ART), &rec(0x00EC, &art(0, 0, 0xF00D, &[])), &txo("Find duplicate values")]);
    let records = cat(&[&text_box(OT_OFFICE_ART, BOX, "Home"), &second]);
    let (sheet, warnings) = open(&records);
    assert!(!warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{warnings:?}");
    let mut texts: Vec<&str> = sheet.shapes.iter().filter(|s| s.kind == ShapeKind::TextBox).map(|s| s.text.as_str()).collect();
    texts.sort();
    assert_eq!(texts, ["Find duplicate values", "Home"]);
}

#[test]
fn a_continue_after_a_txo_that_is_not_office_art_changes_nothing() {
    // Guard: an extra CONTINUE that does not start an OfficeArt container (version nibble 0xF and
    // type 0xF003/0xF004) is not read as a shape, and the text box is unchanged.
    let records = cat(&[&text_box(OT_TEXT_BOX, BOX, "How to"), &rec(0x003C, &[0u8; 8])]);
    let (sheet, warnings) = open(&records);
    assert_eq!(sheet.shapes.len(), 1, "{warnings:?}");
    assert_eq!(sheet.shapes[0].text, "How to");
}

#[test]
fn a_text_box_without_trailing_continues_is_unchanged() {
    // Guard: the ordinary layout (no drawing data after the TxO) still opens one text box.
    let (sheet, warnings) = open(&text_box(OT_TEXT_BOX, BOX, "How to"));
    assert!(!warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{warnings:?}");
    assert_eq!(sheet.shapes.len(), 1);
    assert_eq!(sheet.shapes[0].kind, ShapeKind::TextBox);
    assert_eq!(sheet.shapes[0].text, "How to");
}
