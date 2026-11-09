//! Alternative text (`xdr:cNvPr descr`, ECMA-376 Part 1 §20.1.2.2.8) on shapes, text boxes and
//! charts is kept through XLSX open + save, and Review › Check Accessibility reports shapes and
//! charts without it, as it does for pictures.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const SHAPE: &str = "nvSpPr";
const CHART: &str = "nvGraphicFramePr";

/// A workbook with values in A1:B3, one rectangle and one bar chart, saved as XLSX.
fn saved_book() -> Vec<u8> {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([["Item", "Qty"], ["A", 1], ["B", 2]]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("insert.shape", json!({"kind": "rectangle"})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:B3", "type": "bar"})).unwrap();
    gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "alt.xlsx").unwrap()
}

fn drawing_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

/// The `cNvPr` start tag of the first object whose non-visual properties element is `nv`.
fn cnvpr(xml: &str, nv: &str) -> String {
    let at = xml.find(&format!("<xdr:{nv}>")).unwrap_or_else(|| panic!("no {nv} in {xml}"));
    let start = at + xml[at..].find("<xdr:cNvPr ").expect("cNvPr");
    xml[start..start + xml[start..].find('>').unwrap()].to_string()
}

/// `bytes` with `descr` (already XML-escaped) added to the `cNvPr` of each object in `targets`.
fn with_descr(bytes: &[u8], targets: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    let mut w = zip::ZipWriter::new(&mut out);
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut body = Vec::new();
        f.read_to_end(&mut body).unwrap();
        if name == "xl/drawings/drawing1.xml" {
            let mut xml = String::from_utf8(body).unwrap();
            for (nv, descr) in targets {
                let at = xml.find(&format!("<xdr:{nv}>")).unwrap_or_else(|| panic!("no {nv}"));
                let tag = at + xml[at..].find("<xdr:cNvPr ").unwrap() + "<xdr:cNvPr ".len();
                xml.insert_str(tag, &format!(r#"descr="{descr}" "#));
            }
            body = xml.into_bytes();
        }
        w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(&body).unwrap();
    }
    w.finish().unwrap();
    out.into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "alt.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn resave(bytes: &[u8]) -> Vec<u8> {
    let s = open(bytes);
    gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "alt.xlsx").unwrap()
}

fn issues(s: &mut Session, issue: &str) -> Vec<Json> {
    let all = s.execute("review.checkAccessibility", json!({})).unwrap();
    all.as_array().unwrap().iter().filter(|i| i["issue"] == issue).cloned().collect()
}

#[test]
fn shape_alt_text_is_kept_on_save() {
    let source = with_descr(&saved_book(), &[(SHAPE, "Sales &amp; costs &quot;Q1&quot;")]);
    let tag = cnvpr(&drawing_xml(&resave(&source)), SHAPE);
    assert!(tag.contains(r#"descr="Sales &amp; costs &quot;Q1&quot;""#), "{tag}");
}

#[test]
fn chart_alt_text_is_kept_on_save() {
    let source = with_descr(&saved_book(), &[(CHART, "Quantity by item")]);
    let tag = cnvpr(&drawing_xml(&resave(&source)), CHART);
    assert!(tag.contains(r#"descr="Quantity by item""#), "{tag}");
}

#[test]
fn objects_without_alt_text_save_no_descr() {
    let xml = drawing_xml(&resave(&saved_book()));
    for nv in [SHAPE, CHART] {
        let tag = cnvpr(&xml, nv);
        assert!(!tag.contains("descr="), "{tag}");
    }
}

#[test]
fn accessibility_reports_shapes_and_charts_without_alt_text() {
    let mut s = open(&saved_book());
    let missing = issues(&mut s, "Missing alternative text");
    assert_eq!(missing.len(), 2, "{missing:?}");
    for i in &missing {
        assert_eq!(i["sheet"], "Sheet1");
        assert!(i["object"].as_str().is_some_and(|o| !o.is_empty()), "{i}");
    }
}

#[test]
fn accessibility_skips_described_objects() {
    let mut s = open(&with_descr(&saved_book(), &[(SHAPE, "Box"), (CHART, "Chart of quantities")]));
    assert_eq!(issues(&mut s, "Missing alternative text"), Vec::<Json>::new());
}

#[test]
fn only_numbered_default_sheet_names_are_reported() {
    let mut s = Session::new();
    s.new_workbook();
    assert_eq!(issues(&mut s, "Default sheet name").len(), 1);
    s.execute("sheet.rename", json!({"sheet": 0, "name": "Sheet totals"})).unwrap();
    assert_eq!(issues(&mut s, "Default sheet name"), Vec::<Json>::new());
}
