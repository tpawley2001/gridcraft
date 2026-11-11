//! `.xls` "Print object" ([MS-XLS] 2.4.181 Obj, 2.5.143 FtCmo `fPrint`): pictures, charts, text
//! boxes and drawn shapes whose Obj record has `fPrint` clear open with `no_print` set, so they are
//! left out of print and PDF like Excel, and Save As `.xlsx` keeps the setting
//! (`xdr:clientData fPrintsWithSheet="0"`).

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{bof, book, cat, cells, column_chart, eof, rec, u16s};
use super::xls_shapes_tests::{shape, txo, xls};

/// FtCmo flags measured on a real workbook: fLocked, fAutoFill, fAutoLine with fPrint (0x0010)
/// set, and the same with fPrint clear ("Print object" unchecked).
const PRINTED: u16 = 0x6011;
const NOT_PRINTED: u16 = 0x6001;

// FtCmo ot values.
const OT_RECTANGLE: u16 = 0x0002;
const OT_CHART: u16 = 0x0005;
const OT_TEXT_BOX: u16 = 0x0006;
const OT_OFFICE_ART: u16 = 0x001E;

/// An Obj record of kind `ot` with FtCmo `flags` and FtEnd.
fn obj(ot: u16, flags: u16) -> Vec<u8> {
    rec(0x005D, &cat(&[&u16s(&[0x15, 0x12, ot, 1, flags]), &[0; 12], &u16s(&[0, 0])]))
}

/// A sheet with an Office Art triangle (shape type 5) whose Obj has `flags`.
fn triangle(flags: u16) -> Vec<u8> {
    xls(&cat(&[&shape(5, &[], 0, 0, 2, 2), &obj(OT_OFFICE_ART, flags)]))
}

#[test]
fn an_office_art_shape_with_print_object_off_is_not_printed() {
    let (wb, report) = read_xls(&triangle(NOT_PRINTED)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert!(shapes[0].no_print, "FtCmo fPrint is clear");
}

#[test]
fn an_office_art_shape_with_print_object_on_is_printed() {
    let (wb, report) = read_xls(&triangle(PRINTED)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{:?}", report.warnings);
    assert!(!shapes[0].no_print, "FtCmo fPrint is set");
}

#[test]
fn rectangles_and_text_boxes_follow_their_own_print_flag() {
    let drawing = cat(&[
        &shape(1, &[], 0, 0, 2, 2),
        &obj(OT_RECTANGLE, NOT_PRINTED),
        &shape(1, &[], 3, 0, 5, 2),
        &obj(OT_RECTANGLE, PRINTED),
        &shape(202, &[], 0, 4, 3, 6),
        &obj(OT_TEXT_BOX, NOT_PRINTED),
        &txo("Draft"),
        &shape(202, &[], 4, 4, 7, 6),
        &obj(OT_TEXT_BOX, PRINTED),
        &txo("Final"),
    ]);
    let (wb, report) = read_xls(&xls(&drawing)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    // Text boxes and drawn shapes may be listed in either order; each keeps file order.
    let rects: Vec<bool> = shapes.iter().filter(|s| s.text.is_empty()).map(|s| s.no_print).collect();
    let boxes: Vec<(&str, bool)> = shapes.iter().filter(|s| !s.text.is_empty()).map(|s| (s.text.as_str(), s.no_print)).collect();
    assert_eq!(rects, [true, false], "{:?}", report.warnings);
    assert_eq!(boxes, [("Draft", true), ("Final", false)], "{:?}", report.warnings);
}

/// A workbook whose sheet holds the embedded column chart, its Obj having `flags`.
fn chart_book(flags: u16) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), &shape(201, &[], 4, 1, 10, 15), &obj(OT_CHART, flags), &column_chart(), &eof()]);
    book(&[("Sheet1", 0, sheet)])
}

#[test]
fn an_embedded_chart_follows_its_print_flag() {
    let (off, report) = read_xls(&chart_book(NOT_PRINTED)).unwrap();
    let charts = &off.sheet(0).unwrap().charts;
    assert_eq!(charts.len(), 1, "{:?}", report.warnings);
    assert!(charts[0].no_print, "FtCmo fPrint is clear");

    let (on, _) = read_xls(&chart_book(PRINTED)).unwrap();
    assert!(!on.sheet(0).unwrap().charts[0].no_print, "FtCmo fPrint is set");
}

#[test]
fn print_object_off_survives_save_as_xlsx() {
    let drawing = cat(&[&shape(5, &[], 0, 0, 2, 2), &obj(OT_OFFICE_ART, NOT_PRINTED), &shape(5, &[], 3, 0, 5, 2), &obj(OT_OFFICE_ART, PRINTED)]);
    let (wb, _) = read_xls(&xls(&drawing)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&out).unwrap();
    let flags: Vec<bool> = back.sheet(0).unwrap().shapes.iter().map(|s| s.no_print).collect();
    assert_eq!(flags, [true, false], "fPrintsWithSheet=\"0\" written for the first shape only");

    let (chart, _) = read_xls(&chart_book(NOT_PRINTED)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&chart).unwrap()).unwrap();
    assert!(back.sheet(0).unwrap().charts[0].no_print, "the chart keeps 'Print object' off");
}
