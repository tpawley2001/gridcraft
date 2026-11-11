//! A print area made of several ranges (`_xlnm.Print_Area` = `S!$A$1:$B$2,S!$D$3:$D$5`) prints
//! each range starting on a new page, in the listed order, and keeps every range on save, like
//! Excel. Page counts measured from Microsoft 365 Excel's own PDF prints (two ranges -> 2 pages).

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

/// One sheet named `sheet` (Letter) holding `cells` (A1 ref, number) and the print area `area`.
fn package(sheet_name: &str, cells: &[(&str, f64)], area: &str) -> Vec<u8> {
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
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetFormatPr defaultRowHeight="15"/><sheetData>{data}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait"/></worksheet>"#
    );
    let name_attr = sheet_name.replace('&', "&amp;").replace('\'', "&apos;");
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/></Types>"#
            ),
        ),
        ("_rels/.rels", rels("rId1", "officeDocument", "xl/workbook.xml")),
        (
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="{name_attr}" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="_xlnm.Print_Area" localSheetId="0">{}</definedName></definedNames></workbook>"#,
                area.replace('&', "&amp;")
            ),
        ),
        ("xl/_rels/workbook.xml.rels", rels("rId1", "worksheet", "worksheets/sheet1.xml")),
        ("xl/worksheets/sheet1.xml", sheet),
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

fn open_b64(b64: &str) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "areas.xlsx", "base64": b64})).unwrap();
    s
}

fn open(bytes: &[u8]) -> Session {
    open_b64(&gridcraft_engine::io::base64_encode(bytes))
}

fn ranges(s: &mut Session) -> Vec<String> {
    let v = s.execute("file.printPreview", json!({})).unwrap();
    v["pageList"].as_array().unwrap().iter().map(|p: &Json| p["range"].as_str().unwrap().to_string()).collect()
}

const CELLS: [(&str, f64); 6] = [("A1", 1.0), ("B2", 2.0), ("C3", 9.0), ("D3", 3.0), ("D5", 5.0), ("F8", 8.0)];

#[test]
fn two_ranges_print_on_two_pages() {
    // Excel: `$A$1:$B$2,$D$3:$D$5` prints 2 pages (ours printed only the first range).
    let mut s = open(&package("S", &CELLS, "S!$A$1:$B$2,S!$D$3:$D$5"));
    assert_eq!(ranges(&mut s), ["A1:B2", "D3:D5"]);
    assert_eq!(s.execute("file.exportPdf", json!({})).unwrap()["pages"], 2);
}

#[test]
fn ranges_print_in_the_listed_order() {
    let mut s = open(&package("S", &CELLS, "S!$D$3:$D$5,S!$A$1:$B$2"));
    assert_eq!(ranges(&mut s), ["D3:D5", "A1:B2"]);
}

#[test]
fn three_ranges_with_quoted_sheet_name() {
    let mut s = open(&package("My Sheet", &CELLS, "'My Sheet'!$A$1:$B$2,'My Sheet'!$D$3:$D$5,'My Sheet'!$F$8"));
    assert_eq!(ranges(&mut s), ["A1:B2", "D3:D5", "F8"]);
}

#[test]
fn comma_inside_a_quoted_sheet_name_is_not_a_separator() {
    let mut s = open(&package("Sheet1, Sheet3", &CELLS, "'Sheet1, Sheet3'!$A$1:$D$5"));
    assert_eq!(ranges(&mut s), ["A1:D5"]);
}

#[test]
fn xlsx_save_keeps_every_range() {
    let mut s = open(&package("S", &CELLS, "S!$A$1:$B$2,S!$D$3:$D$5"));
    let saved = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap();
    let mut back = open_b64(saved["base64"].as_str().unwrap());
    assert_eq!(ranges(&mut back), ["A1:B2", "D3:D5"]);
}

#[test]
fn inserting_a_row_above_moves_every_range() {
    let mut s = open(&package("S", &CELLS, "S!$A$1:$B$2,S!$D$3:$D$5"));
    s.execute("home.insertRows", json!({"rows": "1:1"})).unwrap();
    assert_eq!(ranges(&mut s), ["A2:B3", "D4:D6"]);
}

#[test]
fn single_range_is_unchanged() {
    let mut s = open(&package("S", &CELLS, "S!$A$1:$D$5"));
    assert_eq!(ranges(&mut s), ["A1:D5"]);
    assert_eq!(s.execute("file.exportPdf", json!({})).unwrap()["pages"], 1);
}
