//! Freeform shapes: DrawingML custom geometry (`a:custGeom`, ECMA-376 Part 1 §20.1.9.8, paths §20.1.9.15)
//! kept through open and save, and Excel 5.0/95 (BIFF5) polygons (an OBJ record with `ot` 9 followed by a
//! COORDLIST record) opened as freeforms. Every workbook is assembled here; no Excel-produced file is used.

use std::io::Read;

use gridcraft_model::Workbook;

use super::minimal;
use super::xls_tests::compound;
use crate::{read_xls, read_xlsx, write_xlsx};

const SHAPES_WARNING: &str = "Shapes in .xls files are not imported yet";

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(vers: u16, dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[vers, dt, 0x0DBB, 0x07CC]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

/// A Book stream: globals (CodePage 1252, one BoundSheet) and one worksheet with `records`.
fn book(records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 5], b"Sheet"].concat());
    let head = [bof(0x0500, 0x0005), rec(0x0042, &1252u16.to_le_bytes())].concat();
    let pos = (head.len() + boundsheet(0).len() + eof().len()) as u32;
    let globals = [head, boundsheet(pos), eof()].concat();
    let body = [bof(0x0500, 0x0010), records.to_vec(), eof()].concat();
    compound("Book", &[globals, body].concat())
}

/// A 66-byte BIFF5 polygon OBJ as Excel 95 wrote it: the 34-byte header (cObj, ot 9, id, grbit, the anchor
/// colL, dxL, rwT, dyT, colR, dxR, rwB, dyB, cbMacro 0, 6 reserved bytes), the 8 fill/line bytes (icvBack,
/// icvFore, fls, fAutoFill, icv, lns, lnw, fAutoB), frs, wPolyFlags, 10 reserved bytes, cpt and 8 more bytes.
fn polygon(id: u16, anchor: [u16; 8], fill_line: [u8; 8], flags: u16, cpt: u16) -> Vec<u8> {
    let mut p = [&3u32.to_le_bytes()[..], &u16s(&[9, id, 0x0616]), &u16s(&anchor), &[0; 8]].concat();
    p.extend_from_slice(&fill_line);
    p.extend_from_slice(&u16s(&[0, flags]));
    p.extend_from_slice(&[0; 10]);
    p.extend_from_slice(&cpt.to_le_bytes());
    p.extend_from_slice(&[0x8C, 0xC3, 0x1D, 0x01, 0xB0, 0xC3, 0x1D, 0x01]);
    assert_eq!(p.len(), 66);
    rec(0x005D, &p)
}

/// COORDLIST: (x, y) pairs in 1/16384 of the object's box.
fn coordlist(points: &[(u16, u16)]) -> Vec<u8> {
    rec(0x00A9, &points.iter().flat_map(|&(x, y)| u16s(&[x, y])).collect::<Vec<_>>())
}

/// The down arrow measured on a real Excel 95 workbook.
const ARROW: [(u16, u16); 7] = [(0, 0x3000), (0x1000, 0x3000), (0x1000, 0), (0x3000, 0), (0x3000, 0x3000), (0x4000, 0x3000), (0x2000, 0x4000)];
const ARROW_FRACTIONS: [(f64, f64); 7] = [(0.0, 0.75), (0.25, 0.75), (0.25, 0.0), (0.75, 0.0), (0.75, 0.75), (1.0, 0.75), (0.5, 1.0)];
/// White solid fill (icv 9), automatic hairline outline: the real fields.
const WHITE_AUTO: [u8; 8] = [0x09, 0x09, 0x01, 0x00, 0x40, 0x00, 0x01, 0x01];
const ANCHOR: [u16; 8] = [0, 747, 4, 28, 0, 839, 4, 246];
const CLOSED: u16 = 0x0100;

fn written_drawing(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut drawing = String::new();
    archive.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut drawing).unwrap();
    drawing
}

fn attr(tag: &str, name: &str) -> Option<f64> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    tag[at..].split('"').next()?.parse().ok()
}

/// Every `a:path` in the xml: its `<a:moveTo>`/`<a:lnTo>` points divided by the path's `w` and `h`, and
/// whether it ends with `<a:close/>`.
fn paths(xml: &str) -> Vec<(Vec<(f64, f64)>, bool)> {
    let mut out = Vec::new();
    for (i, _) in xml.match_indices("<a:path ") {
        let end = i + xml[i..].find("</a:path>").expect("closed a:path");
        let body = &xml[i..end];
        let head = &body[..body.find('>').unwrap()];
        let (w, h) = (attr(head, "w").expect("path w"), attr(head, "h").expect("path h"));
        let pts = body
            .match_indices("<a:pt ")
            .map(|(j, _)| {
                let tag = &body[j..j + body[j..].find("/>").unwrap()];
                (attr(tag, "x").unwrap() / w, attr(tag, "y").unwrap() / h)
            })
            .collect();
        out.push((pts, body.contains("<a:close/>")));
    }
    out
}

fn assert_points(got: &[(f64, f64)], want: &[(f64, f64)]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9, "{got:?} vs {want:?}");
    }
}

#[test]
fn biff5_polygon_opens_as_a_closed_freeform_with_its_points() {
    let recs = [polygon(70, ANCHOR, WHITE_AUTO, CLOSED, 7), coordlist(&ARROW)].concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().shapes.len(), 1, "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{:?}", report.warnings);
    let xml = written_drawing(&wb);
    assert!(xml.contains("<a:custGeom"), "{xml}");
    assert!(!xml.contains("<a:prstGeom"), "a freeform is not a preset: {xml}");
    let p = paths(&xml);
    assert_eq!(p.len(), 1, "{xml}");
    assert_points(&p[0].0, &ARROW_FRACTIONS);
    assert!(p[0].1, "wPolyFlags 0x0100: closed");
    // The first point is a moveTo, the rest lnTo.
    assert_eq!(xml.matches("<a:moveTo>").count(), 1, "{xml}");
    assert_eq!(xml.matches("<a:lnTo>").count(), 6, "{xml}");
}

#[test]
fn biff5_polygon_keeps_its_fill_and_outline() {
    let recs = [polygon(70, ANCHOR, WHITE_AUTO, CLOSED, 7), coordlist(&ARROW)].concat();
    let (wb, _) = read_xls(&book(&recs)).unwrap();
    let xml = written_drawing(&wb);
    let sp_pr = &xml[xml.find("<xdr:spPr>").unwrap()..xml.find("</xdr:spPr>").unwrap()];
    assert!(sp_pr.contains(r#"<a:solidFill><a:srgbClr val="FFFFFF"/>"#), "icv 9 solid fill: {sp_pr}");
    assert!(!sp_pr.contains("<a:ln><a:noFill/>") && !sp_pr.contains("<a:noFill/></a:ln>"), "outlined: {sp_pr}");
}

#[test]
fn biff5_open_polygon_has_no_close_and_unfilled_polygon_has_no_fill() {
    // fls 0: no fill; wPolyFlags 0: an open polyline.
    let fields = [0x09, 0x09, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00];
    let recs = [polygon(5, ANCHOR, fields, 0, 3), coordlist(&[(0, 0), (0x4000, 0x2000), (0, 0x4000)])].concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().shapes.len(), 1, "{:?}", report.warnings);
    let xml = written_drawing(&wb);
    let p = paths(&xml);
    assert_eq!(p.len(), 1, "{xml}");
    assert_points(&p[0].0, &[(0.0, 0.0), (1.0, 0.5), (0.0, 1.0)]);
    assert!(!p[0].1, "open polyline: {xml}");
    let sp_pr = &xml[xml.find("<xdr:spPr>").unwrap()..xml.find("</xdr:spPr>").unwrap()];
    assert!(!sp_pr.contains(r#"<a:solidFill><a:srgbClr val="FFFFFF"/>"#), "fls 0 is not filled: {sp_pr}");
}

#[test]
fn biff5_polygon_survives_a_second_open_and_save() {
    let recs = [polygon(70, ANCHOR, WHITE_AUTO, CLOSED, 7), coordlist(&ARROW)].concat();
    let (wb, _) = read_xls(&book(&recs)).unwrap();
    let once = write_xlsx(&wb).unwrap();
    let (again, _) = read_xlsx(&once).unwrap();
    let p = paths(&written_drawing(&again));
    assert_eq!(p.len(), 1);
    assert_points(&p[0].0, &ARROW_FRACTIONS);
    assert!(p[0].1);
}

#[test]
fn biff5_polygon_without_its_points_is_skipped_with_the_warning() {
    // No COORDLIST after the OBJ, and a COORDLIST shorter than cpt: nothing to draw, no panic.
    for recs in [polygon(1, ANCHOR, WHITE_AUTO, CLOSED, 7), [polygon(1, ANCHOR, WHITE_AUTO, CLOSED, 7), coordlist(&ARROW[..3])].concat()] {
        let (wb, report) = read_xls(&book(&recs)).unwrap();
        assert!(wb.sheet(0).unwrap().shapes.is_empty());
        assert!(report.warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{:?}", report.warnings);
    }
}

fn read_drawing(sp: &str) -> Workbook {
    let drawing = format!(
        r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{sp}<xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;
    let bytes = minimal(
        r#"<sheetData/><drawing r:id="rId1"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/drawings/drawing1.xml", &drawing)],
        "",
        "",
    );
    read_xlsx(&bytes).unwrap().0
}

/// A filled freeform triangle as Excel writes one: custom geometry with one closed path.
const TRIANGLE: &str = r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Freeform 1"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="1143000"/></a:xfrm><a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="l" t="t" r="r" b="b"/><a:pathLst><a:path w="1828800" h="1143000"><a:moveTo><a:pt x="0" y="1143000"/></a:moveTo><a:lnTo><a:pt x="914400" y="0"/></a:lnTo><a:lnTo><a:pt x="1828800" y="1143000"/></a:lnTo><a:close/></a:path></a:pathLst></a:custGeom><a:solidFill><a:srgbClr val="4472C4"/></a:solidFill></xdr:spPr></xdr:sp>"#;

#[test]
fn xlsx_freeform_keeps_its_custom_geometry_through_save() {
    let wb = read_drawing(TRIANGLE);
    assert_eq!(wb.sheet(0).unwrap().shapes.len(), 1);
    let xml = written_drawing(&wb);
    assert!(xml.contains("<a:custGeom"), "{xml}");
    assert!(!xml.contains(r#"<a:prstGeom prst="rect""#), "not a rectangle: {xml}");
    let p = paths(&xml);
    assert_eq!(p.len(), 1, "{xml}");
    assert_points(&p[0].0, &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)]);
    assert!(p[0].1);
    assert!(xml.contains(r#"<a:srgbClr val="4472C4"/>"#), "{xml}");
    assert!(xml.contains(r#"name="Freeform 1""#), "{xml}");
}

#[test]
fn custom_geometry_without_a_path_is_still_not_imported() {
    // Guard (matches the empty-path rule): an empty pathLst draws nothing.
    let empty = TRIANGLE
        .replace(&TRIANGLE[TRIANGLE.find("<a:pathLst>").unwrap()..TRIANGLE.find("</a:pathLst>").unwrap() + "</a:pathLst>".len()], "<a:pathLst/>");
    let wb = read_drawing(&empty);
    assert!(wb.sheet(0).is_none_or(|s| s.shapes.is_empty()) || !written_drawing(&wb).contains("<a:path "));
}
