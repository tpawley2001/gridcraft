//! Hyperlinks on shapes, text boxes and pictures (`a:hlinkClick` on the object's `xdr:cNvPr`,
//! resolved through the drawing's relationships) survive open + save. Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

fn anchor(col: u32, row: u32, body: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{body}<xdr:clientData/></xdr:twoCellAnchor>"#,
        col + 3,
        row + 4
    )
}

fn text_box(id: u32, link: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="TextBox {id}">{link}</xdr:cNvPr><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1905000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Go there</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#
    )
}

fn shape(id: u32, link: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="Rectangle {id}">{link}</xdr:cNvPr><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1905000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="4472C4"/></a:solidFill></xdr:spPr></xdr:sp>"#
    )
}

fn picture(id: u32, link: &str, embed: &str) -> String {
    format!(
        r#"<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="{id}" name="Picture {id}">{link}</xdr:cNvPr><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="{embed}"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"#
    )
}

/// One sheet "Data" (plus an empty "Sheet2") whose drawing holds `anchors`; `rels` are the
/// drawing's relationships as (id, type, target, external).
fn package(anchors: &[String], rels: &[(&str, &str, &str, bool)]) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Sheet2" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{NS_R}/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let sheet1 = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#);
    let sheet2 = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData/></worksheet>"#);
    let sheet1_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{}</xdr:wsDr>"#,
        anchors.concat()
    );
    let mut d_rels = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target, ext) in rels {
        let mode = if *ext { r#" TargetMode="External""# } else { "" };
        d_rels.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"{mode}/>"#));
    }
    d_rels.push_str("</Relationships>");
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet1_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", d_rels.as_bytes()),
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

/// The links written for the drawing objects of sheet "Data": (object name, target, external,
/// tooltip), resolved through the saved package's relationships.
fn written_links(out: &[u8]) -> Vec<(String, String, bool, Option<String>)> {
    let p = unpack(out);
    let (dname, dx) = p
        .iter()
        .find(|(n, _)| n.starts_with("xl/drawings/") && n.ends_with(".xml") && !n.contains("_rels"))
        .map(|(n, x)| (n.clone(), x.clone()))
        .expect("no drawing part written");
    let file = dname.rsplit('/').next().unwrap();
    let rels = p.get(&format!("xl/drawings/_rels/{file}.rels")).cloned().unwrap_or_default();
    let mut links = Vec::new();
    for (i, _) in dx.match_indices("<xdr:cNvPr ") {
        let end = dx[i..].find("</xdr:cNvPr>").map(|e| i + e).unwrap_or(i);
        let nv = &dx[i..end];
        let Some(name) = between(nv, "name=\"", "\"") else { continue };
        let Some(h) = nv.find("hlinkClick") else { continue };
        let tag = &nv[h..h + nv[h..].find('>').unwrap()];
        let rid = between(tag, "r:id=\"", "\"").expect("hlinkClick without r:id");
        let tooltip = between(tag, "tooltip=\"", "\"");
        let ri = rels.find(&format!("Id=\"{rid}\"")).unwrap_or_else(|| panic!("no relationship {rid} in {rels}"));
        let start = rels[..ri].rfind('<').unwrap();
        let rel = &rels[start..start + rels[start..].find("/>").unwrap()];
        assert!(rel.contains("/relationships/hyperlink\""), "not a hyperlink relationship: {rel}");
        let target = between(rel, "Target=\"", "\"").unwrap();
        links.push((name, target, rel.contains("TargetMode=\"External\""), tooltip));
    }
    links.sort();
    links
}

/// Open, save, open the result, save again: both saves must write `want`.
fn assert_links_survive(pkg: Vec<u8>, want: &[(&str, &str, bool, Option<&str>)]) {
    let want: Vec<(String, String, bool, Option<String>)> =
        want.iter().map(|(n, t, e, tip)| (n.to_string(), t.to_string(), *e, tip.map(str::to_string))).collect();
    let (wb, _) = read_xlsx(&pkg).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_eq!(written_links(&out), want, "first save");
    let (wb2, _) = read_xlsx(&out).unwrap();
    let out2 = write_xlsx(&wb2).unwrap();
    assert_eq!(written_links(&out2), want, "second save");
}

#[test]
fn text_box_link_and_tooltip_survive() {
    let link = r#"<a:hlinkClick r:id="rId1" tooltip="text123"/>"#;
    assert_links_survive(
        package(&[anchor(1, 1, &text_box(2, link))], &[("rId1", "hyperlink", "https://www.example.com/", true)]),
        &[("TextBox 2", "https://www.example.com/", true, Some("text123"))],
    );
}

#[test]
fn picture_link_survives() {
    let link = r#"<a:hlinkClick r:id="rId2"/>"#;
    assert_links_survive(
        package(
            &[anchor(0, 6, &picture(3, link, "rId1"))],
            &[("rId1", "image", "../media/image1.png", false), ("rId2", "hyperlink", "https://www.example.com/pic", true)],
        ),
        &[("Picture 3", "https://www.example.com/pic", true, None)],
    );
}

#[test]
fn in_workbook_link_has_no_target_mode() {
    let link = r#"<a:hlinkClick r:id="rId1"/>"#;
    assert_links_survive(
        package(&[anchor(1, 1, &shape(4, link))], &[("rId1", "hyperlink", "#Sheet2!A1", false)]),
        &[("Rectangle 4", "#Sheet2!A1", false, None)],
    );
}

#[test]
fn url_with_ampersand_is_escaped_and_kept() {
    let link = r#"<a:hlinkClick r:id="rId1" tooltip="Q &amp; A"/>"#;
    assert_links_survive(
        package(&[anchor(1, 1, &text_box(5, link))], &[("rId1", "hyperlink", "https://example.com/?a=1&amp;b=2", true)]),
        &[("TextBox 5", "https://example.com/?a=1&amp;b=2", true, Some("Q &amp; A"))],
    );
}
