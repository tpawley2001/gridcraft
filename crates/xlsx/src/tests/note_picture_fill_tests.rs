//! A note whose box is filled with a picture (VML `<v:fill o:relid=…>`) keeps the picture through open and
//! save, as Excel's own re-save does.

use std::io::Read;

use crate::tests::{CT, ROOT_RELS, make_zip};
use crate::{read_xlsx, write_xlsx};

const WB: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const WB_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
const SHEET: &str = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row></sheetData><legacyDrawing r:id="rId2"/></worksheet>"#;
const SHEET_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#;
const COMMENTS: &str = r#"<?xml version="1.0"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>A</author></authors><commentList><comment ref="A1" authorId="0"><text><t>pic</t></text></comment><comment ref="B1" authorId="0"><text><t>plain</t></text></comment></commentList></comments>"#;
/// A1's note box is filled with a picture (the attributes Excel writes); B1's note is an ordinary one.
const VML: &str = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t202" coordsize="21600,21600" o:spt="202" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/><v:path gradientshapeok="t" o:connecttype="rect"/></v:shapetype><v:shape id="_x0000_s1025" type="#_x0000_t202" style="position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:84.75pt;height:84.75pt;z-index:1;visibility:hidden" o:insetmode="auto"><v:fill o:relid="rId7" o:title="test" color2="#ffffe1" recolor="t" type="frame"/><v:shadow on="t" color="black" obscured="t"/><v:path o:connecttype="none"/><v:textbox style="mso-direction-alt:auto"><div style="text-align:left"></div></v:textbox><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>1, 15, 0, 2, 3, 0, 5, 15</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>0</x:Row><x:Column>0</x:Column></x:ClientData></v:shape><v:shape id="_x0000_s1026" type="#_x0000_t202" style="position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:2;visibility:hidden" fillcolor="#ffffe1" o:insetmode="auto"><v:fill color2="#ffffe1"/><v:shadow on="t" color="black" obscured="t"/><v:path o:connecttype="none"/><v:textbox style="mso-direction-alt:auto"><div style="text-align:left"></div></v:textbox><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>2, 15, 0, 2, 4, 15, 4, 4</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>0</x:Row><x:Column>1</x:Column></x:ClientData></v:shape></xml>"##;
const VML_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.jpeg"/></Relationships>"#;
/// Stand-in picture bytes: the reader and writer pass them through, they are never decoded.
const PICTURE: &[u8] = b"\xff\xd8\xff\xe0 note fill picture bytes \xff\xd9";

fn package() -> Vec<u8> {
    let ct = CT.replace("</Types>", r#"<Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/></Types>"#);
    make_zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", WB.as_bytes()),
        ("xl/_rels/workbook.xml.rels", WB_RELS.as_bytes()),
        ("xl/worksheets/sheet1.xml", SHEET.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", SHEET_RELS.as_bytes()),
        ("xl/comments1.xml", COMMENTS.as_bytes()),
        ("xl/drawings/vmlDrawing1.vml", VML.as_bytes()),
        ("xl/drawings/_rels/vmlDrawing1.vml.rels", VML_RELS.as_bytes()),
        ("xl/media/image1.jpeg", PICTURE),
    ])
}

fn part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).ok()?;
    let mut v = Vec::new();
    f.read_to_end(&mut v).unwrap();
    Some(v)
}

fn names(bytes: &[u8]) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect()
}

fn text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(part(bytes, name).unwrap_or_else(|| panic!("no part {name}"))).unwrap()
}

/// The one written VML part's name.
fn vml_name(bytes: &[u8]) -> String {
    names(bytes).into_iter().find(|n| n.ends_with(".vml")).expect("a VML part")
}

/// Attribute `name` of the first `<tag …>` element in `xml` that contains `needle`.
fn attr<'a>(xml: &'a str, tag: &str, needle: &str, name: &str) -> Option<&'a str> {
    let el = xml.split(&format!("<{tag} ")).skip(1).map(|s| &s[..s.find('>').unwrap()]).find(|e| e.contains(needle))?;
    let start =
        el.find(&format!(" {name}=\"")).map(|i| i + name.len() + 3).or_else(|| el.starts_with(&format!("{name}=\"")).then_some(name.len() + 2))?;
    Some(&el[start..start + el[start..].find('"').unwrap()])
}

/// The picture behind the written note fill: (fill element, media part name, media bytes).
fn written_fill(bytes: &[u8]) -> (String, String, Vec<u8>) {
    let vml_name = vml_name(bytes);
    let vml = text(bytes, &vml_name);
    let fill = vml
        .split("<v:fill ")
        .skip(1)
        .map(|s| s[..s.find('>').unwrap()].to_string())
        .find(|e| e.contains("o:relid="))
        .unwrap_or_else(|| panic!("no picture fill in {vml}"));
    let rid = attr(&format!("<v:fill {fill}>"), "v:fill", "o:relid", "o:relid").unwrap().to_string();
    let (dir, file) = vml_name.rsplit_once('/').unwrap();
    let rels = text(bytes, &format!("{dir}/_rels/{file}.rels"));
    let target = attr(&rels, "Relationship", &format!("Id=\"{rid}\""), "Target").unwrap_or_else(|| panic!("no {rid} in {rels}"));
    assert_eq!(
        attr(&rels, "Relationship", &format!("Id=\"{rid}\""), "Type"),
        Some("http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"),
        "{rels}"
    );
    let media = format!("xl/{}", target.trim_start_matches("../"));
    let data = part(bytes, &media).unwrap_or_else(|| panic!("no media part {media}"));
    (fill, media, data)
}

#[test]
fn a_picture_filled_note_keeps_its_picture_and_fill_settings() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (fill, _, data) = written_fill(&out);
    assert_eq!(data, PICTURE);
    for kept in ["o:title=\"test\"", "type=\"frame\"", "recolor=\"t\"", "color2=\"#ffffe1\""] {
        assert!(fill.contains(kept), "{kept} missing from <v:fill {fill}>");
    }
}

#[test]
fn the_picture_has_a_content_type() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (_, media, _) = written_fill(&out);
    let ext = media.rsplit_once('.').unwrap().1;
    let ct = text(&out, "[Content_Types].xml");
    let has_default = ct.contains(&format!("Extension=\"{ext}\""));
    let has_override = ct.contains(&format!("PartName=\"/{media}\""));
    assert!(has_default || has_override, "no content type for {media} in {ct}");
}

#[test]
fn the_plain_note_keeps_the_default_fill() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let vml = text(&out, &vml_name(&out));
    let fills: Vec<&str> = vml.split("<v:fill ").skip(1).map(|s| &s[..s.find('>').unwrap()]).collect();
    assert_eq!(fills.len(), 2, "{vml}");
    assert_eq!(fills.iter().filter(|f| f.contains("o:relid=")).count(), 1, "only A1's note has a picture: {vml}");
}

#[test]
fn the_picture_survives_a_second_round_trip() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let (wb2, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let out = write_xlsx(&wb2).unwrap();
    let (fill, _, data) = written_fill(&out);
    assert_eq!(data, PICTURE);
    assert!(fill.contains("type=\"frame\""), "<v:fill {fill}>");
}
