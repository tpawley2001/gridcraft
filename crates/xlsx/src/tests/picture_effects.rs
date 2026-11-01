//! Picture effects stored as a blip extension (`a14:imgProps` with an HD Photo `a14:imgLayer`, and
//! `a14:useLocalDpi`) survive open + save with the parts they name. Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_HDPHOTO: &str = "http://schemas.microsoft.com/office/2007/relationships/hdphoto";
const CT_WDP: &str = "image/vnd.ms-photo";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
const WDP: &[u8] = b"II\xbc\x01\x08\x00\x00\x00 not really an HD Photo";
const EFFECTS: &str = r#"<a:ext uri="{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}"><a14:imgProps xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main"><a14:imgLayer r:embed="rId2"><a14:imgEffect><a14:backgroundRemoval t="10000" b="90000" l="10000" r="90000"/></a14:imgEffect><a14:imgEffect><a14:artisticFilmGrain/></a14:imgEffect></a14:imgLayer></a14:imgProps></a:ext>"#;
const LOCAL_DPI: &str = r#"<a:ext uri="{28A0092B-C50C-407E-A947-70E740481C1C}"><a14:useLocalDpi xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" val="0"/></a:ext>"#;

/// One sheet whose drawing holds one picture of image1.png with `ext_list` inside its `a:blip`;
/// `hdphoto` adds the rId2 HD Photo relationship and part.
fn package(ext_list: &str, hdphoto: bool) -> Vec<u8> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="wdp" ContentType="image/vnd.ms-photo"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let wb = format!(
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
    let ext = if ext_list.is_empty() { String::new() } else { format!("<a:extLst>{ext_list}</a:extLst>") };
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}"><xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>6</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1">{ext}</a:blip><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let hd_rel = if hdphoto { format!(r#"<Relationship Id="rId2" Type="{REL_HDPHOTO}" Target="../media/hdphoto1.wdp"/>"#) } else { String::new() };
    let d_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/image" Target="../media/image1.png"/>{hd_rel}</Relationships>"#
    );
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", d_rels.as_bytes()),
        ("xl/media/image1.png", PNG),
    ];
    if hdphoto {
        parts.push(("xl/media/hdphoto1.wdp", WDP));
    }
    make_zip(&parts)
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

fn text(p: &HashMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8_lossy(p.get(name).unwrap_or_else(|| panic!("{name} missing"))).into_owned()
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    Some(s[i..i + s[i..].find(b)?].to_string())
}

/// The saved drawing part of sheet "Data" (the only drawing) and its relationships XML.
fn drawing(p: &HashMap<String, Vec<u8>>) -> (String, String) {
    let name =
        p.keys().find(|n| n.starts_with("xl/drawings/") && n.ends_with(".xml") && !n.contains("_rels")).expect("no drawing part written").clone();
    let file = name.rsplit('/').next().unwrap().to_string();
    let rels = p.get(&format!("xl/drawings/_rels/{file}.rels")).map(|v| String::from_utf8_lossy(v).into_owned()).unwrap_or_default();
    (text(p, &name), rels)
}

/// (type, part name) of drawing relationship `id`, the target resolved against `xl/drawings/`.
fn rel(rels: &str, id: &str) -> (String, String) {
    let at = rels.find(&format!("Id=\"{id}\"")).unwrap_or_else(|| panic!("no relationship {id} in {rels}"));
    let start = rels[..at].rfind("<Relationship").unwrap();
    let el = &rels[start..start + rels[start..].find("/>").unwrap()];
    let ty = between(el, "Type=\"", "\"").unwrap();
    let target = between(el, "Target=\"", "\"").unwrap();
    let mut path: Vec<&str> = if target.starts_with('/') { vec![] } else { vec!["xl", "drawings"] };
    for seg in target.trim_start_matches('/').split('/') {
        match seg {
            ".." => {
                path.pop();
            }
            "." | "" => {}
            s => path.push(s),
        }
    }
    (ty, path.join("/"))
}

/// The content type the saved package gives `part` (override first, then default by extension).
fn content_type(p: &HashMap<String, Vec<u8>>, part: &str) -> Option<String> {
    let ct = text(p, "[Content_Types].xml");
    if let Some(t) = between(&ct, &format!("PartName=\"/{part}\" ContentType=\""), "\"") {
        return Some(t);
    }
    let ext = part.rsplit('.').next()?;
    between(&ct, &format!("Extension=\"{ext}\" ContentType=\""), "\"")
}

#[test]
fn picture_effects_and_hd_photo_layer_survive_a_round_trip() {
    let (wb, _) = read_xlsx(&package(&format!("{EFFECTS}{LOCAL_DPI}"), true)).unwrap();
    let p = unpack(&write_xlsx(&wb).unwrap());
    let (dx, rels) = drawing(&p);
    assert_eq!(dx.matches("<xdr:pic>").count(), 1, "{dx}");
    assert!(dx.contains(r#"<a14:backgroundRemoval t="10000" b="90000" l="10000" r="90000"/>"#), "{dx}");
    assert!(dx.contains("<a14:artisticFilmGrain/>"), "{dx}");
    // The picture itself still resolves to its image.
    let blip = between(&dx, "<a:blip ", ">").expect("no a:blip");
    let img = between(&blip, "r:embed=\"", "\"").unwrap();
    assert_eq!(p.get(&rel(&rels, &img).1).map(Vec::as_slice), Some(PNG));
    // The effect layer resolves to the HD Photo part, under its relationship type and content type.
    let layer = between(&dx, "<a14:imgLayer r:embed=\"", "\"").expect("imgLayer not written");
    let (ty, part) = rel(&rels, &layer);
    assert_eq!(ty, REL_HDPHOTO);
    assert_eq!(p.get(&part).map(Vec::as_slice), Some(WDP), "HD Photo bytes kept at {part}");
    assert_eq!(content_type(&p, &part).as_deref(), Some(CT_WDP));
    // The extension list stays inside the blip, effects first, then the local-dpi flag.
    let blip_xml = between(&dx, "<a:blip ", "</a:blip>").expect("a:blip has no content");
    let fx = blip_xml.find("{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}").expect("effects ext outside a:blip");
    let dpi = blip_xml.find("{28A0092B-C50C-407E-A947-70E740481C1C}").expect("useLocalDpi ext outside a:blip");
    assert!(fx < dpi);
    assert!(blip_xml.contains(r#"val="0""#));
}

#[test]
fn local_dpi_extension_alone_survives() {
    let (wb, _) = read_xlsx(&package(LOCAL_DPI, false)).unwrap();
    let p = unpack(&write_xlsx(&wb).unwrap());
    let (dx, _) = drawing(&p);
    let blip_xml = between(&dx, "<a:blip ", "</a:blip>").expect("a:blip has no content");
    assert!(blip_xml.contains("{28A0092B-C50C-407E-A947-70E740481C1C}"), "{dx}");
}

#[test]
fn plain_picture_writes_no_blip_extensions() {
    let (wb, _) = read_xlsx(&package("", false)).unwrap();
    let p = unpack(&write_xlsx(&wb).unwrap());
    let (dx, _) = drawing(&p);
    assert_eq!(dx.matches("<xdr:pic>").count(), 1);
    assert!(!dx.contains("a:extLst") || !between(&dx, "<a:blip ", "</xdr:blipFill>").unwrap_or_default().contains("a:extLst"));
    assert!(!p.keys().any(|n| n.ends_with(".wdp")));
}

#[test]
fn effect_layer_with_a_missing_part_keeps_the_picture_without_the_effect() {
    // rId2 is not in the drawing's relationships: the extension is dropped, the picture is not.
    let (wb, _) = read_xlsx(&package(&format!("{EFFECTS}{LOCAL_DPI}"), false)).unwrap();
    let p = unpack(&write_xlsx(&wb).unwrap());
    let (dx, rels) = drawing(&p);
    assert_eq!(dx.matches("<xdr:pic>").count(), 1, "{dx}");
    assert!(!dx.contains("imgLayer"), "a layer naming no part must not be written: {dx}");
    let blip = between(&dx, "<a:blip ", ">").expect("no a:blip");
    let img = between(&blip, "r:embed=\"", "\"").unwrap();
    assert_eq!(p.get(&rel(&rels, &img).1).map(Vec::as_slice), Some(PNG));
    assert!(!p.keys().any(|n| n.ends_with(".wdp")));
}
