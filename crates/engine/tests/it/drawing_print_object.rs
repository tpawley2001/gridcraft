//! A drawing object with "Print object" turned off (`xdr:clientData fPrintsWithSheet="0"`,
//! ECMA-376 Part 1 §20.5.2.3) is not printed, doesn't extend the automatic print range, and keeps
//! the setting on save, like Excel. It is not hidden: it still shows on screen.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument";

fn rels(id: &str, kind: &str, target: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="{id}" Type="{NS_R}/{kind}" Target="{target}"/></Relationships>"#
    )
}

/// Sheet "S": values in A1:B2 and a text box anchored from A1 to K51 whose anchor ends in
/// `<xdr:clientData{client}/>`.
fn package(client: &str) -> Vec<u8> {
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>10</xdr:col><xdr:colOff>381000</xdr:colOff><xdr:row>50</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Box"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="9525000" cy="9525000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></xdr:spPr><xdr:txBody><a:bodyPr/><a:p><a:r><a:t>note</a:t></a:r></a:p></xdr:txBody></xdr:sp><xdr:clientData{client}/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row><row r="2"><c r="A2"><v>3</v></c><c r="B2"><v>4</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#
    );
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/></Types>"#
            ),
        ),
        ("_rels/.rels", rels("rId1", "officeDocument", "xl/workbook.xml")),
        (
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
        ),
        ("xl/_rels/workbook.xml.rels", rels("rId1", "worksheet", "worksheets/sheet1.xml")),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/_rels/sheet1.xml.rels", rels("rId1", "drawing", "../drawings/drawing1.xml")),
        ("xl/drawings/drawing1.xml", drawing),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "print_object.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn pages(s: &mut Session) -> Vec<Json> {
    s.execute("file.printPreview", json!({})).unwrap()["pageList"].as_array().unwrap().clone()
}

/// The saved `xl/drawings/drawing1.xml`.
fn saved_drawing(s: &Session) -> String {
    let saved = gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "print_object.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(saved)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

/// The first `<tag ...>` start tag in `xml`.
fn start_tag<'a>(xml: &'a str, tag: &str) -> &'a str {
    let start = xml.find(&format!("<{tag}")).unwrap_or_else(|| panic!("no {tag} in {xml}"));
    &xml[start..start + xml[start..].find('>').unwrap()]
}

const OFF: &str = r#" fPrintsWithSheet="0""#;

#[test]
fn non_printing_shape_does_not_extend_the_printed_range() {
    let mut s = open(&package(OFF));
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0]["range"], "A1:B2");
}

#[test]
fn non_printing_shape_false_spelling_does_not_extend_the_printed_range() {
    let mut s = open(&package(r#" fPrintsWithSheet="false""#));
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0]["range"], "A1:B2");
}

#[test]
fn non_printing_shape_pdf_is_one_page() {
    let mut s = open(&package(OFF));
    assert_eq!(s.execute("file.exportPdf", json!({})).unwrap()["pages"], 1);
}

#[test]
fn printing_shape_still_extends_the_printed_range() {
    for client in ["", r#" fPrintsWithSheet="1""#] {
        let mut s = open(&package(client));
        let p = pages(&mut s);
        assert_ne!(p[0]["range"], "A1:B2", "{client}: {p:?}");
        assert!(p[0]["range"].as_str().unwrap().starts_with("A1:"), "{client}: {p:?}");
    }
}

#[test]
fn print_object_off_is_kept_on_save_and_not_turned_into_hidden() {
    let s = open(&package(OFF));
    let xml = saved_drawing(&s);
    let client = start_tag(&xml, "xdr:clientData");
    assert!(client.contains(r#"fPrintsWithSheet="0""#), "{client}");
    let nv = start_tag(&xml, "xdr:cNvPr ");
    assert!(!nv.contains("hidden="), "{nv}");
}

#[test]
fn default_print_object_writes_a_bare_client_data() {
    let s = open(&package(""));
    let xml = saved_drawing(&s);
    let client = start_tag(&xml, "xdr:clientData");
    assert!(!client.contains("fPrintsWithSheet"), "{client}");
}
