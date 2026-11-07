//! Drawn shapes keep their DrawingML preset geometry (`a:prstGeom prst`, ECMA-376 Part 1 §20.1.9.18) and its
//! adjust-value guides (`a:avLst/a:gd`) through open and save, whatever the preset.

use std::io::Read;

use gridcraft_model::Workbook;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

fn anchor(row: u32, shape: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{shape}<xdr:clientData/></xdr:twoCellAnchor>"#,
        row + 3
    )
}

fn sp(id: u32, name: &str, geom: &str, text: &str) -> String {
    let tx = if text.is_empty() {
        String::new()
    } else {
        format!(r#"<xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>{text}</a:t></a:r></a:p></xdr:txBody>"#)
    };
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="{name}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm>{geom}<a:solidFill><a:srgbClr val="4472C4"/></a:solidFill></xdr:spPr>{tx}</xdr:sp>"#
    )
}

fn drawing(shapes: &[String]) -> String {
    let mut body = String::new();
    for (i, s) in shapes.iter().enumerate() {
        body.push_str(&anchor(i as u32 * 4, s));
    }
    format!(
        r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{body}</xdr:wsDr>"#
    )
}

fn read_drawing(drawing: &str) -> Workbook {
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;
    let bytes = minimal(
        r#"<sheetData/><drawing r:id="rId1"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/drawings/drawing1.xml", drawing)],
        "",
        "",
    );
    read_xlsx(&bytes).unwrap().0
}

fn written_drawing(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut drawing = String::new();
    archive.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut drawing).unwrap();
    drawing
}

/// The `prst` values of the written drawing's `a:prstGeom` elements, in document order.
fn presets(xml: &str) -> Vec<String> {
    xml.match_indices("<a:prstGeom prst=\"").map(|(i, m)| xml[i + m.len()..].split('"').next().unwrap().to_string()).collect()
}

/// The `a:avLst` body that follows the n-th `a:prstGeom` (empty for `<a:avLst/>`).
fn guides(xml: &str, n: usize) -> String {
    let (i, _) = xml.match_indices("<a:prstGeom ").nth(n).expect("prstGeom");
    let rest = &xml[i..xml[i..].find("</a:prstGeom>").map_or(xml.len(), |e| i + e)];
    match (rest.find("<a:avLst>"), rest.find("</a:avLst>")) {
        (Some(s), Some(e)) => rest[s + "<a:avLst>".len()..e].to_string(),
        _ => String::new(),
    }
}

const STAR: &str = r#"<a:prstGeom prst="star5"><a:avLst/></a:prstGeom>"#;
const DIAMOND: &str = r#"<a:prstGeom prst="diamond"><a:avLst/></a:prstGeom>"#;
const CALLOUT: &str = r#"<a:prstGeom prst="wedgeRectCallout"><a:avLst><a:gd name="adj1" fmla="val -36458"/><a:gd name="adj2" fmla="val 98333"/></a:avLst></a:prstGeom>"#;
const PROCESS: &str = r#"<a:prstGeom prst="flowChartProcess"><a:avLst/></a:prstGeom>"#;

#[test]
fn star_diamond_callout_and_flowchart_keep_their_presets_through_save() {
    let wb = read_drawing(&drawing(&[
        sp(2, "Star 1", STAR, ""),
        sp(3, "Diamond 2", DIAMOND, ""),
        sp(4, "Callout 3", CALLOUT, "Note"),
        sp(5, "Process 4", PROCESS, "Step one"),
    ]));
    assert_eq!(wb.sheet(0).unwrap().shapes.len(), 4);
    let xml = written_drawing(&wb);
    assert_eq!(presets(&xml), ["star5", "diamond", "wedgeRectCallout", "flowChartProcess"]);
    assert!(xml.contains("<a:t>Note</a:t>") && xml.contains("<a:t>Step one</a:t>"), "{xml}");
}

#[test]
fn callout_keeps_its_adjust_guides_in_order() {
    let wb = read_drawing(&drawing(&[sp(2, "Callout 1", CALLOUT, "Note")]));
    let xml = written_drawing(&wb);
    assert_eq!(presets(&xml), ["wedgeRectCallout"]);
    assert_eq!(guides(&xml, 0), r#"<a:gd name="adj1" fmla="val -36458"/><a:gd name="adj2" fmla="val 98333"/>"#);
}

#[test]
fn rounded_rectangle_keeps_its_corner_guide() {
    let geom = r#"<a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 8000"/></a:avLst></a:prstGeom>"#;
    let xml = written_drawing(&read_drawing(&drawing(&[sp(2, "Rounded Rectangle 1", geom, "")])));
    assert_eq!(presets(&xml), ["roundRect"]);
    assert_eq!(guides(&xml, 0), r#"<a:gd name="adj" fmla="val 8000"/>"#);
}

#[test]
fn presets_survive_a_second_open_and_save() {
    let first = written_drawing(&read_drawing(&drawing(&[sp(2, "Star 1", STAR, ""), sp(3, "Callout 2", CALLOUT, "x")])));
    let second = written_drawing(&read_drawing(&first));
    assert_eq!(presets(&second), ["star5", "wedgeRectCallout"]);
    assert_eq!(guides(&second, 1), guides(&first, 1));
}

#[test]
fn plain_rectangle_and_ellipse_write_as_before() {
    let rect = r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#;
    let ellipse = r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#;
    let xml = written_drawing(&read_drawing(&drawing(&[sp(2, "Rectangle 1", rect, ""), sp(3, "Oval 2", ellipse, "")])));
    assert_eq!(presets(&xml), ["rect", "ellipse"]);
    assert_eq!(guides(&xml, 0), "");
    assert_eq!(guides(&xml, 1), "");
}
