//! `.xls` shapes whose OfficeArt drawing data ([MS-ODRAW] 2.2.14 OfficeArtSpContainer) is written
//! in a Continue record ([MS-XLS] 2.4.58) that follows the previous shape's Obj record, rather
//! than in a new MsoDrawing record.

use gridcraft_model::ShapeKind;

use crate::read_xls;

use super::xls_chart_tests::{cat, obj};
use super::xls_shapes_tests::{OT_OFFICE_ART, shape, xls};

/// The first MsoDrawing record of [`shape`] (its spContainer), re-tagged as a Continue record.
fn continued(spt: u16, col_l: u16, col_r: u16) -> Vec<u8> {
    let mut v = shape(spt, &[], col_l, 0, col_r, 2);
    let len = usize::from(u16::from_le_bytes([v[2], v[3]]));
    v.truncate(4 + len);
    v[0..2].copy_from_slice(&0x003Cu16.to_le_bytes());
    v
}

#[test]
fn a_shape_continued_after_an_obj_is_imported() {
    let triangle = cat(&[&shape(5, &[], 0, 0, 2, 2), &obj(OT_OFFICE_ART)]);
    let rect = cat(&[&continued(1, 3, 5), &obj(OT_OFFICE_ART)]);
    let (wb, report) = read_xls(&xls(&cat(&[&triangle, &rect]))).unwrap();
    let kinds: Vec<ShapeKind> = wb.sheet(0).unwrap().shapes.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [ShapeKind::Triangle, ShapeKind::Rectangle], "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("Shapes in .xls")), "{:?}", report.warnings);
}

#[test]
fn a_chain_of_continued_shapes_is_imported_in_order() {
    let first = cat(&[&shape(5, &[], 0, 0, 2, 2), &obj(OT_OFFICE_ART)]);
    let second = cat(&[&continued(1, 3, 5), &obj(OT_OFFICE_ART)]);
    let third = cat(&[&continued(3, 6, 8), &obj(OT_OFFICE_ART)]);
    let (wb, report) = read_xls(&xls(&cat(&[&first, &second, &third]))).unwrap();
    let kinds: Vec<ShapeKind> = wb.sheet(0).unwrap().shapes.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [ShapeKind::Triangle, ShapeKind::Rectangle, ShapeKind::Ellipse], "{:?}", report.warnings);
}
