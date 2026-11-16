//! An embedded object or ActiveX control with "Print object" on prints its saved preview picture
//! at its place; one with "Print object" off prints nothing there.

use serde_json::json;

use super::*;
use gridcraft_model::{EmbeddedObject, EmbeddedObjectKind, ObjectPreview};

fn pdf_with_object(print: bool) -> (Vec<u8>, u64) {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [["x"]]})).unwrap();
    let mut png = Vec::new();
    let img = image::RgbaImage::from_raw(2, 2, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255]).unwrap();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let d = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut d.wb);
    wb.sheet_mut(0).unwrap().embedded_objects.push(EmbeddedObject {
        anchor: Anchor { cell: CellRef::new(2, 1), dx: 0.0, dy: 0.0, width: 80.0, height: 60.0, mode: gridcraft_model::AnchorMode::MoveAndSize },
        print,
        kind: EmbeddedObjectKind::Ole,
        prog_id: "Package".into(),
        preview: Some(ObjectPreview { data: std::sync::Arc::new(png), mime: "image/png".into() }),
    });
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    (crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap(), r["pages"].as_u64().unwrap())
}

fn has_image(pdf: &[u8]) -> bool {
    pdf.windows(b"/Subtype /Image".len()).any(|w| w == b"/Subtype /Image")
}

#[test]
fn printable_object_prints_its_preview() {
    let (pdf, pages) = pdf_with_object(true);
    assert_eq!(pages, 1);
    assert!(has_image(&pdf));
}

#[test]
fn object_with_print_off_prints_no_preview() {
    let (pdf, pages) = pdf_with_object(false);
    assert_eq!(pages, 1);
    assert!(!has_image(&pdf));
}
