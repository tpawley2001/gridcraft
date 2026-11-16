//! `.xls` shapes keep their flips: BIFF8 OfficeArtFSP fFlipH (0x40) / fFlipV (0x80) ([MS-ODRAW]
//! 2.2.40) and the BIFF5 line Obj's start quadrant `iqu`. A line drawn from top-right to
//! bottom-left must not open as top-left to bottom-right.

use gridcraft_model::{Shape, ShapeKind};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_biff5_line_tests::{BLACK, NO_ARROW, book, line};
use super::xls_chart_tests::{cat, obj, rec, u16s};
use super::xls_shapes_tests::xls;

const OT_LINE: u16 = 0x0001;
const OT_RECTANGLE: u16 = 0x0002;
const SPT_RECTANGLE: u16 = 1;
const SPT_LINE: u16 = 20;
const F_HAVE_ANCHOR_SPT: u32 = 0x0A00;
const F_FLIP_H: u32 = 0x0040;
const F_FLIP_V: u32 = 0x0080;

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

/// A BIFF8 MsoDrawing shape of type `spt` with these FSP flags (cells B2:E6), then its Obj.
fn biff8_shape(spt: u16, flags: u32, ot: u16) -> Vec<u8> {
    let sp = art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &(F_HAVE_ANCHOR_SPT | flags).to_le_bytes()]));
    let fopt = art(3, 0, 0xF00B, &[]);
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 0, 1, 0, 4, 0, 5, 0]));
    let data = art(0, 0, 0xF011, &[]);
    cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &fopt, &anchor, &data]))), &obj(ot)])
}

fn flips(s: &Shape) -> (bool, bool) {
    (s.flip_h, s.flip_v)
}

fn only(shapes: &[Shape]) -> &Shape {
    assert_eq!(shapes.len(), 1, "{shapes:?}");
    &shapes[0]
}

#[test]
fn biff8_lines_take_their_flips_from_the_shape_record() {
    for (flags, want) in [(0, (false, false)), (F_FLIP_H, (true, false)), (F_FLIP_V, (false, true)), (F_FLIP_H | F_FLIP_V, (true, true))] {
        let (wb, _) = read_xls(&xls(&biff8_shape(SPT_LINE, flags, OT_LINE))).unwrap();
        let s = only(&wb.sheet(0).unwrap().shapes);
        assert_eq!(s.kind, ShapeKind::Line);
        assert_eq!(flips(s), want, "FSP flags {flags:#x}");
    }
}

#[test]
fn a_biff8_rectangle_keeps_its_flip() {
    let (wb, _) = read_xls(&xls(&biff8_shape(SPT_RECTANGLE, F_FLIP_V, OT_RECTANGLE))).unwrap();
    assert_eq!(flips(only(&wb.sheet(0).unwrap().shapes)), (false, true));
}

#[test]
fn a_flipped_biff8_line_keeps_its_flips_through_save_as_xlsx() {
    let (wb, _) = read_xls(&xls(&biff8_shape(SPT_LINE, F_FLIP_H, OT_LINE))).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(flips(only(&back.sheet(0).unwrap().shapes)), (true, false));
}

#[test]
fn biff5_lines_take_their_flips_from_the_start_quadrant() {
    // iqu 0: top-left to bottom-right; 1: top-right to bottom-left; 2: bottom-right to top-left;
    // 3: bottom-left to top-right; anything else: no flips.
    for (iqu, want) in [(0u8, (false, false)), (1, (true, false)), (2, (true, true)), (3, (false, true)), (7, (false, false))] {
        let (wb, report) = read_xls(&book(&line(1, [1, 0, 2, 0, 4, 0, 6, 0], BLACK, NO_ARROW, iqu))).unwrap();
        let s = only(&wb.sheet(0).unwrap().shapes);
        assert_eq!(s.kind, ShapeKind::Line, "{:?}", report.warnings);
        assert_eq!(flips(s), want, "iqu {iqu}");
    }
}
