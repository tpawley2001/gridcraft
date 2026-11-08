//! Signature lines (Insert > Signature Line): a legacy VML picture shape with `o:signatureline`, its
//! EMF image by `o:relid`, and nothing in DrawingML or the sheet. Excel keeps them on save, so must we.
//! Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// Bytes standing in for the EMF picture of the line (an EMR_HEADER type, then filler).
const EMF: &[u8] = b"\x01\x00\x00\x00\x6c\x00\x00\x00signature line picture";

/// The VML drawing: one picture shape (number 1025), a signature line when `signature`.
fn vml(signature: bool) -> String {
    let sig = if signature {
        r#"<o:signatureline v:ext="edit" id="{8EEC75DD-04B2-421E-BE83-1A51D3295ECB}" provid="{00000000-0000-0000-0000-000000000000}" o:suggestedsigner="me" o:suggestedsigner2="me" issignatureline="t"/>"#
    } else {
        ""
    };
    format!(
        r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel">
 <o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f"><v:stroke joinstyle="miter"/><v:path o:extrusionok="f" gradientshapeok="t" o:connecttype="rect"/><o:lock v:ext="edit" aspectratio="t"/></v:shapetype><v:shape id="_x0000_s1025" type="#_x0000_t75" alt="Microsoft Office Signature Line..." style='position:absolute;margin-left:96pt;margin-top:86.4pt;width:192pt;height:96pt;z-index:1'>
  <v:imagedata o:relid="rId1" o:title=""/>
  <o:lock v:ext="edit" ungrouping="t" rotation="t" cropping="t" verticies="t" text="t" grouping="t"/>
  {sig}
  <x:ClientData ObjectType="Pict">
   <x:SizeWithCells/>
   <x:Anchor>
    2, 0, 6, 0, 6, 0, 12, 16</x:Anchor>
   <x:CF>Pict</x:CF>
   <x:AutoPict/>
  </x:ClientData>
 </v:shape></xml>"##
    )
}

fn package(signature: bool) -> Vec<u8> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="emf" ContentType="image/x-emf"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><legacyDrawing r:id="rId1"/></worksheet>"#
    );
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#
    );
    let v_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/image" Target="../media/image1.emf"/></Relationships>"#
    );
    let vml = vml(signature);
    make_zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/vmlDrawing1.vml", vml.as_bytes()),
        ("xl/drawings/_rels/vmlDrawing1.vml.rels", v_rels.as_bytes()),
        ("xl/media/image1.emf", EMF),
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

/// The `v:shape` element holding `signatureline`, with its part name.
fn signature_shape(parts: &HashMap<String, Vec<u8>>) -> Option<(String, String)> {
    for (name, t) in vml_parts(parts) {
        if let Some(i) = t.find("signatureline ") {
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

/// `base_dir` joined with a relative `target` (`../media/x.emf`), as a package part name.
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
fn a_signature_line_survives_save() {
    let parts = unpack(&round_trip(&package(true)));
    let (_, shape) = signature_shape(&parts).unwrap_or_else(|| panic!("signature line lost: {:?}", vml_parts(&parts)));
    assert!(shape.contains("issignatureline=\"t\""), "{shape}");
    assert!(shape.contains("suggestedsigner=\"me\""), "{shape}");
    assert!(shape.contains("ObjectType=\"Pict\""), "{shape}");
    assert_eq!(text(&parts, "xl/worksheets/sheet1.xml").matches("<legacyDrawing ").count(), 1, "sheet must name the kept VML drawing once");
}

#[test]
fn the_signature_line_picture_relationship_resolves() {
    let parts = unpack(&round_trip(&package(true)));
    let (part, shape) = signature_shape(&parts).expect("signature line kept");
    let i = shape.find("<v:imagedata ").expect("imagedata kept");
    let rid = attr(&shape[i..], "o:relid").expect("o:relid kept");
    let (dir, file) = part.rsplit_once('/').unwrap();
    let rels = text(&parts, &format!("{dir}/_rels/{file}.rels"));
    let rel = rels.split("<Relationship ").find(|r| attr(&format!(" {r}"), "Id").as_deref() == Some(rid.as_str())).expect("relationship for o:relid");
    let rel = format!(" {rel}");
    assert!(attr(&rel, "Type").unwrap().ends_with("/image"), "{rel}");
    let target = resolve(dir, &attr(&rel, "Target").unwrap());
    assert_eq!(parts.get(&target).map(Vec::as_slice), Some(EMF), "picture part {target} missing or changed");
    let ext = target.rsplit('.').next().unwrap().to_ascii_lowercase();
    let ct = text(&parts, "[Content_Types].xml").to_ascii_lowercase();
    assert!(
        ct.contains(&format!("extension=\"{ext}\"")) || ct.contains(&format!("partname=\"/{}\"", target.to_ascii_lowercase())),
        "no content type for {target}: {ct}"
    );
}

#[test]
fn second_save_keeps_one_signature_line() {
    let parts = unpack(&round_trip(&round_trip(&package(true))));
    let n: usize = vml_parts(&parts).iter().map(|(_, t)| t.matches("issignatureline=").count()).sum();
    assert_eq!(n, 1, "{:?}", vml_parts(&parts));
}

#[test]
fn a_plain_pict_is_still_not_kept() {
    let parts = unpack(&round_trip(&package(false)));
    assert!(vml_parts(&parts).iter().all(|(_, t)| !t.contains("ObjectType=\"Pict\"")), "{:?}", vml_parts(&parts));
}
