//! With no print area, a sheet prints from A1 to its last used cell or the far corner of any
//! object on it, and that includes embedded (OLE) objects and ActiveX controls, which are kept
//! for saving but not drawn. Expected page counts measured from Microsoft 365 Excel's own PDF
//! prints (Letter portrait, margins 0.7 / 0.75 in, default 64 px columns and 15 pt rows): a sheet
//! with no cells and one embedded object 707.25 pt wide and 427.5 pt tall at A1 prints two pages,
//! and so does a sheet whose ActiveX controls reach column P.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const CT: &str = "application/vnd.openxmlformats-officedocument";

/// A cell anchor: zero-based column, column offset (EMU), row, row offset (EMU).
type Corner = (u32, u32, u32, u32);

fn anchor(from: Corner, to: Corner) -> String {
    let corner =
        |c: Corner| format!("<xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff>", c.0, c.1, c.2, c.3);
    format!(r#"<anchor moveWithCells="1"><from>{}</from><to>{}</to></anchor>"#, corner(from), corner(to))
}

/// `<oleObjects>` as Excel 2010 and later write it: the entry with its place inside
/// `mc:Choice`, the plain entry as the fallback.
fn ole_modern(from: Corner, to: Corner) -> String {
    format!(
        r#"<oleObjects><mc:AlternateContent xmlns:mc="{NS_MC}"><mc:Choice Requires="x14"><oleObject progId="Word.Document.12" shapeId="1025" r:id="rId2"><objectPr defaultSize="0" r:id="rId3">{}</objectPr></oleObject></mc:Choice><mc:Fallback><oleObject progId="Word.Document.12" shapeId="1025" r:id="rId2"/></mc:Fallback></mc:AlternateContent></oleObjects>"#,
        anchor(from, to)
    )
}

/// `<oleObjects>` as Excel 2007 writes it: no place in the sheet part, only the shape id of the
/// VML shape that holds it.
fn ole_legacy() -> String {
    r#"<oleObjects><oleObject progId="Word.Document.12" shapeId="1025" r:id="rId2"/></oleObjects>"#.to_string()
}

/// `<controls>` holding one ActiveX control.
fn activex(from: Corner, to: Corner) -> String {
    format!(
        r#"<controls><mc:AlternateContent xmlns:mc="{NS_MC}"><mc:Choice Requires="x14"><control shapeId="1025" r:id="rId2" name="ToggleButton1"><controlPr defaultSize="0" autoLine="0" r:id="rId3">{}</controlPr></control></mc:Choice><mc:Fallback><control shapeId="1025" r:id="rId2" name="ToggleButton1"/></mc:Fallback></mc:AlternateContent></controls>"#,
        anchor(from, to)
    )
}

/// The legacy drawing part: one picture-frame shape `_x0000_s1025` at `vml_anchor`
/// (`colL, dxL, rowT, dyT, colR, dxR, rowB, dyB`, offsets in pixels).
fn vml(vml_anchor: &str) -> String {
    format!(
        r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f"><v:stroke joinstyle="miter"/><v:path o:extrusionok="f" gradientshapeok="t" o:connecttype="rect"/></v:shapetype><v:shape id="_x0000_s1025" type="#_x0000_t75" style='position:absolute;margin-left:0;margin-top:0;width:100pt;height:100pt;z-index:1' filled="t" stroked="t"><x:ClientData ObjectType="Pict"><x:SizeWithCells/><x:Anchor>{vml_anchor}</x:Anchor><x:CF>Pict</x:CF><x:AutoPict/></x:ClientData></v:shape></xml>"##
    )
}

/// What the sheet's `rId2` points at.
enum Kind {
    Ole,
    ActiveX,
}

/// One sheet "S" (Letter portrait, margins 0.7 / 0.75 in) whose cells `filled` (a range like
/// "A1:C5", or "" for none) hold numbers, with `objects` (an `<oleObjects>` or `<controls>`
/// element) after the legacy drawing reference.
fn package(filled: &str, objects: &str, kind: Kind, vml_anchor: &str) -> Vec<u8> {
    let mut data = String::new();
    if !filled.is_empty() {
        let (a, b) = filled.split_once(':').unwrap();
        let split = |r: &str| {
            let col = r.bytes().take_while(u8::is_ascii_alphabetic).fold(0u32, |n, c| n * 26 + u32::from(c - b'A') + 1);
            let row: u32 = r.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse().unwrap();
            (col, row)
        };
        let ((c0, r0), (c1, r1)) = (split(a), split(b));
        for r in r0..=r1 {
            data.push_str(&format!(r#"<row r="{r}">"#));
            for c in c0..=c1 {
                let letter = char::from(b'A' + (c - 1) as u8);
                data.push_str(&format!(r#"<c r="{letter}{r}"><v>{}</v></c>"#, r * 10 + c));
            }
            data.push_str("</row>");
        }
    }
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}" xmlns:xdr="{NS_XDR}" xmlns:mc="{NS_MC}"><sheetFormatPr defaultRowHeight="15"/><sheetData>{data}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait"/><legacyDrawing r:id="rId1"/>{objects}</worksheet>"#
    );
    let (object_rel, object_part, object_type, object_body): (&str, &str, &str, Vec<u8>) = match kind {
        Kind::Ole => ("oleObject", "xl/embeddings/oleObject1.bin", "application/vnd.openxmlformats-officedocument.oleObject", vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]),
        Kind::ActiveX => (
            "control",
            "xl/activeX/activeX1.xml",
            "application/vnd.ms-office.activeX+xml",
            br#"<?xml version="1.0"?><ax:ocx ax:classid="{8BD21D60-EC42-11CE-9E0D-00AA006002F3}" ax:persistence="persistPropertyBag" xmlns:ax="http://schemas.microsoft.com/office/2006/activeX"/>"#.to_vec(),
        ),
    };
    let target = object_part.trim_start_matches("xl/");
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="rId1" Type="{NS_R}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/><Relationship Id="rId2" Type="{NS_R}/{object_rel}" Target="../{target}"/><Relationship Id="rId3" Type="{NS_R}/image" Target="../media/image1.png"/></Relationships>"#
    );
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="vml" ContentType="{CT}.vmlDrawing"/><Default Extension="png" ContentType="image/png"/><Default Extension="bin" ContentType="{CT}.oleObject"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/{object_part}" ContentType="{object_type}"/></Types>"#
    );
    let one_rel = |kind: &str, target: &str| {
        format!(
            r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="rId1" Type="{NS_R}/{kind}" Target="{target}"/></Relationships>"#
        )
    };
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let parts: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", types.into_bytes()),
        ("_rels/.rels", one_rel("officeDocument", "xl/workbook.xml").into_bytes()),
        ("xl/workbook.xml", workbook.into_bytes()),
        ("xl/_rels/workbook.xml.rels", one_rel("worksheet", "worksheets/sheet1.xml").into_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.into_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.into_bytes()),
        ("xl/drawings/vmlDrawing1.vml", vml(vml_anchor).into_bytes()),
        ("xl/media/image1.png", PNG_1X1.to_vec()),
        (object_part, object_body),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

/// A 1 x 1 pixel PNG (the object's preview picture).
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x56, 0xC7, 0x2F, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "objects.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn pages(s: &mut Session) -> Vec<Json> {
    s.execute("file.printPreview", json!({})).unwrap()["pageList"].as_array().unwrap().clone()
}

fn range(p: &Json) -> &str {
    p["range"].as_str().unwrap()
}

/// The object Excel's print was measured with: A1 to column O + 447675 EMU, row 29 + 95250 EMU.
fn wide_object() -> Vec<u8> {
    package("", &ole_modern((0, 0, 0, 0), (14, 447_675, 28, 95_250)), Kind::Ole, "0, 0, 0, 0, 14, 47, 28, 10")
}

#[test]
fn ole_object_alone_prints_across_two_pages() {
    // Excel: 2 pages (the object is 707.25 pt wide; the page holds 511.2 pt).
    let mut s = open(&wide_object());
    let p = pages(&mut s);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(range(&p[0]).starts_with("A1:"), "{p:?}");
    assert!(range(&p[1]).ends_with("O29"), "{p:?}");
}

#[test]
fn ole_object_below_the_cells_extends_the_rows() {
    let objects = ole_modern((0, 0, 60, 0), (2, 100_000, 62, 100_000));
    let mut s = open(&package("A1:A3", &objects, Kind::Ole, "0, 0, 60, 0, 2, 10, 62, 10"));
    let p = pages(&mut s);
    assert!(range(&p[0]).starts_with("A1:"), "{p:?}");
    assert!(range(p.last().unwrap()).ends_with("C63"), "{p:?}");
}

#[test]
fn ole_object_without_object_pr_uses_the_vml_anchor() {
    // Excel 2007's form: the object's place is only in the VML shape `_x0000_s1025`, here from
    // A19 to column H + 32 px, row 40 + 3 px.
    let mut s = open(&package("A1:I10", &ole_legacy(), Kind::Ole, "0, 0, 18, 0, 7, 32, 39, 3"));
    let p = pages(&mut s);
    assert!(range(&p[0]).starts_with("A1:"), "{p:?}");
    assert!(range(p.last().unwrap()).ends_with("I40"), "{p:?}");
}

#[test]
fn activex_control_extends_the_printed_range() {
    // Excel: a sheet with no cells whose ActiveX controls reach column P prints 2 pages.
    let objects = activex((13, 0, 9, 0), (15, 285_750, 15, 114_300));
    let mut s = open(&package("", &objects, Kind::ActiveX, "13, 0, 9, 0, 15, 30, 15, 12"));
    let p = pages(&mut s);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(range(&p[0]).starts_with("A1:"), "{p:?}");
    assert!(range(&p[1]).ends_with("P16"), "{p:?}");
}

#[test]
fn object_inside_the_cells_changes_nothing() {
    let objects = ole_modern((0, 0, 0, 0), (1, 100_000, 2, 100_000));
    let mut s = open(&package("A1:C5", &objects, Kind::Ole, "0, 0, 0, 0, 1, 10, 2, 10"));
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(range(&p[0]), "A1:C5");
}

#[test]
fn ole_object_in_alternate_content_is_counted_once_and_kept_on_save() {
    let mut s = open(&wide_object());
    let saved = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap();
    let bytes = gridcraft_engine::io::base64_decode(saved["base64"].as_str().unwrap()).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut sheet = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut sheet).unwrap();
    assert!(sheet.contains("<oleObjects"), "the saved sheet lost its embedded object: {sheet}");
    assert_eq!(sheet.matches("<objectPr").count(), 1, "{sheet}");
    let mut again = open(&bytes);
    let p = pages(&mut again);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(range(&p[1]).ends_with("O29"), "{p:?}");
}
