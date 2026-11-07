//! `.xls` Office Art shapes ([MS-ODRAW] 2.4.24 MSOSPT) whose shape type has a DrawingML preset
//! (ECMA-376 Part 1 §20.1.10.56 ST_ShapeType) open as that preset and keep it through Save As
//! `.xlsx`; a freeform has no preset and is still reported.

use std::io::Read;

use gridcraft_model::{ShapeKind, Workbook};

use crate::{read_xls, write_xlsx};

use super::xls_chart_tests::{cat, obj};
use super::xls_shapes_tests::{OT_OFFICE_ART, shape, txo, xls};

const SPT_FREEFORM: u16 = 0; // msosptNotPrimitive
const SPT_DIAMOND: u16 = 4; // msosptDiamond
const SPT_OCTAGON: u16 = 10; // msosptOctagon
const SPT_STAR: u16 = 12; // msosptStar

/// An Office Art shape of type `spt` in its own cells, with its Obj.
fn art_shape(spt: u16, col: u16) -> Vec<u8> {
    cat(&[&shape(spt, &[], col, 0, col + 2, 2), &obj(OT_OFFICE_ART)])
}

fn presets(wb: &Workbook) -> Vec<(ShapeKind, Option<&str>)> {
    wb.sheet(0).unwrap().shapes.iter().map(|s| (s.kind, s.geometry.as_ref().map(|g| g.preset.as_str()))).collect()
}

#[test]
fn diamond_star_and_octagon_open_as_their_presets() {
    let records = cat(&[&art_shape(SPT_DIAMOND, 0), &art_shape(SPT_STAR, 3), &art_shape(SPT_OCTAGON, 6)]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    assert_eq!(
        presets(&wb),
        [(ShapeKind::Rectangle, Some("diamond")), (ShapeKind::Rectangle, Some("star5")), (ShapeKind::Rectangle, Some("octagon"))]
    );
    assert!(!report.warnings.iter().any(|w| w.contains("Shapes in .xls")), "{:?}", report.warnings);
}

#[test]
fn a_preset_shape_keeps_its_text() {
    let records = cat(&[&art_shape(SPT_DIAMOND, 0), &txo("Go?")]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].text, "Go?");
}

#[test]
fn the_already_supported_types_keep_no_preset_of_their_own() {
    // A rectangle (1) and a triangle (5) are kinds of their own.
    let (wb, _) = read_xls(&xls(&cat(&[&art_shape(1, 0), &art_shape(5, 3)]))).unwrap();
    assert_eq!(presets(&wb), [(ShapeKind::Rectangle, None), (ShapeKind::Triangle, None)]);
}

#[test]
fn a_freeform_is_still_reported() {
    let (wb, report) = read_xls(&xls(&art_shape(SPT_FREEFORM, 0))).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("Shapes in .xls files are not imported yet.")), "{:?}", report.warnings);
}

#[test]
fn a_star_saves_as_xlsx_with_its_preset() {
    let (wb, _) = read_xls(&xls(&art_shape(SPT_STAR, 0))).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut drawing = String::new();
    archive.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut drawing).unwrap();
    assert!(drawing.contains("<a:prstGeom prst=\"star5\">"), "{drawing}");
}
