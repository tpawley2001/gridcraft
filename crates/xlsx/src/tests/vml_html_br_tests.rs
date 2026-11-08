//! Legacy VML drawings whose text boxes use HTML-style `<br>` line breaks with no closing tag (Excel
//! 2007-era captions): the drawing is still read, so a VML-only button and a shown note survive save.
//! Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};
use gridcraft_core::CellRef;
use gridcraft_model::{ControlAction, ControlKind};

const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/comments1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
const WORKBOOK: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const WORKBOOK_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;

/// A VML-only button whose caption markup is `caption` (inside `<font>`), as Excel 2007 writes it.
fn button(caption: &str) -> String {
    format!(
        "<v:shape id=\"_x0000_s1025\" type=\"#_x0000_t201\" style='position:absolute;margin-left:106.5pt;margin-top:49.5pt;width:87.75pt;height:70.5pt;z-index:1;mso-wrap-style:tight' o:button=\"t\" fillcolor=\"buttonFace [67]\" strokecolor=\"windowText [64]\" o:insetmode=\"auto\">\n <v:fill color2=\"buttonFace [67]\" o:detectmouseclick=\"t\"/>\n <o:lock v:ext=\"edit\" rotation=\"t\"/>\n <v:textbox style='mso-direction-alt:auto' o:singleclick=\"f\">\n  <div style='text-align:center'><font face=\"Calibri\" size=\"220\" color=\"#000000\">{caption}</font></div>\n </v:textbox>\n <x:ClientData ObjectType=\"Button\">\n  <x:Anchor>\n   2, 14, 3, 6, 4, 3, 8, 0</x:Anchor>\n  <x:PrintObject>False</x:PrintObject>\n  <x:AutoFill>False</x:AutoFill>\n  <x:FmlaMacro>[0]!Button1_Click</x:FmlaMacro>\n  <x:TextHAlign>Center</x:TextHAlign>\n  <x:TextVAlign>Center</x:TextVAlign>\n </x:ClientData>\n</v:shape>"
    )
}

fn vml(shapes: &str) -> String {
    format!(
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\"\n xmlns:o=\"urn:schemas-microsoft-com:office:office\"\n xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\n <o:shapelayout v:ext=\"edit\">\n  <o:idmap v:ext=\"edit\" data=\"1\"/>\n </o:shapelayout><v:shapetype id=\"_x0000_t201\" coordsize=\"21600,21600\" o:spt=\"201\" path=\"m,l,21600r21600,l21600,xe\">\n  <v:stroke joinstyle=\"miter\"/>\n </v:shapetype>{shapes}</xml>"
    )
}

/// One sheet whose `<legacyDrawing>` names `vml`; `comments` adds a comments part with a note on A1.
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

fn round_trip(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}

/// The caption's words, whitespace and line breaks dropped.
fn words(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

const MULTI_LINE: &str = "<b>Multi<br>\n      Line<br>\n      Text</b>";

#[test]
fn a_button_with_unclosed_br_in_its_caption_is_read() {
    let (wb, _) = read_xlsx(&package(&vml(&button(MULTI_LINE)), false)).unwrap();
    let controls = &wb.sheets[0].controls;
    assert_eq!(controls.len(), 1, "VML-only button not read");
    let c = &controls[0];
    assert_eq!(c.kind, ControlKind::Button);
    assert_eq!(words(&c.text), ["Multi", "Line", "Text"], "caption: {:?}", c.text);
    match &c.action {
        Some(ControlAction::Macro { name, .. }) => assert_eq!(name, "Button1_Click"),
        other => panic!("assigned macro lost: {other:?}"),
    }
}

#[test]
fn a_button_with_unclosed_br_survives_save_as_well_formed_vml() {
    let out = round_trip(&package(&vml(&button(MULTI_LINE)), false));
    let v = saved_vml(&out);
    assert_eq!(v.matches("ObjectType=\"Button\"").count(), 1, "button lost or duplicated: {v}");
    assert_eq!(v.matches("[0]!Button1_Click").count(), 1, "assigned macro lost: {v}");
    assert!(!v.contains("<br>"), "saved VML keeps an unclosed <br>: {v}");
    let again = saved_vml(&round_trip(&out));
    assert_eq!(again.matches("ObjectType=\"Button\"").count(), 1, "second save: {again}");
}

#[test]
fn upper_and_mixed_case_br_with_spaces_are_line_breaks_too() {
    let (wb, _) = read_xlsx(&package(&vml(&button("A<BR>B<Br >C")), false)).unwrap();
    let controls = &wb.sheets[0].controls;
    assert_eq!(controls.len(), 1, "VML-only button not read");
    assert_eq!(controls[0].kind, ControlKind::Button);
    assert_eq!(words(&controls[0].text), ["A", "B", "C"], "caption: {:?}", controls[0].text);
}

#[test]
fn a_shown_note_whose_caption_has_unclosed_br_stays_shown() {
    let note = "<v:shape id=\"_x0000_s1026\" type=\"#_x0000_t202\" style='position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:2' fillcolor=\"#ffffe1\"><v:textbox><div style='text-align:left'><font face=\"Tahoma\" size=\"160\">x<br>y</font></div></v:textbox><x:ClientData ObjectType=\"Note\"><x:Anchor>1, 15, 0, 2, 3, 15, 4, 4</x:Anchor><x:Row>0</x:Row><x:Column>0</x:Column><x:Visible/></x:ClientData></v:shape>";
    let (wb, _) = read_xlsx(&package(&vml(note), true)).unwrap();
    assert!(wb.sheets[0].comments[&CellRef::new(0, 0)].visible, "shown note read as hidden");
}

#[test]
fn a_self_closed_br_keeps_its_place_in_the_caption() {
    // Today the text after `<br/>` is joined to the text before it ("MultiLine").
    let (wb, _) = read_xlsx(&package(&vml(&button("Multi<br/>Line")), false)).unwrap();
    let controls = &wb.sheets[0].controls;
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0].text, "Multi\nLine");
}

#[test]
fn unclosed_br_captions_keep_line_order() {
    let (wb, _) = read_xlsx(&package(&vml(&button("A<BR>B<Br >C")), false)).unwrap();
    assert_eq!(wb.sheets[0].controls[0].text, "A\nB\nC");
}

#[test]
fn parse_vml_turns_br_into_a_line_break_in_its_parent() {
    let root = crate::xml::parse_vml(b"<a><b>1<br>2<BR/>3</br></b><c/></a>").unwrap();
    assert_eq!(root.name, "a");
    let names: Vec<&str> = root.children.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, ["b", "c"]);
    let b = &root.children[0];
    assert!(b.children.is_empty(), "br kept as an element: {:?}", b.children);
    assert_eq!(b.text, "1\n2\n3");
}

#[test]
fn parse_is_unchanged_for_other_parts() {
    // Guard: the general parser keeps `<br/>` as an element (OMML and others rely on it).
    let root = crate::xml::parse(b"<a><b>1<br/>2</b></a>").unwrap();
    assert_eq!(root.children[0].children.len(), 1);
    assert_eq!(root.children[0].children[0].name, "br");
}
