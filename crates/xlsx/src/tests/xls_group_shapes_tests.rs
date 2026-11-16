//! `.xls` Office Art text boxes and grouped shapes ([MS-XLS] 2.4.181 Obj, 2.4.168 MsoDrawing;
//! [MS-ODRAW] 2.2.16 OfficeArtSpgrContainer, 2.2.38 OfficeArtFSPGR, 2.2.39 OfficeArtChildAnchor,
//! 2.2.40 OfficeArtFSP): text boxes written as Office Art objects (shape type 202) keep their
//! text, and the shapes of a group open at their place in the group, flattened to top-level shapes.

use gridcraft_model::{Shape, ShapeKind, Sheet};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;

const OT_GROUP: u16 = 0x0000;
const OT_RECTANGLE: u16 = 0x0002;
const OT_TEXT_BOX: u16 = 0x0006;
const OT_OFFICE_ART: u16 = 0x001E;

const SPT_RECTANGLE: u16 = 1;
const SPT_WORD_ART: u16 = 136;
const SPT_TEXT_BOX: u16 = 202;

// OfficeArtFSP flags.
const F_GROUP: u32 = 0x0001;
const F_CHILD: u32 = 0x0002;
const F_HAVE_ANCHOR: u32 = 0x0200;
const F_HAVE_SPT: u32 = 0x0800;

const SHAPES_WARNING: &str = "Shapes in .xls files are not imported yet";

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

fn i32s(v: [i32; 4]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn fsp(spt: u16, flags: u32) -> Vec<u8> {
    art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &flags.to_le_bytes()]))
}

fn client_anchor(col_l: u16, row_t: u16, col_r: u16, row_b: u16) -> Vec<u8> {
    art(0, 0, 0xF010, &u16s(&[0, col_l, 0, row_t, 0, col_r, 0, row_b, 0]))
}

fn child_anchor(r: [i32; 4]) -> Vec<u8> {
    art(0, 0, 0xF00F, &i32s(r))
}

fn fspgr(r: [i32; 4]) -> Vec<u8> {
    art(1, 0, 0xF009, &i32s(r))
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

/// A top-level shape anchored to cells: its SpContainer in one MsoDrawing record, its Obj, then
/// (with `text`) the ClientTextbox MsoDrawing and the TxO, in Excel's order.
fn top_level(spt: u16, ot: u16, a: (u16, u16, u16, u16), text: Option<&str>) -> Vec<u8> {
    let sp = art(15, 0, 0xF004, &cat(&[&fsp(spt, F_HAVE_ANCHOR | F_HAVE_SPT), &client_anchor(a.0, a.1, a.2, a.3), &art(0, 0, 0xF011, &[])]));
    match text {
        Some(t) => cat(&[&rec(0x00EC, &sp), &obj(ot), &rec(0x00EC, &art(0, 0, 0xF00D, &[])), &txo(t)]),
        None => cat(&[&rec(0x00EC, &sp), &obj(ot)]),
    }
}

/// A group child as Excel 2007+ writes it: an SpContainer with a child anchor, whose length
/// includes the ClientTextbox atom that sits in the next MsoDrawing record (after the Obj).
/// Returns the container bytes (for the parent's length) and the records.
fn child(spt: u16, ot: u16, r: [i32; 4], text: Option<&str>) -> (usize, Vec<u8>) {
    let head = cat(&[&fsp(spt, F_CHILD | F_HAVE_ANCHOR | F_HAVE_SPT), &child_anchor(r), &art(0, 0, 0xF011, &[])]);
    let tb = art(0, 0, 0xF00D, &[]);
    match text {
        Some(t) => {
            let full = art(15, 0, 0xF004, &cat(&[&head, &tb]));
            let first = &full[..full.len() - tb.len()];
            (full.len(), cat(&[&rec(0x00EC, first), &obj(ot), &rec(0x00EC, &tb), &txo(t)]))
        }
        None => {
            let full = art(15, 0, 0xF004, &head);
            (full.len(), cat(&[&rec(0x00EC, &full), &obj(ot)]))
        }
    }
}

/// A group: the SpgrContainer header and the group shape (frame + anchor) in one MsoDrawing
/// record, the group's Obj, then its children's records. `anchor` is the group shape's own
/// anchor atom (a client anchor at top level, a child anchor inside another group).
fn group(frame: [i32; 4], anchor: &[u8], is_child: bool, children: &[(usize, Vec<u8>)]) -> (usize, Vec<u8>) {
    let flags = F_GROUP | F_HAVE_ANCHOR | if is_child { F_CHILD } else { 0 };
    let group_sp = art(15, 0, 0xF004, &cat(&[&fspgr(frame), &fsp(0, flags), anchor, &art(0, 0, 0xF011, &[])]));
    let len = group_sp.len() + children.iter().map(|c| c.0).sum::<usize>();
    let header = cat(&[&u16s(&[0x000F, 0xF003]), &(len as u32).to_le_bytes()]);
    let mut out = cat(&[&rec(0x00EC, &cat(&[&header, &group_sp])), &obj(OT_GROUP)]);
    for c in children {
        out.extend(&c.1);
    }
    (header.len() + len, out)
}

/// A shape's sheet rectangle in points: (x, y, width, height).
fn rect(sheet: &Sheet, s: &Shape) -> (f64, f64, f64, f64) {
    let a = &s.anchor;
    (sheet.col_left(a.cell.col) + f64::from(a.dx), sheet.row_top(a.cell.row) + f64::from(a.dy), f64::from(a.width), f64::from(a.height))
}

/// Where a plain top-level rectangle with this client anchor lands: the reference for a group's
/// placed rectangle.
fn frame_of(a: (u16, u16, u16, u16)) -> (f64, f64, f64, f64) {
    let (wb, _) = read_xls(&xls(&top_level(SPT_RECTANGLE, OT_RECTANGLE, a, None))).unwrap();
    let sheet = wb.sheet(0).unwrap();
    rect(sheet, &sheet.shapes[0])
}

fn assert_rect(got: (f64, f64, f64, f64), want: (f64, f64, f64, f64)) {
    let close = |a: f64, b: f64| (a - b).abs() < 0.01;
    assert!(close(got.0, want.0) && close(got.1, want.1) && close(got.2, want.2) && close(got.3, want.3), "got {got:?}, want {want:?}");
}

const GROUP_CELLS: (u16, u16, u16, u16) = (1, 2, 5, 12);

#[test]
fn an_office_art_text_box_opens_like_a_text_box_with_its_text() {
    let (wb, report) = read_xls(&xls(&top_level(SPT_TEXT_BOX, OT_OFFICE_ART, (1, 2, 4, 6), Some("Field Office")))).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{:?}", report.warnings);
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert_eq!(shapes[0].kind, ShapeKind::TextBox);
    assert_eq!(shapes[0].text, "Field Office");
    // The same box written as a classic text box Obj (ot 6) opens identically.
    let (classic, _) = read_xls(&xls(&top_level(SPT_TEXT_BOX, OT_TEXT_BOX, (1, 2, 4, 6), Some("Field Office")))).unwrap();
    assert_eq!(shapes[0], classic.sheet(0).unwrap().shapes[0]);
}

#[test]
fn grouped_shapes_open_at_their_place_in_the_group() {
    let a = GROUP_CELLS;
    let children =
        [child(SPT_RECTANGLE, OT_OFFICE_ART, [0, 0, 500, 1000], None), child(SPT_TEXT_BOX, OT_OFFICE_ART, [500, 1000, 1000, 2000], Some("Note"))];
    let g = group([0, 0, 1000, 2000], &client_anchor(a.0, a.1, a.2, a.3), false, &children);
    // A top-level rectangle after the group keeps its own anchor.
    let after = top_level(SPT_RECTANGLE, OT_RECTANGLE, (6, 1, 8, 3), None);
    let (wb, report) = read_xls(&xls(&cat(&[&g.1, &after]))).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{:?}", report.warnings);
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.shapes.len(), 3, "{:?}", report.warnings);
    let (gx, gy, gw, gh) = frame_of(a);
    let rectangle = sheet.shapes.iter().find(|s| s.kind == ShapeKind::Rectangle && rect(sheet, s).0 < gx + gw).unwrap();
    assert_rect(rect(sheet, rectangle), (gx, gy, gw / 2.0, gh / 2.0));
    let note = sheet.shapes.iter().find(|s| s.kind == ShapeKind::TextBox).unwrap();
    assert_eq!(note.text, "Note");
    assert_rect(rect(sheet, note), (gx + gw / 2.0, gy + gh / 2.0, gw / 2.0, gh / 2.0));
    let own = sheet.shapes.iter().filter(|s| s.kind == ShapeKind::Rectangle).map(|s| rect(sheet, s)).find(|r| r.0 >= gx + gw).unwrap();
    assert_rect(own, frame_of((6, 1, 8, 3)));
}

#[test]
fn a_nested_group_maps_through_both_frames() {
    let a = GROUP_CELLS;
    // Inner group: right half of the outer frame, its own frame 0..10 x 0..10; its one child
    // fills the inner frame's bottom half.
    let inner = group([0, 0, 10, 10], &child_anchor([50, 0, 100, 100]), true, &[child(SPT_RECTANGLE, OT_OFFICE_ART, [0, 5, 10, 10], None)]);
    // After the inner group ends, this child is in the outer frame again: top-left quarter.
    let outer_child = child(SPT_TEXT_BOX, OT_OFFICE_ART, [0, 0, 50, 50], Some("Outer"));
    let g = group([0, 0, 100, 100], &client_anchor(a.0, a.1, a.2, a.3), false, &[inner, outer_child]);
    let (wb, report) = read_xls(&xls(&g.1)).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{:?}", report.warnings);
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.shapes.len(), 2, "{:?}", report.warnings);
    let (gx, gy, gw, gh) = frame_of(a);
    let deep = sheet.shapes.iter().find(|s| s.kind == ShapeKind::Rectangle).unwrap();
    assert_rect(rect(sheet, deep), (gx + gw / 2.0, gy + gh / 2.0, gw / 2.0, gh / 2.0));
    let outer = sheet.shapes.iter().find(|s| s.kind == ShapeKind::TextBox).unwrap();
    assert_eq!(outer.text, "Outer");
    assert_rect(rect(sheet, outer), (gx, gy, gw / 2.0, gh / 2.0));
}

#[test]
fn an_unsupported_child_is_reported_and_the_others_still_open() {
    let a = GROUP_CELLS;
    let children = [child(SPT_WORD_ART, OT_OFFICE_ART, [0, 0, 1000, 500], None), child(SPT_RECTANGLE, OT_OFFICE_ART, [0, 500, 1000, 1000], None)];
    let g = group([0, 0, 1000, 1000], &client_anchor(a.0, a.1, a.2, a.3), false, &children);
    let (wb, report) = read_xls(&xls(&g.1)).unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.contains(SHAPES_WARNING)).count(), 1, "{:?}", report.warnings);
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.shapes.len(), 1);
    let (gx, gy, gw, gh) = frame_of(a);
    assert_rect(rect(sheet, &sheet.shapes[0]), (gx, gy + gh / 2.0, gw, gh / 2.0));
}

#[test]
fn grouped_shapes_survive_save_as_xlsx() {
    let a = GROUP_CELLS;
    let children =
        [child(SPT_RECTANGLE, OT_OFFICE_ART, [0, 0, 500, 1000], None), child(SPT_TEXT_BOX, OT_OFFICE_ART, [500, 1000, 1000, 2000], Some("Note"))];
    let g = group([0, 0, 1000, 2000], &client_anchor(a.0, a.1, a.2, a.3), false, &children);
    let (wb, _) = read_xls(&xls(&g.1)).unwrap();
    let before: Vec<_> = wb.sheet(0).unwrap().shapes.iter().map(|s| (s.kind, s.text.clone(), rect(wb.sheet(0).unwrap(), s))).collect();
    assert_eq!(before.len(), 2);
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let sheet = back.sheet(0).unwrap();
    assert_eq!(sheet.shapes.len(), 2);
    for (kind, text, r) in before {
        let s = sheet.shapes.iter().find(|s| s.kind == kind).unwrap();
        assert_eq!(s.text, text);
        let got = rect(sheet, s);
        assert!((got.0 - r.0).abs() < 0.5 && (got.1 - r.1).abs() < 0.5 && (got.2 - r.2).abs() < 0.5 && (got.3 - r.3).abs() < 0.5, "{got:?} vs {r:?}");
    }
}

/// Change one synthetic FSP without changing its container lengths or anchors.
fn with_fsp_flags(mut records: Vec<u8>, spt: u16, old: u32, new: u32) -> Vec<u8> {
    let before = fsp(spt, old);
    let after = fsp(spt, new);
    let at = records.windows(before.len()).position(|w| w == before).unwrap();
    records[at..at + before.len()].copy_from_slice(&after);
    records
}

#[test]
fn text_box_flips_survive_save_and_do_not_leak_to_the_next_shape() {
    let flags = F_HAVE_ANCHOR | F_HAVE_SPT;
    let flipped = with_fsp_flags(top_level(SPT_TEXT_BOX, OT_TEXT_BOX, (1, 2, 4, 6), Some("Note")), SPT_TEXT_BOX, flags, flags | 0xC0);
    let plain = top_level(SPT_RECTANGLE, OT_RECTANGLE, (6, 1, 8, 3), None);
    let (wb, _) = read_xls(&xls(&cat(&[&flipped, &plain]))).unwrap();
    for book in [wb.clone(), read_xlsx(&write_xlsx(&wb).unwrap()).unwrap().0] {
        let shapes = &book.sheet(0).unwrap().shapes;
        let text = shapes.iter().find(|s| s.kind == ShapeKind::TextBox).unwrap();
        assert_eq!((text.flip_h, text.flip_v), (true, true));
        assert_eq!(text.text, "Note");
        let rectangle = shapes.iter().find(|s| s.kind == ShapeKind::Rectangle).unwrap();
        assert_eq!((rectangle.flip_h, rectangle.flip_v), (false, false));
    }
}

#[test]
fn group_children_use_their_own_flips_without_the_groups_flags() {
    let flags = F_CHILD | F_HAVE_ANCHOR | F_HAVE_SPT;
    let mut flipped = child(SPT_RECTANGLE, OT_OFFICE_ART, [0, 0, 50, 50], None);
    flipped.1 = with_fsp_flags(flipped.1, SPT_RECTANGLE, flags, flags | 0x80);
    let plain = child(SPT_RECTANGLE, OT_OFFICE_ART, [50, 50, 100, 100], None);
    let g = group([0, 0, 100, 100], &client_anchor(1, 2, 5, 12), false, &[flipped, plain]);
    let records = with_fsp_flags(g.1, 0, F_GROUP | F_HAVE_ANCHOR, F_GROUP | F_HAVE_ANCHOR | 0x40);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 2);
    assert_eq!((shapes[0].flip_h, shapes[0].flip_v), (false, true));
    assert_eq!((shapes[1].flip_h, shapes[1].flip_v), (false, false));
}
