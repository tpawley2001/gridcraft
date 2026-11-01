//! Grouped drawing objects (`xdr:grpSp`, ECMA-376 Part 1 §20.5.2.17: pictures, text boxes and
//! shapes in a group) survive open + save: the group's anchor XML and the images it uses are kept.
//! Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
/// A second, different image (the bytes are only compared, never decoded).
const PNG_B: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0aSECOND-IMAGE";
/// A third image, for the modelled picture.
const PNG_C: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0aTHIRD-IMAGE";

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

fn anchor(col: u32, row: u32, body: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>457200</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{body}<xdr:clientData/></xdr:twoCellAnchor>"#,
        col + 1,
        row + 5
    )
}

fn pic(id: u32, name: &str, rid: &str) -> String {
    format!(
        r#"<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="{id}" name="{name}"/><xdr:cNvPicPr><a:picLocks noChangeAspect="1"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="{rid}"/><a:srcRect/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr bwMode="auto"><a:xfrm><a:off x="1080" y="697"/><a:ext cx="2160" cy="1215"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"#
    )
}

fn text_box(id: u32) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="Text Box {id}"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr bwMode="auto"><a:xfrm><a:off x="-27412008195" y="9048750"/><a:ext cx="2340" cy="0"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></xdr:spPr><xdr:txBody><a:bodyPr vertOverflow="clip" wrap="square"/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1000"/><a:t>GROUP-TEXT</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#
    )
}

fn group(id: u32, name: &str, children: &str) -> String {
    format!(
        r#"<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id="{id}" name="{name}"><a:extLst><a:ext uri="{{FF2B5EF4-FFF2-40B4-BE49-F238E27FC236}}"><a16:creationId xmlns:a16="http://schemas.microsoft.com/office/drawing/2014/main" id="{{BFA8D545-9671-49E8-8C26-0C8B6FE6D0D4}}"/></a:ext></a:extLst></xdr:cNvPr><xdr:cNvGrpSpPr><a:grpSpLocks/></xdr:cNvGrpSpPr></xdr:nvGrpSpPr><xdr:grpSpPr bwMode="auto"><a:xfrm><a:off x="762000" y="7286625"/><a:ext cx="457200" cy="0"/><a:chOff x="1080" y="697"/><a:chExt cx="2340" cy="1459"/></a:xfrm></xdr:grpSpPr>{children}</xdr:grpSp>"#
    )
}

/// Sheet "Data" with a drawing made of `anchors`; the drawing's rels: rId1 -> image1 (PNG),
/// rId2 -> image2 (PNG_B), rId3 -> image3 (PNG_C).
fn package(anchors: &str) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#
    );
    let sheet1_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    let drawing = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{anchors}</xdr:wsDr>"#
    );
    let drawing_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/image" Target="../media/image1.png"/><Relationship Id="rId2" Type="{NS_R}/image" Target="../media/image2.png"/><Relationship Id="rId3" Type="{NS_R}/image" Target="../media/image3.png"/></Relationships>"#
    );
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet1_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/media/image1.png", PNG),
        ("xl/media/image2.png", PNG_B),
        ("xl/media/image3.png", PNG_C),
    ])
}

/// The group of a picture (rId1) and a text box, alone in its drawing (ids 2, 3, 4).
fn simple_group() -> String {
    anchor(1, 25, &group(2, "Group 1", &format!("{}{}", pic(3, "Picture 2", "rId1"), text_box(4))))
}

fn raw_part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).ok()?;
    let mut v = vec![];
    f.read_to_end(&mut v).unwrap();
    Some(v)
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

/// Every `r:embed` in `x` in document order.
fn embeds(x: &str) -> Vec<String> {
    x.match_indices("r:embed=\"").filter_map(|(i, m)| between(&x[i..], m, "\"")).collect()
}

fn cnvpr_ids(x: &str) -> Vec<String> {
    x.match_indices("<xdr:cNvPr id=\"").filter_map(|(i, m)| between(&x[i..], m, "\"")).collect()
}

/// `out` keeps the simple group: drawing, names, text, the unparsed offset and the image bytes.
fn assert_simple_group_kept(out: &[u8]) {
    let p = unpack(out);
    let drawing = data_drawing(&p);
    let dx = p.get(&drawing).unwrap_or_else(|| panic!("no part {drawing}"));
    assert!(dx.contains("<xdr:grpSp>"), "group lost: {dx}");
    assert!(dx.contains("name=\"Group 1\""), "group name lost: {dx}");
    assert!(dx.contains("GROUP-TEXT"), "text box text lost: {dx}");
    assert!(dx.contains("x=\"-27412008195\""), "offset changed: {dx}");
    let e = embeds(dx);
    assert_eq!(e.len(), 1, "one picture expected: {dx}");
    let (t, part) = rel(&p, &drawing, &e[0]).unwrap_or_else(|| panic!("r:embed {} does not resolve", e[0]));
    assert!(t.ends_with("/image"), "image rel type {t}");
    assert_eq!(raw_part(out, &part).as_deref(), Some(PNG), "image bytes of {part}");
}

#[test]
fn group_alone_keeps_drawing_and_images() {
    let (wb, _) = read_xlsx(&package(&simple_group())).unwrap();
    assert_simple_group_kept(&write_xlsx(&wb).unwrap());
}

#[test]
fn group_survives_second_save() {
    let (wb, _) = read_xlsx(&package(&simple_group())).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (wb2, _) = read_xlsx(&out).unwrap();
    assert_simple_group_kept(&write_xlsx(&wb2).unwrap());
}

#[test]
fn group_next_to_picture_keeps_both_in_order() {
    let anchors = format!("{}{}", anchor(0, 1, &pic(5, "Lone Picture", "rId3")), simple_group());
    let (wb, _) = read_xlsx(&package(&anchors)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().images.len(), 1, "the lone picture is modelled");
    let out = write_xlsx(&wb).unwrap();
    assert_simple_group_kept_beside(&out);
}

fn assert_simple_group_kept_beside(out: &[u8]) {
    let p = unpack(out);
    let drawing = data_drawing(&p);
    let dx = &p[&drawing];
    let g = dx.find("<xdr:grpSp>").unwrap_or_else(|| panic!("group lost: {dx}"));
    let lone = dx.find("<xdr:pic").unwrap_or_else(|| panic!("picture lost: {dx}"));
    assert!(lone < g, "the modelled picture's anchor must stay before the group: {dx}");
    let images: Vec<Vec<u8>> =
        embeds(dx).iter().map(|id| raw_part(out, &rel(&p, &drawing, id).expect("r:embed resolves").1).expect("image part")).collect();
    assert!(images.iter().any(|b| b == PNG), "group picture image lost");
    assert!(images.iter().any(|b| b == PNG_C), "modelled picture image lost");
}

#[test]
fn nested_group_kept() {
    let inner = group(3, "Inner Group", &format!("{}{}", pic(4, "Picture A", "rId1"), pic(5, "Picture B", "rId2")));
    let outer = anchor(2, 3, &group(2, "Outer Group", &format!("{inner}{}", text_box(6))));
    let (wb, _) = read_xlsx(&package(&outer)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let p = unpack(&out);
    let drawing = data_drawing(&p);
    let dx = &p[&drawing];
    assert_eq!(dx.matches("<xdr:grpSp>").count(), 2, "both groups kept: {dx}");
    let e = embeds(dx);
    assert_eq!(e.len(), 2, "two pictures: {dx}");
    let a = raw_part(&out, &rel(&p, &drawing, &e[0]).expect("first r:embed").1);
    let b = raw_part(&out, &rel(&p, &drawing, &e[1]).expect("second r:embed").1);
    assert_eq!(a.as_deref(), Some(PNG), "Picture A image");
    assert_eq!(b.as_deref(), Some(PNG_B), "Picture B image");
}

#[test]
fn group_ids_do_not_collide() {
    // The modelled picture's source id (2) collides with the group's own id on purpose.
    let anchors = format!("{}{}", simple_group(), anchor(0, 1, &pic(2, "Lone Picture", "rId3")));
    let (wb, _) = read_xlsx(&package(&anchors)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let p = unpack(&out);
    let dx = &p[&data_drawing(&p)];
    let mut ids = cnvpr_ids(dx);
    assert_eq!(ids.len(), 4, "group, its picture and text box, and the lone picture: {dx}");
    ids.sort();
    let mut dedup = ids.clone();
    dedup.dedup();
    assert_eq!(ids, dedup, "duplicate cNvPr ids: {dx}");
}

#[test]
fn group_reports_kept_warning() {
    let (_, rep) = read_xlsx(&package(&simple_group())).unwrap();
    assert!(rep.warnings.iter().any(|w| w.contains("grouped drawing objects are not shown")), "{:?}", rep.warnings);
    assert!(!rep.warnings.iter().any(|w| w.contains("grouped shapes are not supported")), "{:?}", rep.warnings);
}
