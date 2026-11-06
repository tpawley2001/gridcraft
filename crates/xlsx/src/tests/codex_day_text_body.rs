//! Synthetic XLSX acceptance tests; no vendor-produced fixtures.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> El {
    parse(&Package::open(bytes).unwrap().read(name).unwrap().unwrap()).unwrap()
}
fn save(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}
fn twice(bytes: &[u8], name: &str) -> El {
    part(&save(&save(bytes)), name)
}

fn drawing(attrs: &str) -> Vec<u8> {
    let d = format!(
        r#"<x:wsDr xmlns:x="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><x:twoCellAnchor><x:from><x:col>0</x:col><x:colOff>0</x:colOff><x:row>0</x:row><x:rowOff>0</x:rowOff></x:from><x:to><x:col>3</x:col><x:colOff>0</x:colOff><x:row>3</x:row><x:rowOff>0</x:rowOff></x:to><x:sp><x:nvSpPr><x:cNvPr id="2" name="Example"/><x:cNvSpPr txBox="1"/></x:nvSpPr><x:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></x:spPr><x:txBody><a:bodyPr {attrs}/><a:lstStyle/><a:p><a:r><a:t>Example text</a:t></a:r></a:p></x:txBody></x:sp><x:clientData/></x:twoCellAnchor></x:wsDr>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="drawing" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;
    minimal(r#"<sheetData/><drawing r:id="drawing"/>"#, &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/drawings/drawing1.xml", &d)], "", "")
}
fn body(attrs: &str) -> El {
    let x = twice(&drawing(attrs), "xl/drawings/drawing1.xml");
    x.path(&["twoCellAnchor", "sp", "txBody", "bodyPr"]).unwrap().clone()
}
#[test]
fn custom_insets_are_retained_without_unit_conversion() {
    let b = body(r#"lIns="90000" rIns="137160" tIns="0" bIns="-45720" wrap="none""#);
    for (k, v) in [("lIns", 90000), ("rIns", 137160), ("tIns", 0), ("bIns", -45720)] {
        assert_eq!(b.attr_i64(k), Some(v), "{k}");
    }
    assert_eq!(b.attr("wrap"), Some("none"));
}
#[test]
fn vertical_modes_and_body_rotation_are_retained() {
    for v in ["horz", "vert", "vert270", "wordArtVert", "eaVert", "mongolianVert", "wordArtVertRtl"] {
        let b = body(&format!(r#"vert="{v}" rot="-5400000""#));
        assert_eq!(b.attr("vert"), Some(v));
        assert_eq!(b.attr_i64("rot"), Some(-5400000));
    }
}
#[test]
fn malformed_body_properties_are_not_written() {
    let b = body(r#"lIns="2147483648" rIns="NaN" tIns="1.5" bIns="-2147483649" wrap="bad" vert="bad" rot="9999999999""#);
    for k in ["lIns", "rIns", "tIns", "bIns", "wrap", "vert", "rot"] {
        assert!(b.attr(k).is_none(), "{k}");
    }
}
