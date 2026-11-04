//! Form controls kept only as shapes in the legacy VML drawing (no `<controls>`, no `ctrlProp`
//! parts), as Excel 2007 writes buttons: Excel keeps them on save, so must we. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/comments1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
const WORKBOOK: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const WORKBOOK_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;

fn vml_shape(id: u32, object_type: &str, extra: &str) -> String {
    format!(
        r#"<v:shape id="_x0000_s{id}" type="#_x0000_t201" style="position:absolute;margin-left:96pt;margin-top:15pt;width:48pt;height:15pt;z-index:1" o:button="t" fillcolor="buttonFace [67]"><v:textbox><div style="text-align:center"><font face="Calibri" size="220">Button 1</font></div></v:textbox><x:ClientData ObjectType="{object_type}"><x:Anchor>2, 0, 1, 0, 3, 0, 2, 0</x:Anchor><x:AutoFill>False</x:AutoFill>{extra}</x:ClientData></v:shape>"#
    )
}

fn vml(shapes: &str) -> String {
    format!(
        r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t201" coordsize="21600,21600" o:spt="201" path="m,l,21600r21600,l21600,xe"/>{shapes}</xml>"#
    )
}

/// One sheet whose `<legacyDrawing>` names `vml`; `comments` adds a comments part for A1.
fn package(vml: &str, comments: bool) -> Vec<u8> {
    let sheet = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row></sheetData><legacyDrawing r:id="rId1"/></worksheet>"#;
    let mut sheet_rels = String::from(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/>"#,
    );
    if comments {
        sheet_rels.push_str(r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/>"#);
    }
    sheet_rels.push_str("</Relationships>");
    let comments_xml = r#"<?xml version="1.0"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>a</author></authors><commentList><comment ref="A1" authorId="0"><text><t>note</t></text></comment></commentList></comments>"#;
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", WORKBOOK.as_bytes()),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/vmlDrawing1.vml", vml.as_bytes()),
    ];
    if comments {
        parts.push(("xl/comments1.xml", comments_xml.as_bytes()));
    }
    make_zip(&parts)
}

/// Every `.vml` part of a saved package, concatenated.
fn saved_vml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        if f.name().ends_with(".vml") {
            f.read_to_string(&mut out).unwrap();
        }
    }
    out
}

fn sheet_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn round_trip(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}

#[test]
fn a_vml_only_button_and_its_macro_survive_save() {
    let input = package(&vml(&vml_shape(1025, "Button", "<x:FmlaMacro>[0]!Button1_Click</x:FmlaMacro>")), false);
    let out = round_trip(&input);
    assert_eq!(sheet_xml(&out).matches("<legacyDrawing ").count(), 1, "sheet must name the kept VML drawing once");
    let v = saved_vml(&out);
    assert_eq!(v.matches("ObjectType=\"Button\"").count(), 1, "button lost or duplicated: {v}");
    assert_eq!(v.matches("[0]!Button1_Click").count(), 1, "assigned macro lost: {v}");
}

#[test]
fn a_vml_only_button_survives_a_second_save() {
    let input = package(&vml(&vml_shape(1025, "Button", "<x:FmlaMacro>[0]!Button1_Click</x:FmlaMacro>")), false);
    let out = round_trip(&round_trip(&input));
    assert_eq!(saved_vml(&out).matches("ObjectType=\"Button\"").count(), 1);
}

#[test]
fn a_vml_only_check_box_survives_save() {
    let input = package(&vml(&vml_shape(1025, "Checkbox", "<x:Checked>1</x:Checked>")), false);
    let v = saved_vml(&round_trip(&input));
    assert_eq!(v.matches("ObjectType=\"Checkbox\"").count(), 1, "check box lost: {v}");
}

#[test]
fn a_notes_only_vml_drawing_is_not_kept_twice() {
    let note = r##"<v:shape id="_x0000_s1025" type="#_x0000_t202" style="position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:1;visibility:hidden" fillcolor="#ffffe1"><v:textbox/><x:ClientData ObjectType="Note"><x:Anchor>1, 15, 0, 2, 3, 15, 4, 4</x:Anchor><x:Row>0</x:Row><x:Column>0</x:Column></x:ClientData></v:shape>"##;
    let out = round_trip(&package(&vml(note), true));
    assert_eq!(saved_vml(&out).matches("ObjectType=\"Note\"").count(), 1);
}
