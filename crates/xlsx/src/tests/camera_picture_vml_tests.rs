//! Camera tool pictures (linked pictures of a cell range): the DrawingML picture with an
//! `a14:cameraTool spid` extension and its legacy VML shape (`ObjectType="Pict"` with
//! `x:FmlaPict`, `x:Camera`, an image by `o:relid`) both survive open + save, as in Excel.
//! Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
const RANGE: &str = "$B$2:$D$4";

/// The VML half of a camera picture (shape number 1025), or a plain `Pict` when `camera` is false.
fn vml(camera: bool) -> String {
    let camera_data = if camera { format!("<x:FmlaPict>{RANGE}</x:FmlaPict><x:CF>Pict</x:CF><x:Camera/>") } else { String::new() };
    format!(
        r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f"><v:path o:extrusionok="f" gradientshapeok="t" o:connecttype="rect"/></v:shapetype><v:shape id="Picture_x0020_1" o:spid="_x0000_s1025" type="#_x0000_t75" style="position:absolute;margin-left:96pt;margin-top:15pt;width:144pt;height:45pt;z-index:1;visibility:visible" filled="t" fillcolor="white [9]" stroked="t" strokecolor="windowText [64]"><v:imagedata o:relid="rId1" o:title=""/><x:ClientData ObjectType="Pict"><x:SizeWithCells/><x:Anchor>4, 0, 6, 0, 7, 0, 9, 0</x:Anchor>{camera_data}</x:ClientData></v:shape></xml>"##
    )
}

/// One sheet with the DrawingML picture (`cameraTool` when `camera`) and the VML drawing.
fn package(camera: bool) -> Vec<u8> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>"#;
    let root = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="2"><c r="B2"><v>1</v></c><c r="C2"><v>2</v></c></row></sheetData><drawing r:id="rId1"/><legacyDrawing r:id="rId2"/></worksheet>"#
    );
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/drawing" Target="../drawings/drawing1.xml"/><Relationship Id="rId2" Type="{NS_R}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#
    );
    let ext = if camera {
        format!(
            r#"<a:extLst><a:ext uri="{{84589F7E-364E-4C9E-8A38-B11213B215E9}}"><a14:cameraTool cellRange="{RANGE}" spid="_x0000_s1025"/></a:ext></a:extLst>"#
        )
    } else {
        String::new()
    };
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" xmlns:r="{NS_R}"><xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>6</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>9</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/><xdr:cNvPicPr><a:picLocks noChangeAspect="1" noChangeArrowheads="1"/>{ext}</xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:srcRect/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let d_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/image" Target="../media/image1.png"/></Relationships>"#
    );
    let v_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/image" Target="../media/image2.png"/></Relationships>"#
    );
    let vml = vml(camera);
    make_zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", d_rels.as_bytes()),
        ("xl/drawings/vmlDrawing1.vml", vml.as_bytes()),
        ("xl/drawings/_rels/vmlDrawing1.vml.rels", v_rels.as_bytes()),
        ("xl/media/image1.png", PNG),
        ("xl/media/image2.png", PNG),
    ])
}

fn unpack(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = HashMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut v = Vec::new();
        f.read_to_end(&mut v).unwrap();
        out.insert(f.name().to_string(), v);
    }
    out
}

fn text(parts: &HashMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8_lossy(&parts[name]).into_owned()
}

fn round_trip(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}

/// The saved VML parts (name, text).
fn vml_parts(parts: &HashMap<String, Vec<u8>>) -> Vec<(String, String)> {
    let mut v: Vec<_> = parts.keys().filter(|k| k.ends_with(".vml")).map(|k| (k.clone(), text(parts, k))).collect();
    v.sort();
    v
}

/// The `v:shape` element (start tag through `</v:shape>`) holding `x:FmlaPict`, with its part name.
fn camera_shape(parts: &HashMap<String, Vec<u8>>) -> Option<(String, String)> {
    for (name, t) in vml_parts(parts) {
        if let Some(i) = t.find("FmlaPict>") {
            let start = t[..i].rfind("<v:shape ")?;
            let end = t[i..].find("</v:shape>")? + i + "</v:shape>".len();
            return Some((name, t[start..end].to_string()));
        }
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let i = tag.find(&key)? + key.len();
    Some(tag[i..i + tag[i..].find('"')?].to_string())
}

/// `base_dir` joined with a relative `target` (`../media/x.png`), as a package part name.
fn resolve(base_dir: &str, target: &str) -> String {
    let mut segs: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    for s in target.split('/') {
        match s {
            ".." => {
                segs.pop();
            }
            "." | "" => {}
            s => segs.push(s),
        }
    }
    segs.join("/")
}

#[test]
fn camera_vml_shape_survives_save() {
    let parts = unpack(&round_trip(&package(true)));
    let (_, shape) = camera_shape(&parts).expect("the camera picture's VML shape is kept");
    assert!(shape.contains(&format!("FmlaPict>{RANGE}</")), "{shape}");
    assert!(shape.contains("Camera"), "{shape}");
    assert!(shape.contains("ObjectType=\"Pict\""), "{shape}");
}

#[test]
fn camera_vml_image_relationship_resolves() {
    let parts = unpack(&round_trip(&package(true)));
    let (name, shape) = camera_shape(&parts).expect("camera shape kept");
    let i = shape.find("<v:imagedata").expect("imagedata kept");
    let rid = attr(&shape[i..], "o:relid").expect("o:relid kept");
    let (dir, file) = name.rsplit_once('/').unwrap();
    let rels_name = format!("{dir}/_rels/{file}.rels");
    let rels = text(&parts, &rels_name);
    let rel = rels.split("<Relationship ").find(|r| attr(&format!(" {r}"), "Id").as_deref() == Some(rid.as_str())).expect("relationship for o:relid");
    let rel = format!(" {rel}");
    assert!(attr(&rel, "Type").unwrap().ends_with("/image"), "{rel}");
    let target = resolve(dir, &attr(&rel, "Target").unwrap());
    assert!(parts.contains_key(&target), "image part {target} missing; parts {:?}", parts.keys().collect::<Vec<_>>());
}

#[test]
fn drawingml_camera_spid_names_the_kept_vml_shape() {
    let parts = unpack(&round_trip(&package(true)));
    let drawing = parts.keys().filter(|k| k.starts_with("xl/drawings/drawing") && k.ends_with(".xml")).map(|k| text(&parts, k)).collect::<String>();
    let i = drawing.find("cameraTool").expect("DrawingML cameraTool kept");
    let spid = attr(&drawing[i..], "spid").expect("spid kept");
    let (_, shape) = camera_shape(&parts).expect("camera shape kept");
    let tag = &shape[..shape.find('>').unwrap()];
    let ids = [attr(tag, "o:spid"), attr(&format!(" {}", &tag[1..]), "id")];
    assert!(ids.iter().flatten().any(|id| *id == spid), "spid {spid} vs shape {tag}");
}

#[test]
fn second_save_keeps_one_camera_shape() {
    let parts = unpack(&round_trip(&round_trip(&package(true))));
    let n: usize = vml_parts(&parts).iter().map(|(_, t)| t.matches("FmlaPict>").count()).sum();
    // Open and close tag of one element.
    assert_eq!(n, 2, "{:?}", vml_parts(&parts));
}

#[test]
fn a_plain_pict_without_a_formula_is_still_not_kept() {
    let parts = unpack(&round_trip(&package(false)));
    assert!(vml_parts(&parts).iter().all(|(_, t)| !t.contains("ObjectType=\"Pict\"")), "{:?}", vml_parts(&parts));
}
