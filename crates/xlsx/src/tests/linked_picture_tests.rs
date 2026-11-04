//! Linked pictures (`a:blip r:link` naming an external image relationship, ECMA-376 Part 1
//! 20.1.8.13) survive open + save, as in Excel. Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
const URL: &str = "http://localhost:8080/blue_square.png";

fn package(blip_attrs: &str, rels: &[(&str, &str, bool)]) -> Vec<u8> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>"#;
    let root = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#
    );
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}"><xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>6</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 6" descr="blue square"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip {blip_attrs}/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let mut d_rels = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, target, ext) in rels {
        let mode = if *ext { r#" TargetMode="External""# } else { "" };
        d_rels.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/image" Target="{target}"{mode}/>"#));
    }
    d_rels.push_str("</Relationships>");
    make_zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", d_rels.as_bytes()),
        ("xl/media/image1.png", PNG),
    ])
}

fn unpack(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = vec![];
        f.read_to_end(&mut v).unwrap();
        parts.insert(f.name().to_string(), v);
    }
    parts
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let i = tag.find(&key)? + key.len();
    Some(tag[i..i + tag[i..].find('"')?].to_string())
}

/// The saved picture: (r:embed target, r:link target and whether its relationship is external).
struct Saved {
    embed: Option<Vec<u8>>,
    link: Option<(String, bool)>,
    media_parts: usize,
}

fn saved_picture(out: &[u8]) -> Saved {
    let p = unpack(out);
    let (dname, dx) = p
        .iter()
        .find(|(n, _)| n.starts_with("xl/drawings/") && n.ends_with(".xml") && !n.contains("_rels"))
        .map(|(n, x)| (n.clone(), String::from_utf8_lossy(x).into_owned()))
        .expect("no drawing part written");
    let file = dname.rsplit('/').next().unwrap();
    let rels = p.get(&format!("xl/drawings/_rels/{file}.rels")).map(|r| String::from_utf8_lossy(r).into_owned()).unwrap_or_default();
    assert_eq!(dx.matches("<xdr:pic>").count(), 1, "one picture expected in {dx}");
    let b = dx.find("<a:blip ").expect("no a:blip written");
    let blip = &dx[b..b + dx[b..].find('>').unwrap()];
    let rel_of = |rid: &str| -> (String, bool) {
        let key = format!("Id=\"{rid}\"");
        let ri = rels.find(&key).unwrap_or_else(|| panic!("no relationship {rid} in {rels}"));
        let start = rels[..ri].rfind('<').unwrap();
        let rel = &rels[start..start + rels[start..].find("/>").unwrap()];
        assert!(rel.contains("/relationships/image\""), "not an image relationship: {rel}");
        (attr(rel, "Target").unwrap(), rel.contains("TargetMode=\"External\""))
    };
    let embed = attr(blip, "r:embed").map(|rid| {
        let (t, ext) = rel_of(&rid);
        assert!(!ext, "embedded picture points outside the package");
        let path = format!("xl/{}", t.trim_start_matches("../"));
        p.get(&path).cloned().unwrap_or_else(|| panic!("missing media part {path}"))
    });
    let link = attr(blip, "r:link").map(|rid| rel_of(&rid));
    let media_parts = p.keys().filter(|n| n.starts_with("xl/media/")).count();
    Saved { embed, link, media_parts }
}

fn round_trip(pkg: Vec<u8>) -> (Vec<u8>, Vec<u8>, Vec<String>) {
    let (wb, report) = read_xlsx(&pkg).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (wb2, _) = read_xlsx(&out).unwrap();
    let out2 = write_xlsx(&wb2).unwrap();
    (out, out2, report.warnings.clone())
}

#[test]
fn link_only_picture_is_kept_with_its_external_target() {
    let (out, out2, warnings) = round_trip(package(r#"r:link="rId1""#, &[("rId1", URL, true)]));
    assert!(!warnings.iter().any(|w| w.contains("linked")), "linked picture still skipped: {warnings:?}");
    for (n, o) in [("first", &out), ("second", &out2)] {
        let s = saved_picture(o);
        assert_eq!(s.link, Some((URL.to_string(), true)), "{n} save");
        assert!(s.embed.is_none(), "{n} save: a link-only picture got an embedded copy");
        assert_eq!(s.media_parts, 0, "{n} save: a media part was written for a link-only picture");
    }
}

#[test]
fn link_only_picture_keeps_its_anchor_and_name() {
    let (wb, _) = read_xlsx(&package(r#"r:link="rId1""#, &[("rId1", URL, true)])).unwrap();
    let imgs = &wb.sheets[0].images;
    assert_eq!(imgs.len(), 1);
    assert_eq!(imgs[0].name.as_deref(), Some("Picture 6"));
    assert_eq!(imgs[0].alt, "blue square");
    assert!(imgs[0].data.is_empty());
    let (wb2, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(wb2.sheets[0].images[0].anchor, imgs[0].anchor);
}

#[test]
fn embedded_and_linked_picture_keeps_both() {
    let (out, out2, _) = round_trip(package(r#"r:embed="rId2" r:link="rId1""#, &[("rId1", URL, true), ("rId2", "../media/image1.png", false)]));
    for (n, o) in [("first", &out), ("second", &out2)] {
        let s = saved_picture(o);
        assert_eq!(s.link, Some((URL.to_string(), true)), "{n} save");
        assert_eq!(s.embed.as_deref(), Some(PNG), "{n} save: the cached copy changed");
    }
}

#[test]
fn embedded_picture_without_link_is_unchanged() {
    let (out, _, _) = round_trip(package(r#"r:embed="rId2""#, &[("rId2", "../media/image1.png", false)]));
    let s = saved_picture(&out);
    assert!(s.link.is_none());
    assert_eq!(s.embed.as_deref(), Some(PNG));
}
