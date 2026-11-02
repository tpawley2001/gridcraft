//! XLSB worksheets keep their DrawingML drawing parts: original BIFF12 records and packages
//! assembled in Rust from the public format schema, with small hand-written DrawingML parts.
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Sheet, Workbook};

use super::make_zip;
use crate::{ReadReport, read_xlsb};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
const BOOK_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/></Types>"#;
const SHEET_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;
const DRAWING_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/></Relationships>"#;
const CHART: &str = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:ser><c:idx val="0"/><c:order val="0"/><c:cat><c:strRef><c:f>Sheet1!$A$1:$A$3</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$1:$B$3</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
/// The 8-byte PNG signature followed by a few bytes: the reader keeps picture bytes as they are.
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, b'I', b'H', b'D', b'R'];

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
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}
fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend(1u32.to_le_bytes());
    data.extend(wide("rId1"));
    data.extend(wide("Sheet1"));
    out.extend(record(156, &data));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}
/// Sheet1 with A1 = 42 and, when `drawing` is set, a BrtDrawing record naming that relationship.
fn sheet(drawing: Option<&str>) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(145, &[]));
    let mut row = 0u32.to_le_bytes().to_vec();
    row.extend([0; 13]);
    out.extend(record(0, &row));
    let mut num = 0u32.to_le_bytes().to_vec();
    num.extend([0; 4]);
    num.extend(42.0f64.to_le_bytes());
    out.extend(record(5, &num));
    out.extend(record(146, &[]));
    if let Some(id) = drawing {
        out.extend(record(550, &wide(id)));
    }
    out.extend(record(551, &wide("rId99"))); // BrtLegacyDrawing (VML): ignored.
    out.extend(record(130, &[]));
    out
}
/// An XLSB package of one sheet plus `extra` parts (the sheet's `.bin.rels`, drawings, media…).
fn package(sheet: &[u8], extra: &[(&str, &[u8])]) -> Vec<u8> {
    let book = book();
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", BOOK_RELS.as_bytes()),
        ("xl/worksheets/sheet1.bin", sheet),
    ];
    parts.extend_from_slice(extra);
    make_zip(&parts)
}
fn drawing(anchors: &str) -> String {
    format!(
        r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{anchors}</xdr:wsDr>"#
    )
}
/// A two-cell anchor from (`c0`, `r0`) to (`c1`, `r1`), zero-based, around `object`.
fn anchor(c0: u32, r0: u32, c1: u32, r1: u32, object: &str) -> String {
    format!(
        "<xdr:twoCellAnchor><xdr:from><xdr:col>{c0}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r0}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{c1}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r1}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{object}<xdr:clientData/></xdr:twoCellAnchor>"
    )
}
fn read_with_drawing(drawing_xml: &str) -> (Workbook, ReadReport) {
    let bytes = package(
        &sheet(Some("rId7")),
        &[
            ("xl/worksheets/_rels/sheet1.bin.rels", SHEET_RELS.as_bytes()),
            ("xl/drawings/drawing1.xml", drawing_xml.as_bytes()),
            ("xl/drawings/_rels/drawing1.xml.rels", DRAWING_RELS.as_bytes()),
            ("xl/media/image1.png", PNG),
            ("xl/charts/chart1.xml", CHART.as_bytes()),
        ],
    );
    read_xlsb(&bytes).unwrap()
}
fn first(wb: &Workbook) -> &Sheet {
    wb.sheet(0).unwrap()
}
fn cells_intact(wb: &Workbook) {
    assert_eq!(first(wb).value(CellRef::new(0, 0)), Value::Number(42.0));
}

#[test]
fn picture_keeps_its_anchor_and_bytes() {
    let pic = r#"<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1" descr="dot"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"#;
    let (wb, _) = read_with_drawing(&drawing(&anchor(1, 1, 3, 5, pic)));
    cells_intact(&wb);
    let images = &first(&wb).images;
    assert_eq!(images.len(), 1);
    let image = &images[0];
    assert_eq!(image.anchor.cell, CellRef::new(1, 1));
    assert!(image.anchor.width > 0.0 && image.anchor.height > 0.0);
    assert_eq!(&image.data[..], PNG);
    assert_eq!(image.mime, "image/png");
    assert_eq!(image.alt, "dot");
}

#[test]
fn bar_chart_keeps_its_anchor_and_series_refs() {
    let frame = r#"<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="3" name="Chart 1"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId2"/></a:graphicData></a:graphic></xdr:graphicFrame>"#;
    let (wb, _) = read_with_drawing(&drawing(&anchor(5, 1, 11, 14, frame)));
    cells_intact(&wb);
    let charts = &first(&wb).charts;
    assert_eq!(charts.len(), 1);
    let chart = &charts[0];
    assert_eq!(chart.anchor.cell, CellRef::new(1, 5));
    assert_eq!(chart.series.len(), 1);
    assert_eq!(chart.series[0].values, "Sheet1!$B$1:$B$3");
    assert_eq!(chart.series[0].categories.as_deref(), Some("Sheet1!$A$1:$A$3"));
}

#[test]
fn rectangle_shape_keeps_its_text() {
    let shape = r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="4" name="Rectangle 1"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Hello</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#;
    let (wb, _) = read_with_drawing(&drawing(&anchor(0, 3, 2, 6, shape)));
    cells_intact(&wb);
    let shapes = &first(&wb).shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].text, "Hello");
    assert_eq!(shapes[0].name.as_deref(), Some("Rectangle 1"));
    assert_eq!(shapes[0].anchor.cell, CellRef::new(3, 0));
}

#[test]
fn drawing_relationship_missing_from_rels_warns_and_keeps_cells() {
    let other = SHEET_RELS.replace("rId7", "rId8");
    for rels in [Some(other.as_str()), None] {
        let mut extra: Vec<(&str, &[u8])> = vec![("xl/drawings/drawing1.xml", b"<xdr:wsDr/>")];
        if let Some(rels) = rels {
            extra.push(("xl/worksheets/_rels/sheet1.bin.rels", rels.as_bytes()));
        }
        let (wb, report) = read_xlsb(&package(&sheet(Some("rId7")), &extra)).unwrap();
        cells_intact(&wb);
        assert!(report.warnings.iter().any(|w| w.contains("no drawing relationship")), "{:?}", report.warnings);
        let s = first(&wb);
        assert!(s.images.is_empty() && s.shapes.is_empty() && s.charts.is_empty());
    }
}

#[test]
fn malformed_drawing_xml_warns_and_keeps_cells() {
    let before = read_xlsb(&package(&sheet(None), &[])).unwrap().1.warnings.len();
    let (wb, report) = read_with_drawing("<xdr:wsDr><xdr:twoCellAnchor></xdr:wsDr");
    cells_intact(&wb);
    assert_eq!(report.warnings.len(), before + 1, "{:?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w.contains("xl/drawings/drawing1.xml")), "{:?}", report.warnings);
    let s = first(&wb);
    assert!(s.images.is_empty() && s.shapes.is_empty() && s.charts.is_empty());
}

#[test]
fn sheet_without_drawing_record_has_no_objects() {
    let (wb, report) = read_xlsb(&package(&sheet(None), &[])).unwrap();
    cells_intact(&wb);
    let s = first(&wb);
    assert!(s.images.is_empty() && s.shapes.is_empty() && s.charts.is_empty());
    assert!(!report.warnings.iter().any(|w| w.contains("drawing")), "{:?}", report.warnings);
}
