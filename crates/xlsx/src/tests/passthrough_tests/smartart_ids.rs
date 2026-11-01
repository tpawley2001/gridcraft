//! A SmartArt graphic whose drawing relationships aren't numbered from rId1 (a picture comes
//! first, the diagram drawing has a high id) keeps every id the anchor and the data part name,
//! through `write_xlsx` and the named save the engine uses, and beside pictures GridCraft adds.

use std::collections::HashMap;
use std::io::Read;

use super::rels;
use crate::tests::make_zip;
use crate::{XlsxKind, read_xlsx, write_xlsx, write_xlsx_named};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const DSP: &str = "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n picture";
const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/diagrams/data1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml"/><Override PartName="/xl/diagrams/layout1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml"/><Override PartName="/xl/diagrams/quickStyle1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml"/><Override PartName="/xl/diagrams/colors1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml"/><Override PartName="/xl/diagrams/drawing1.xml" ContentType="application/vnd.ms-office.drawingml.diagramDrawing+xml"/></Types>"#;
const DATA: &str = r#"<?xml version="1.0"?><dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dgm:ptLst/><dgm:extLst><a:ext uri="http://schemas.microsoft.com/office/drawing/2008/diagram"><dsp:dataModelExt xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" relId="rId9" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:ext></dgm:extLst></dgm:dataModel>"#;
const PART: &str = r#"<?xml version="1.0"?><x xmlns="urn:example"/>"#;

fn anchor(col: u32, body: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{body}<xdr:clientData/></xdr:twoCellAnchor>"#,
        col + 2
    )
}

fn package() -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let sheet = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData/><drawing r:id="rId1"/></worksheet>"#);
    let pic = anchor(
        0,
        r#"<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"#,
    );
    let smartart = anchor(
        4,
        r#"<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="3" name="Diagram 2"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId2" r:lo="rId3" r:qs="rId4" r:cs="rId5"/></a:graphicData></a:graphic></xdr:graphicFrame>"#,
    );
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{pic}{smartart}</xdr:wsDr>"#
    );
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml")]);
    let sheet_rels = rels(&[("rId1", "drawing", "../drawings/drawing1.xml")]);
    let drawing_rels = rels(&[
        ("rId1", "image", "../media/image1.png"),
        ("rId2", "diagramData", "../diagrams/data1.xml"),
        ("rId3", "diagramLayout", "../diagrams/layout1.xml"),
        ("rId4", "diagramQuickStyle", "../diagrams/quickStyle1.xml"),
        ("rId5", "diagramColors", "../diagrams/colors1.xml"),
        ("rId9", DSP, "../diagrams/drawing1.xml"),
    ]);
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/diagrams/data1.xml", DATA.as_bytes()),
        ("xl/diagrams/layout1.xml", PART.as_bytes()),
        ("xl/diagrams/quickStyle1.xml", PART.as_bytes()),
        ("xl/diagrams/colors1.xml", PART.as_bytes()),
        ("xl/diagrams/drawing1.xml", PART.as_bytes()),
        ("xl/media/image1.png", PNG),
    ])
}

fn unpack(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut v = vec![];
            f.read_to_end(&mut v).unwrap();
            (f.name().to_string(), String::from_utf8_lossy(&v).into_owned())
        })
        .collect()
}

/// The written drawing's relationships as (id, type) pairs.
fn drawing_rels(p: &HashMap<String, String>) -> Vec<(String, String)> {
    let x = p.iter().find(|(k, _)| k.starts_with("xl/drawings/_rels/")).map(|(_, v)| v).expect("no drawing rels");
    let el = crate::xml::parse(x.as_bytes()).unwrap();
    el.children.iter().map(|r| (r.attr("Id").unwrap_or("").to_string(), r.attr("Type").unwrap_or("").to_string())).collect()
}

fn assert_ids_kept(out: &[u8]) {
    let p = unpack(out);
    let rels = drawing_rels(&p);
    let ty = |id: &str| rels.iter().find(|(i, _)| i == id).map(|(_, t)| t.clone()).unwrap_or_default();
    assert!(ty("rId2").ends_with("/diagramData"), "{rels:?}");
    assert!(ty("rId5").ends_with("/diagramColors"), "{rels:?}");
    assert_eq!(ty("rId9"), DSP, "the id the data part names: {rels:?}");
    let mut ids: Vec<&str> = rels.iter().map(|(i, _)| i.as_str()).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate relationship ids: {rels:?}");
    assert!(rels.iter().any(|(_, t)| t.ends_with("/image")), "picture relationship lost: {rels:?}");
    let drawing = p.iter().find(|(k, _)| k.starts_with("xl/drawings/drawing")).map(|(_, v)| v).unwrap();
    assert!(drawing.contains(r#"r:dm="rId2""#) && drawing.contains("<xdr:pic"), "{drawing}");
}

#[test]
fn smartart_keeps_source_relationship_ids() {
    let (wb, rep) = read_xlsx(&package()).unwrap();
    assert!(!rep.warnings.iter().any(|w| w.contains("was skipped")), "{:?}", rep.warnings);
    assert_ids_kept(&write_xlsx(&wb).unwrap());
    assert_ids_kept(&write_xlsx_named(&wb, XlsxKind::Workbook, "book.xlsx").unwrap());
}

#[test]
fn smartart_ids_survive_added_pictures() {
    let (mut wb, _) = read_xlsx(&package()).unwrap();
    let sheet = std::sync::Arc::make_mut(&mut wb.sheets[0]);
    for _ in 0..8 {
        let img = sheet.images[0].clone();
        sheet.images.push(img);
    }
    let out = write_xlsx(&wb).unwrap();
    assert_ids_kept(&out);
    let (wb2, _) = read_xlsx(&out).unwrap();
    assert_eq!(wb2.sheet(0).unwrap().images.len(), 9);
}
