//! Printed shapes follow their `a:xfrm` flips and rotation (ECMA-376 Part 1 §20.1.7.6): a line
//! with `flipV="1"` rises from bottom-left to top-right, and a rotated rectangle is its box turned
//! clockwise about the box centre.

use std::io::Write;

use gridcraft_engine::Session;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument";

fn rels(id: &str, kind: &str, target: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="{id}" Type="{NS_R}/{kind}" Target="{target}"/></Relationships>"#
    )
}

/// Every object anchored to the same box, C3:G9, so each drawn outline can be checked against it.
fn anchored(obj: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>6</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{obj}<xdr:clientData/></xdr:twoCellAnchor>"#
    )
}

fn line(id: u32, attrs: &str) -> String {
    anchored(&format!(
        r#"<xdr:cxnSp macro=""><xdr:nvCxnSpPr><xdr:cNvPr id="{id}" name="Line {id}"/><xdr:cNvCxnSpPr/></xdr:nvCxnSpPr><xdr:spPr><a:xfrm{attrs}><a:off x="0" y="0"/><a:ext cx="2438400" cy="1143000"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></xdr:spPr></xdr:cxnSp>"#
    ))
}

fn rect(id: u32, attrs: &str) -> String {
    anchored(&format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="Rectangle {id}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm{attrs}><a:off x="0" y="0"/><a:ext cx="2438400" cy="1143000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></xdr:spPr></xdr:sp>"#
    ))
}

fn package(objects: &[String]) -> Vec<u8> {
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{}</xdr:wsDr>"#,
        objects.concat()
    );
    let sheet = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData/><drawing r:id="rId1"/></worksheet>"#);
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/></Types>"#
            ),
        ),
        ("_rels/.rels", rels("rId1", "officeDocument", "xl/workbook.xml")),
        (
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
        ),
        ("xl/_rels/workbook.xml.rels", rels("rId1", "worksheet", "worksheets/sheet1.xml")),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/_rels/sheet1.xml.rels", rels("rId1", "drawing", "../drawings/drawing1.xml")),
        ("xl/drawings/drawing1.xml", drawing),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

/// The PDF's uncompressed content, as text.
fn pdf_text(objects: &[String]) -> String {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "flips.xlsx", "base64": gridcraft_engine::io::base64_encode(&package(objects))})).unwrap();
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let bytes = gridcraft_engine::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The points (`x y m`, `x y l`) of every painted path, in drawing order, with its paint operator.
fn paths(pdf: &str) -> Vec<(Vec<(f32, f32)>, String)> {
    let mut out = Vec::new();
    for l in pdf.lines() {
        let toks: Vec<&str> = l.split_whitespace().collect();
        let mut pts = Vec::new();
        for (i, t) in toks.iter().enumerate() {
            match *t {
                "m" | "l" if i >= 2 => {
                    if let (Ok(x), Ok(y)) = (toks[i - 2].parse::<f32>(), toks[i - 1].parse::<f32>()) {
                        pts.push((x, y));
                    }
                }
                "S" | "B" | "f" if !pts.is_empty() => out.push((std::mem::take(&mut pts), t.to_string())),
                _ => {}
            }
        }
    }
    out
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() <= 0.05 && (a.1 - b.1).abs() <= 0.05
}

/// The stroked two-point segments.
fn segments(pdf: &str) -> Vec<((f32, f32), (f32, f32))> {
    paths(pdf).into_iter().filter(|(p, op)| op == "S" && p.len() == 2).map(|(p, _)| (p[0], p[1])).collect()
}

#[test]
fn line_flips_pick_the_printed_diagonal() {
    let pdf = pdf_text(&[line(2, ""), line(3, r#" flipV="1""#), line(4, r#" flipH="1""#), line(5, r#" flipH="1" flipV="1""#)]);
    let segs = segments(&pdf);
    assert_eq!(segs.len(), 4, "{segs:?}\n{pdf}");
    // The plain line runs from the box's top-left to its bottom-right; PDF y grows upward.
    let ((left, top), (right, bottom)) = segs[0];
    assert!(right > left + 1.0 && top > bottom + 1.0, "{segs:?}");
    let want = [((left, top), (right, bottom)), ((left, bottom), (right, top)), ((right, top), (left, bottom)), ((right, bottom), (left, top))];
    for (i, (got, want)) in segs.iter().zip(want).enumerate() {
        assert!(close(got.0, want.0) && close(got.1, want.1), "line {i}: got {got:?}, want {want:?}");
    }
}

#[test]
fn rotated_rectangle_prints_its_box_turned_about_the_centre() {
    let pdf = pdf_text(&[line(2, ""), rect(3, r#" rot="5400000""#)]);
    let ((left, top), (right, bottom)) = segments(&pdf)[0];
    let polys: Vec<Vec<(f32, f32)>> = paths(&pdf).into_iter().filter(|(p, op)| op == "B" && p.len() == 4).map(|(p, _)| p).collect();
    assert_eq!(polys.len(), 1, "{polys:?}\n{pdf}");
    let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
    // The page's top-left, top-right, bottom-right, bottom-left corners, turned 90° clockwise on
    // the page: in PDF space (y up) the offset (dx, dy) becomes (dy, -dx).
    let corners = [(left, top), (right, top), (right, bottom), (left, bottom)];
    for (i, (got, (px, py))) in polys[0].iter().zip(corners).enumerate() {
        let (dx, dy) = (px - cx, py - cy);
        let want = (cx + dy, cy - dx);
        assert!(close(*got, want), "corner {i}: got {got:?}, want {want:?} (box {left},{top} – {right},{bottom})");
    }
    // Not the unturned box: the box is wider than tall, so the turned one is taller than wide.
    let xs: Vec<f32> = polys[0].iter().map(|p| p.0).collect();
    let width = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!((width - (top - bottom)).abs() <= 0.1, "turned width {width} should be the box height {}", top - bottom);
}
