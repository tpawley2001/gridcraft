//! SmartArt graphics (a drawing `graphicFrame` whose `dgm:relIds` name four diagram parts, plus
//! the pre-rendered diagram drawing named from the data part) survive open + save unchanged.
//! Packages are built in code; the diagram parts are small synthetic stand-ins.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const MS_DIAGRAM_DRAWING: &str = "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/diagrams/data1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml"/><Override PartName="/xl/diagrams/layout1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml"/><Override PartName="/xl/diagrams/quickStyle1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml"/><Override PartName="/xl/diagrams/colors1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml"/><Override PartName="/xl/diagrams/drawing1.xml" ContentType="application/vnd.ms-office.drawingml.diagramDrawing+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

const DATA: &str = r#"<?xml version="1.0"?><dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dgm:ptLst><dgm:pt modelId="{11111111-0000-0000-0000-000000000001}" type="doc"><dgm:prSet/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></dgm:t></dgm:pt><dgm:pt modelId="{11111111-0000-0000-0000-000000000002}"><dgm:prSet phldrT="[Text]"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>DATA-MARKER step one</a:t></a:r></a:p></dgm:t></dgm:pt></dgm:ptLst><dgm:cxnLst><dgm:cxn modelId="{11111111-0000-0000-0000-000000000003}" srcId="{11111111-0000-0000-0000-000000000001}" destId="{11111111-0000-0000-0000-000000000002}" srcOrd="0" destOrd="0"/></dgm:cxnLst><dgm:bg/><dgm:whole/><dgm:extLst><a:ext uri="http://schemas.microsoft.com/office/drawing/2008/diagram"><dsp:dataModelExt xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" relId="rId5" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:ext></dgm:extLst></dgm:dataModel>"#;
const LAYOUT: &str = r#"<?xml version="1.0"?><dgm:layoutDef xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" uniqueId="urn:example/layout/LAYOUT-MARKER"><dgm:title val=""/><dgm:desc val=""/><dgm:layoutNode name="root"/></dgm:layoutDef>"#;
const QUICK_STYLE: &str = r#"<?xml version="1.0"?><dgm:styleDef xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" uniqueId="urn:example/quickstyle/STYLE-MARKER"><dgm:title val=""/><dgm:desc val=""/></dgm:styleDef>"#;
const COLORS: &str = r#"<?xml version="1.0"?><dgm:colorsDef xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" uniqueId="urn:example/colors/COLORS-MARKER"><dgm:title val=""/><dgm:desc val=""/></dgm:colorsDef>"#;
const DIAGRAM_DRAWING: &str = r#"<?xml version="1.0"?><dsp:drawing xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dsp:spTree><dsp:nvGrpSpPr><dsp:cNvPr id="0" name=""/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr/><dsp:sp modelId="{22222222-0000-0000-0000-000000000001}"><dsp:nvSpPr><dsp:cNvPr id="0" name=""/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></dsp:spPr><dsp:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>DRAWING-MARKER step one</a:t></a:r></a:p></dsp:txBody></dsp:sp></dsp:spTree></dsp:drawing>"#;

fn anchor(col: u32, row: u32, body: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{body}<xdr:clientData/></xdr:twoCellAnchor>"#,
        col + 5,
        row + 10
    )
}

const SMARTART: &str = r#"<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="3" name="Diagram 2"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId1" r:lo="rId2" r:qs="rId3" r:cs="rId4"/></a:graphicData></a:graphic></xdr:graphicFrame>"#;
const PICTURE: &str = r#"<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId6"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"#;

/// Sheet "Data" with a SmartArt graphic (and with `picture`, a picture in the same drawing);
/// sheet "Other" is plain.
fn package(picture: bool) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{NS_R}/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let sheet1 = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#);
    let sheet2 = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>2</v></c></row></sheetData></worksheet>"#);
    let sheet1_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
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
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet1_rels.as_bytes()),
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

fn unpack(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = vec![];
        f.read_to_end(&mut v).unwrap();
        parts.insert(f.name().to_string(), String::from_utf8_lossy(&v).into_owned());
    }
    parts
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    Some(s[i..i + s[i..].find(b)?].to_string())
}

fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// The relationship `id` of `part`: (type, resolved part name).
fn rel(p: &HashMap<String, String>, part: &str, id: &str) -> Option<(String, String)> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let rels = p.get(&format!("{dir}/_rels/{file}.rels"))?;
    let i = rels.find(&format!("Id=\"{id}\""))?;
    let start = rels[..i].rfind('<')?;
    let r = &rels[start..start + rels[start..].find("/>")?];
    let target = between(r, "Target=\"", "\"")?;
    let full = match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => normalize(&format!("{dir}/{target}")),
    };
    Some((between(r, "Type=\"", "\"")?, full))
}

/// The drawing part of sheet "Data" in a saved package.
fn data_drawing(p: &HashMap<String, String>) -> String {
    let wb = &p["xl/workbook.xml"];
    let i = wb.find("name=\"Data\"").expect("no sheet Data");
    let rid = between(&wb[i..], "r:id=\"", "\"").unwrap();
    let (_, sheet) = rel(p, "xl/workbook.xml", &rid).unwrap();
    let sx = &p[&sheet];
    let j = sx.find("<drawing ").expect("sheet Data lost its <drawing>");
    let did = between(&sx[j..], "r:id=\"", "\"").unwrap();
    rel(p, &sheet, &did).expect("drawing relationship").1
}

/// The SmartArt graphic in `out` is complete: frame, the four `dgm:relIds` parts and the
/// diagram drawing named by the data part, each with its content type and original content.
fn assert_smartart_kept(out: &[u8]) {
    let p = unpack(out);
    let drawing = data_drawing(&p);
    let dx = p.get(&drawing).unwrap_or_else(|| panic!("no part {drawing}"));
    let relids = between(dx, "<dgm:relIds", "/>").unwrap_or_else(|| panic!("SmartArt frame lost: {dx}"));
    let ct = &p["[Content_Types].xml"];
    for (attr, ty, marker, content_type) in [
        ("r:dm", "diagramData", "DATA-MARKER", "drawingml.diagramData+xml"),
        ("r:lo", "diagramLayout", "LAYOUT-MARKER", "drawingml.diagramLayout+xml"),
        ("r:qs", "diagramQuickStyle", "STYLE-MARKER", "drawingml.diagramStyle+xml"),
        ("r:cs", "diagramColors", "COLORS-MARKER", "drawingml.diagramColors+xml"),
    ] {
        let id = between(&relids, &format!("{attr}=\""), "\"").unwrap_or_else(|| panic!("{attr} missing: {relids}"));
        let (t, part) = rel(&p, &drawing, &id).unwrap_or_else(|| panic!("{attr}={id} does not resolve"));
        assert!(t.ends_with(&format!("/{ty}")), "{attr} has type {t}");
        assert!(p.get(&part).is_some_and(|x| x.contains(marker)), "{part} missing or changed");
        assert!(ct.contains(&format!("PartName=\"/{part}\"")) && ct.contains(content_type), "content type for {part}");
    }
    let dm = between(&relids, "r:dm=\"", "\"").unwrap();
    let data = &p[&rel(&p, &drawing, &dm).unwrap().1];
    let dsp_id = between(data, "relId=\"", "\"").expect("dataModelExt relId");
    let (t, part) = rel(&p, &drawing, &dsp_id).unwrap_or_else(|| panic!("diagram drawing {dsp_id} does not resolve from the drawing"));
    assert_eq!(t, MS_DIAGRAM_DRAWING);
    assert!(p.get(&part).is_some_and(|x| x.contains("DRAWING-MARKER")), "{part} missing or changed");
    assert!(ct.contains("drawingml.diagramDrawing+xml"), "diagram drawing content type");
}

fn cnvpr_ids(x: &str) -> Vec<String> {
    let mut ids: Vec<String> = x.match_indices("<xdr:cNvPr id=\"").filter_map(|(i, m)| between(&x[i..], m, "\"")).collect();
    ids.sort();
    ids
}

#[test]
fn smartart_survives_two_saves() {
    let (wb, rep) = read_xlsx(&package(false)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_smartart_kept(&out);
    let (wb2, _) = read_xlsx(&out).unwrap();
    assert_smartart_kept(&write_xlsx(&wb2).unwrap());
    assert!(rep.warnings.iter().any(|w| w.contains("SmartArt")), "expected a 'not shown, kept' warning: {:?}", rep.warnings);
}

#[test]
fn smartart_beside_a_picture_keeps_both() {
    let (wb, _) = read_xlsx(&package(true)).unwrap();
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

#[test]
fn renamed_sheet_keeps_smartart() {
    let (mut wb, _) = read_xlsx(&package(false)).unwrap();
    Arc::make_mut(&mut wb.sheets[0]).name = "Renamed".into();
    let out = write_xlsx(&wb).unwrap();
    let p = unpack(&out);
    let wbx = p["xl/workbook.xml"].replace("name=\"Renamed\"", "name=\"Data\"");
    let mut p2 = p.clone();
    p2.insert("xl/workbook.xml".into(), wbx);
    let drawing = data_drawing(&p2);
    assert!(p2[&drawing].contains("<dgm:relIds"), "SmartArt lost after a rename");
}

#[test]
fn deleted_sheet_drops_smartart_parts() {
    let (mut wb, _) = read_xlsx(&package(false)).unwrap();
    wb.sheets.remove(0);
    let p = unpack(&write_xlsx(&wb).unwrap());
    assert!(!p.values().any(|x| x.contains("DATA-MARKER") || x.contains("DRAWING-MARKER")), "diagram parts of a deleted sheet were written");
    let ct = &p["[Content_Types].xml"];
    assert!(!ct.contains("/xl/diagrams/"), "content types name dropped diagram parts");
}
