//! `.xls` metafile previews of embedded OLE objects (an Obj with an FtPictFmla, [MS-XLS] 2.5.140)
//! and ActiveX controls (shape type 201) stay embedded objects, not pictures, and keep the blip as
//! their saved preview ([MS-ODRAW] 2.3.23.1 pib), without a "format isn't imported" warning.

use gridcraft_model::EmbeddedObjectKind;

use super::xls_chart_tests::{cat, obj, rec, u16s};
use super::xls_picture_tests::{art, book, drawing_group, fbse, fopt, read};

/// A stored EMF blip with one UID.
fn emf_store() -> Vec<u8> {
    let emf = cat(&[&[1, 0, 0, 0], &[0x22; 60]]);
    let mut head = vec![0xAB; 16];
    head.extend((emf.len() as u32).to_le_bytes());
    for v in [0i32, 0, 100, 50, 914400, 457200] {
        head.extend(v.to_le_bytes());
    }
    head.extend((emf.len() as u32).to_le_bytes());
    head.extend([0xFE, 0xFE]);
    drawing_group(&[fbse(&art(0, 0x3D4, 0xF01A, &cat(&[&head, &emf])))])
}

/// A picture shape of type `shape_type` naming store entry 1, then its Obj record.
fn shape(shape_type: u16, obj_record: &[u8]) -> Vec<u8> {
    let fsp = art(2, shape_type, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 256, 2, 0, 3, 0, 6, 128]));
    let data = art(0, 0, 0xF011, &[]);
    let sp = art(0xF, 0, 0xF004, &cat(&[&fsp, &fopt(&[(0x4104, 1)], &[]), &anchor, &data]));
    cat(&[&rec(0x00EC, &sp), obj_record])
}

/// Obj: FtCmo (ot 8, picture), an FtPictFmla with a small body, FtEnd.
fn ole_obj() -> Vec<u8> {
    let pict_fmla = cat(&[&u16s(&[0x0009, 8]), &[0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]]);
    rec(0x005D, &cat(&[&u16s(&[0x15, 0x12, 8, 1, 0x6011]), &[0; 12], &pict_fmla, &u16s(&[0, 0])]))
}

#[test]
fn xls_ole_object_emf_preview_stays_an_embedded_object() {
    let (wb, warnings) = read(&book(&emf_store(), &shape(75, &ole_obj())));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty(), "{warnings:?}");
    assert_eq!(sheet.embedded_objects.len(), 1);
    assert_eq!(sheet.embedded_objects[0].kind, EmbeddedObjectKind::Ole);
    let preview = sheet.embedded_objects[0].preview.as_ref().expect("EMF preview");
    assert_eq!(preview.mime, "image/x-emf");
    assert!(!preview.data.is_empty());
    assert_eq!(warnings.iter().filter(|w| w.contains("use a format that isn't imported")).count(), 0, "{warnings:?}");
}

#[test]
fn xls_activex_emf_preview_stays_an_embedded_object() {
    let (wb, warnings) = read(&book(&emf_store(), &shape(201, &obj(8))));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty(), "{warnings:?}");
    assert_eq!(sheet.embedded_objects.len(), 1);
    assert_eq!(sheet.embedded_objects[0].kind, EmbeddedObjectKind::ActiveX);
    assert_eq!(sheet.embedded_objects[0].preview.as_ref().map(|p| p.mime.as_str()), Some("image/x-emf"));
    assert!(!warnings.iter().any(|w| w.contains("use a format that isn't imported")), "{warnings:?}");
}

#[test]
fn xls_plain_emf_picture_next_to_the_same_obj_layout_is_a_picture() {
    let (wb, warnings) = read(&book(&emf_store(), &shape(75, &obj(8))));
    assert_eq!(wb.sheets[0].images.len(), 1, "{warnings:?}");
    assert!(wb.sheets[0].embedded_objects.is_empty());
}
