//! XLSB worksheet drawings keep their SmartArt graphics through open + Save As `.xlsx`, as `.xlsx`
//! files do (`smartart_passthrough.rs`). An XLSB sheet drawing is the same DrawingML XML part as in
//! `.xlsx`; only the workbook and sheet parts are BIFF12 records, built here in code.
use gridcraft_core::{CellRef, Value};

use super::make_zip;
use super::smartart_passthrough::{
    COLORS, DATA, DIAGRAM_DRAWING, LAYOUT, MS_DIAGRAM_DRAWING, NS_R, PICTURE, PNG, QUICK_STYLE, SMARTART, anchor, assert_smartart_kept, cnvpr_ids,
    data_drawing, unpack,
};
use crate::{read_xlsb, read_xlsx, write_xlsx};

const TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/diagrams/data1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml"/><Override PartName="/xl/diagrams/layout1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml"/><Override PartName="/xl/diagrams/quickStyle1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml"/><Override PartName="/xl/diagrams/colors1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml"/><Override PartName="/xl/diagrams/drawing1.xml" ContentType="application/vnd.ms-office.drawingml.diagramDrawing+xml"/></Types>"#;
const ROOT: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
const BOOK_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/></Relationships>"#;
const SHEET_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;

/// A BIFF12 record: variable-length id and size ([MS-XLSB] 2.1.4), then the payload.
fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}

/// XLWideString: character count, then UTF-16LE.
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}

/// Workbook part with one sheet named "Data" (relationship rId1).
fn book() -> Vec<u8> {
    let mut out = record(131, &[]); // BrtBeginBook
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props)); // BrtWbProp
    out.extend(record(143, &[])); // BrtBeginBundleShs
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend(1u32.to_le_bytes());
    data.extend(wide("rId1"));
    data.extend(wide("Data"));
    out.extend(record(156, &data)); // BrtBundleSh
    out.extend(record(144, &[])); // BrtEndBundleShs
    out.extend(record(132, &[])); // BrtEndBook
    out
}

/// Sheet with A1 = 42 and a BrtDrawing record naming relationship rId7.
fn sheet() -> Vec<u8> {
    let mut out = record(129, &[]); // BrtBeginSheet
    out.extend(record(145, &[])); // BrtBeginSheetData
    let mut row = 0u32.to_le_bytes().to_vec();
    row.extend([0; 13]);
    out.extend(record(0, &row)); // BrtRowHdr
    let mut num = 0u32.to_le_bytes().to_vec();
    num.extend([0; 4]);
    num.extend(42.0f64.to_le_bytes());
    out.extend(record(5, &num)); // BrtCellReal
    out.extend(record(146, &[])); // BrtEndSheetData
    out.extend(record(550, &wide("rId7"))); // BrtDrawing
    out.extend(record(130, &[])); // BrtEndSheet
    out
}

/// An XLSB package whose sheet "Data" has a drawing with a SmartArt graphic and, with `picture`,
/// a picture in the same drawing.
fn package(picture: bool) -> Vec<u8> {
    let mut anchors = String::new();
    if picture {
        anchors.push_str(&anchor(0, 12, PICTURE));
    }
    anchors.push_str(&anchor(2, 1, SMARTART));
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{anchors}</xdr:wsDr>"#
    );
    let drawing_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/diagramData" Target="../diagrams/data1.xml"/><Relationship Id="rId2" Type="{NS_R}/diagramLayout" Target="../diagrams/layout1.xml"/><Relationship Id="rId3" Type="{NS_R}/diagramQuickStyle" Target="../diagrams/quickStyle1.xml"/><Relationship Id="rId4" Type="{NS_R}/diagramColors" Target="../diagrams/colors1.xml"/><Relationship Id="rId5" Type="{MS_DIAGRAM_DRAWING}" Target="../diagrams/drawing1.xml"/><Relationship Id="rId6" Type="{NS_R}/image" Target="../media/image1.png"/></Relationships>"#
    );
    let book = book();
    let sheet = sheet();
    make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", BOOK_RELS.as_bytes()),
        ("xl/worksheets/sheet1.bin", &sheet),
        ("xl/worksheets/_rels/sheet1.bin.rels", SHEET_RELS.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/diagrams/data1.xml", DATA.as_bytes()),
        ("xl/diagrams/layout1.xml", LAYOUT.as_bytes()),
        ("xl/diagrams/quickStyle1.xml", QUICK_STYLE.as_bytes()),
        ("xl/diagrams/colors1.xml", COLORS.as_bytes()),
        ("xl/diagrams/drawing1.xml", DIAGRAM_DRAWING.as_bytes()),
        ("xl/media/image1.png", PNG),
    ])
}

#[test]
fn xlsb_smartart_survives_save_as_xlsx() {
    let (wb, rep) = read_xlsb(&package(false)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_smartart_kept(&out);
    assert!(rep.warnings.iter().any(|w| w.contains("SmartArt")), "expected a 'not shown, kept' warning: {:?}", rep.warnings);
}

#[test]
fn xlsb_smartart_survives_a_second_xlsx_save() {
    let (wb, _) = read_xlsb(&package(false)).unwrap();
    let (wb2, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_smartart_kept(&write_xlsx(&wb2).unwrap());
}

#[test]
fn xlsb_smartart_beside_a_picture_keeps_both() {
    let (wb, _) = read_xlsb(&package(true)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().images.len(), 1, "the picture is modelled");
    let out = write_xlsx(&wb).unwrap();
    assert_smartart_kept(&out);
    let p = unpack(&out);
    let dx = &p[&data_drawing(&p)];
    assert!(dx.contains("<xdr:pic"), "picture lost: {dx}");
    let ids = cnvpr_ids(dx);
    let mut dedup = ids.clone();
    dedup.dedup();
    assert_eq!(ids, dedup, "duplicate cNvPr ids");
}

/// Guard: the sheet itself reads as before.
#[test]
fn xlsb_sheet_with_smartart_keeps_its_cells() {
    let (wb, _) = read_xlsb(&package(false)).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.name, "Data");
    assert_eq!(sheet.value(CellRef::new(0, 0)), Value::Number(42.0));
}
