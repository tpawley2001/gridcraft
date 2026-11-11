//! With no print area, a sheet prints from A1 to its last used cell or drawing object, like
//! Excel: leading empty rows and columns print as blank space, and pages that hold nothing are
//! still printed. Expected values measured from Microsoft 365 Excel's own PDF prints.

use std::io::Write;

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument";

fn rels(id: &str, kind: &str, target: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="{id}" Type="{NS_R}/{kind}" Target="{target}"/></Relationships>"#
    )
}

/// A text box anchored from (col, row) to (col, row), zero-based, with EMU offsets on the end.
fn text_box(from: (u32, u32), to: (u32, u32, u32, u32)) -> String {
    format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:twoCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to><xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Box"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></xdr:spPr><xdr:txBody><a:bodyPr/><a:p><a:r><a:t>note</a:t></a:r></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#,
        from.0, from.1, to.0, to.1, to.2, to.3
    )
}

/// One sheet "S" (Letter, margins 0.7/0.7/0.75/0.75 in) holding `cells` (A1 ref, number), an
/// optional drawing and an optional print area.
fn package(cells: &[(&str, f64)], drawing: Option<String>, print_area: Option<&str>) -> Vec<u8> {
    let mut rows: Vec<(u32, String)> = Vec::new();
    for (r, v) in cells {
        let n: u32 = r.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse().unwrap();
        let c = format!(r#"<c r="{r}"><v>{v}</v></c>"#);
        match rows.iter_mut().find(|(k, _)| *k == n) {
            Some((_, s)) => s.push_str(&c),
            None => rows.push((n, c)),
        }
    }
    rows.sort_by_key(|(k, _)| *k);
    let data: String = rows.iter().map(|(n, c)| format!(r#"<row r="{n}">{c}</row>"#)).collect();
    let dref = if drawing.is_some() { r#"<drawing r:id="rId1"/>"# } else { "" };
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetFormatPr defaultRowHeight="15"/><sheetData>{data}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait"/>{dref}</worksheet>"#
    );
    let names = match print_area {
        Some(a) => format!(r#"<definedNames><definedName name="_xlnm.Print_Area" localSheetId="0">{a}</definedName></definedNames>"#),
        None => String::new(),
    };
    let mut parts: Vec<(&str, String)> = vec![
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
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets>{names}</workbook>"#
            ),
        ),
        ("xl/_rels/workbook.xml.rels", rels("rId1", "worksheet", "worksheets/sheet1.xml")),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    if let Some(d) = drawing {
        parts.push(("xl/worksheets/_rels/sheet1.xml.rels", rels("rId1", "drawing", "../drawings/drawing1.xml")));
        parts.push(("xl/drawings/drawing1.xml", d));
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "from_a1.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn pages(s: &mut Session) -> Vec<Json> {
    s.execute("file.printPreview", json!({})).unwrap()["pageList"].as_array().unwrap().clone()
}

fn range(p: &Json) -> &str {
    p["range"].as_str().unwrap()
}

/// The 1-based row numbers a page range "A1:J45" spans.
fn rows_of(r: &str) -> (u32, u32) {
    let num = |s: &str| s.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse::<u32>().unwrap();
    let (a, b) = r.split_once(':').unwrap_or((r, r));
    (num(a), num(b))
}

#[test]
fn data_away_from_a1_prints_from_a1() {
    let mut s = open(&package(&[("C5", 1.0), ("D6", 2.0)], None, None));
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(range(&p[0]), "A1:D6");
}

#[test]
fn leading_blank_rows_print_as_blank_pages() {
    // Excel: one value at A2001 prints 43 pages, the first 42 blank (row heights in print are
    // the subject of separate work, so only "dozens of pages, starting at row 1" is pinned).
    let mut s = open(&package(&[("A2001", 7.0)], None, None));
    let p = pages(&mut s);
    assert!(p.len() > 40, "{} pages: {p:?}", p.len());
    assert!(range(&p[0]).starts_with("A1:"), "{p:?}");
    let (first, last) = rows_of(range(p.last().unwrap()));
    assert!(first <= 2001 && 2001 <= last, "last page {:?}", p.last());
    let pdf = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(pdf["pages"].as_u64().unwrap() as usize, p.len());
}

#[test]
fn leading_blank_columns_push_a_drawing_onto_a_second_page() {
    // Excel: values in H1:I5 and an object anchored E9 .. L23 print on 2 pages; page 1 starts
    // at column A and holds the values, page 2 the right part of the object.
    let cells = [("H1", 1.0), ("I1", 2.0), ("H2", 25.0), ("I2", 75.0), ("H5", 25.0), ("I5", 124.0)];
    let mut s = open(&package(&cells, Some(text_box((4, 8), (11, 304800, 22, 76200))), None));
    let p = pages(&mut s);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(range(&p[0]).starts_with("A1:"), "{p:?}");
    assert_eq!(rows_of(range(&p[1])).0, 1, "{p:?}");
}

#[test]
fn print_area_away_from_a1_is_kept() {
    let mut s = open(&package(&[("C5", 1.0), ("D6", 2.0), ("A1", 3.0)], None, Some("S!$C$5:$D$6")));
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(range(&p[0]), "C5:D6");
}

#[test]
fn data_from_a1_is_unchanged() {
    let mut s = open(&package(&[("A1", 1.0), ("B2", 2.0)], None, None));
    let p = pages(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(range(&p[0]), "A1:B2");
}

#[test]
fn empty_sheet_prints_one_page() {
    let mut s = open(&package(&[], None, None));
    assert_eq!(s.execute("file.exportPdf", json!({})).unwrap()["pages"], 1);
}
