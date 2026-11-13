//! `.xls` picture objects whose image is not imported (the EMF or WMF preview of an embedded
//! object, an ActiveX control's picture, a blip missing from the store): the picture is not
//! shown, but its place is kept as an [`EmbeddedObject`] so the sheet still prints as far as the
//! object reaches ([MS-XLS] 2.4.181 Obj with `ot` 8, [MS-ODRAW] 2.2.40 OfficeArtFSP).

use gridcraft_core::CellRef;
use gridcraft_model::{EmbeddedObjectKind, Sheet, Workbook};

use super::xls_chart_tests::{cat, rec, u16s};
use super::xls_picture_tests::{FORMAT_WARNING, art, blip, book, drawing_group, fbse, fopt, png, read};

/// FtCmo `fPrint` and `fUIObj` ([MS-XLS] 2.5.143).
const F_PRINT: u16 = 0x0010;
const F_UI_OBJ: u16 = 0x0100;
/// msosptPictureFrame and msosptHostControl ([MS-ODRAW] 2.4.24).
const PICTURE_FRAME: u16 = 75;
const HOST_CONTROL: u16 = 201;

/// An OfficeArtBlipEMF ([MS-ODRAW] 2.2.24) with one UID: the UID, the 34-byte metafile header,
/// then the (here meaningless) compressed file.
fn emf() -> Vec<u8> {
    art(0, 0x3D4, 0xF01A, &cat(&[&[0xCD; 16], &[0; 34], b"compressed metafile"]))
}

/// A store with a PNG as blip 1 and an EMF as blip 2.
fn store() -> Vec<u8> {
    drawing_group(&[fbse(&blip(0xF01E, 0x6E0, 1, &png())), fbse(&emf())])
}

/// An MsoDrawing holding one shape of type `spt` anchored from the top left of A8 to the top
/// left of P20, then its picture Obj with the FtCmo `flags`.
fn object(spt: u16, props: &[(u16, u32)], flags: u16) -> Vec<u8> {
    let fsp = art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 0, 0, 7, 0, 15, 0, 19, 0]));
    let data = art(0, 0, 0xF011, &[]);
    let drawing = rec(0x00EC, &art(0xF, 0, 0xF004, &cat(&[&fsp, &fopt(props, &[]), &anchor, &data])));
    let obj = rec(0x005D, &cat(&[&u16s(&[0x15, 0x12, 8, 1, flags]), &[0; 12], &u16s(&[0, 0])]));
    cat(&[&drawing, &obj])
}

fn open(sheet_records: &[u8]) -> (Workbook, Vec<String>) {
    read(&book(&store(), sheet_records))
}

/// The object's place must be A8 to the top left of P20.
fn assert_place(sheet: &Sheet, at: usize) {
    let a = &sheet.embedded_objects[at].anchor;
    assert_eq!(a.cell, CellRef::new(7, 0));
    assert_eq!((a.dx, a.dy), (0.0, 0.0));
    let width = sheet.col_left(15) - sheet.col_left(0);
    let height = sheet.row_top(19) - sheet.row_top(7);
    assert!((f64::from(a.width) - width).abs() < 0.01, "width {} != {width}", a.width);
    assert!((f64::from(a.height) - height).abs() < 0.01, "height {} != {height}", a.height);
}

#[test]
fn xls_emf_picture_keeps_its_place_as_an_embedded_object() {
    let (wb, warnings) = open(&object(PICTURE_FRAME, &[(0x4104, 2)], F_PRINT));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty(), "{warnings:?}");
    assert_eq!(sheet.embedded_objects.len(), 1, "{warnings:?}");
    assert_place(sheet, 0);
    let o = &sheet.embedded_objects[0];
    assert!(o.print);
    assert_eq!(o.kind, EmbeddedObjectKind::Ole);
    assert_eq!(o.prog_id, "");
}

#[test]
fn xls_emf_picture_with_print_object_off_is_kept_but_not_printed() {
    let (wb, _) = open(&object(PICTURE_FRAME, &[(0x4104, 2)], 0));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty());
    assert_eq!(sheet.embedded_objects.len(), 1);
    assert_place(sheet, 0);
    assert!(!sheet.embedded_objects[0].print);
}

#[test]
fn xls_png_picture_is_an_image_and_not_an_embedded_object() {
    let (wb, warnings) = open(&object(PICTURE_FRAME, &[(0x4104, 1)], F_PRINT));
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.images.len(), 1, "{warnings:?}");
    assert!(sheet.embedded_objects.is_empty());
    assert!(!sheet.images[0].no_print);
}

#[test]
fn xls_picture_whose_blip_is_not_in_the_store_keeps_its_place() {
    let (wb, _) = open(&object(PICTURE_FRAME, &[(0x4104, 9)], F_PRINT));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty());
    assert_eq!(sheet.embedded_objects.len(), 1);
    assert_place(sheet, 0);
    assert!(sheet.embedded_objects[0].print);
}

#[test]
fn xls_picture_without_a_blip_property_keeps_its_place() {
    let (wb, _) = open(&object(PICTURE_FRAME, &[], F_PRINT));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty());
    assert_eq!(sheet.embedded_objects.len(), 1);
    assert_place(sheet, 0);
}

#[test]
fn xls_ui_object_picture_adds_nothing() {
    let (wb, _) = open(&object(PICTURE_FRAME, &[(0x4104, 2)], F_PRINT | F_UI_OBJ));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty());
    assert!(sheet.embedded_objects.is_empty());
}

#[test]
fn xls_host_control_shape_is_an_activex_object() {
    let (wb, _) = open(&object(HOST_CONTROL, &[(0x4104, 2)], F_PRINT));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty());
    assert_eq!(sheet.embedded_objects.len(), 1);
    assert_place(sheet, 0);
    assert_eq!(sheet.embedded_objects[0].kind, EmbeddedObjectKind::ActiveX);
    assert!(sheet.embedded_objects[0].print);
}

#[test]
fn xls_hidden_unshown_picture_is_not_printed() {
    // Group booleans: fUsefHidden + fHidden.
    let (wb, _) = open(&object(PICTURE_FRAME, &[(0x4104, 2), (0x03BF, 0x0002_0002)], F_PRINT));
    assert!(wb.sheets[0].embedded_objects.iter().all(|o| !o.print));
}

#[test]
fn xls_unshown_pictures_are_reported_once() {
    let records = cat(&[&object(PICTURE_FRAME, &[(0x4104, 2)], F_PRINT), &object(PICTURE_FRAME, &[(0x4104, 2)], F_PRINT)]);
    let (wb, warnings) = open(&records);
    assert_eq!(wb.sheets[0].embedded_objects.len(), 2);
    assert_eq!(warnings.iter().filter(|w| *w == FORMAT_WARNING).count(), 1, "{warnings:?}");
    let about_pictures = warnings.iter().filter(|w| w.contains("picture") && !w.starts_with("Imported from")).count();
    assert_eq!(about_pictures, 1, "{warnings:?}");
}
