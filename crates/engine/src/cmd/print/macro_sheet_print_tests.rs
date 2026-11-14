//! Printing the whole workbook prints each visible Excel 4.0 macro sheet ([MS-XLSX] 2.1.11
//! `CT_Macrosheet`, `xl/macrosheets/sheetN.xml`; [MS-XLS] BoundSheet8 `dt = 1`) in its tab place,
//! like a worksheet: its cells over its own page setup, showing formulas when its sheet view does
//! (`showFormulas="1"`, .xls WINDOW2 `fDspFmla`, which Excel sets on macro sheets). Hidden and empty
//! macro sheets print nothing. Measured on Excel's own prints (Microsoft 365): a workbook with
//! worksheets "Sheet A", "Sheet B" and macro sheet "Macro1" prints 3 pages, the third the macro
//! sheet's text; a .xls with a worksheet and a macro sheet prints the macro sheet's formulas
//! (`=ALIGNMENT(2,FALSE,1,0,FALSE)`) on its own page after the worksheet. The macro sheets stay
//! hidden from the grid and are saved exactly as before. Packages are built in code.
use std::io::Write as _;

use serde_json::json;

use super::*;

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";
const MACRO_REL: &str = "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet";

/// One sheet of the test package: (name, `Some(state)` for the `state` attribute, kind, sheetData xml).
struct Tab<'a> {
    name: &'a str,
    state: Option<&'a str>,
    macro_sheet: bool,
    data: &'a str,
    show_formulas: bool,
}

fn package(tabs: &[Tab]) -> Vec<u8> {
    let mut types = String::from(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.ms-excel.sheet.macroEnabled.main+xml"/>"#,
    );
    let mut sheets = String::new();
    let mut rels = String::new();
    let mut parts: Vec<(String, String)> = vec![];
    for (i, t) in tabs.iter().enumerate() {
        let n = i + 1;
        let state = t.state.map(|s| format!(r#" state="{s}""#)).unwrap_or_default();
        sheets.push_str(&format!(r#"<sheet name="{}" sheetId="{n}"{state} r:id="rId{n}"/>"#, t.name));
        let view = if t.show_formulas { r#"<sheetViews><sheetView showFormulas="1" workbookViewId="0"/></sheetViews>"# } else { "" };
        let margins = r#"<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>"#;
        if t.macro_sheet {
            let part = format!("xl/macrosheets/sheet{n}.xml");
            types.push_str(&format!(r#"<Override PartName="/{part}" ContentType="application/vnd.ms-excel.macrosheet+xml"/>"#));
            rels.push_str(&format!(r#"<Relationship Id="rId{n}" Type="{MACRO_REL}" Target="macrosheets/sheet{n}.xml"/>"#));
            parts.push((part, format!(r#"<?xml version="1.0"?><xm:macrosheet xmlns="{NS_MAIN}" xmlns:xm="{NS_XM}" xmlns:r="{NS_R}">{view}<sheetData>{}</sheetData>{margins}</xm:macrosheet>"#, t.data)));
        } else {
            let part = format!("xl/worksheets/sheet{n}.xml");
            types.push_str(&format!(
                r#"<Override PartName="/{part}" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
            ));
            rels.push_str(&format!(r#"<Relationship Id="rId{n}" Type="{NS_R}/worksheet" Target="worksheets/sheet{n}.xml"/>"#));
            parts.push((
                part,
                format!(
                    r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}">{view}<sheetData>{}</sheetData>{margins}</worksheet>"#,
                    t.data
                ),
            ));
        }
    }
    types.push_str("</Types>");
    let workbook = format!(r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets>{sheets}</sheets></workbook>"#);
    let wb_rels =
        format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#);
    let root = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut put = |name: &str, data: &str| {
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(data.as_bytes()).unwrap();
    };
    put("[Content_Types].xml", &types);
    put("_rels/.rels", root);
    put("xl/workbook.xml", &workbook);
    put("xl/_rels/workbook.xml.rels", &wb_rels);
    for (name, data) in &parts {
        put(name, data);
    }
    out.finish().unwrap().into_inner()
}

const DATA_A: &str = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>Content of Sheet A</t></is></c></row>"#;
const DATA_B: &str = r#"<row r="1"><c r="A1"><v>4</v></c></row>"#;
const MACRO: &str =
    r#"<row r="1"><c r="A1" t="str"><f>ECHO(FALSE)</f><v></v></c></row><row r="2"><c r="A2" t="str"><f>RETURN()</f><v></v></c></row>"#;

fn open(tabs: &[Tab]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "book.xlsm", "base64": crate::io::base64_encode(&package(tabs))})).unwrap();
    s
}

/// (sheet name, range) of every page `file.printPreview` lists for `params`.
fn pages(s: &mut Session, params: serde_json::Value) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", params).unwrap();
    pv["pageList"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["sheet"].as_str().unwrap().to_string(), p["range"].as_str().unwrap_or("").to_string()))
        .collect()
}

fn names(p: &[(String, String)]) -> Vec<&str> {
    p.iter().map(|(n, _)| n.as_str()).collect()
}

fn tab<'a>(name: &'a str, macro_sheet: bool, data: &'a str) -> Tab<'a> {
    Tab { name, state: None, macro_sheet, data, show_formulas: macro_sheet }
}

#[test]
fn a_visible_macro_sheet_prints_in_its_tab_place() {
    let mut s = open(&[tab("Sheet A", false, DATA_A), tab("Macro1", true, MACRO), tab("Sheet B", false, DATA_B)]);
    let p = pages(&mut s, json!({"sheets": "all"}));
    assert_eq!(names(&p), ["Sheet A", "Macro1", "Sheet B"]);
    assert_eq!(p[1].1, "A1:A2");
}

#[test]
fn a_macro_sheet_last_prints_last() {
    let mut s = open(&[tab("Sheet A", false, DATA_A), tab("Sheet B", false, DATA_B), tab("Macro1", true, MACRO)]);
    assert_eq!(names(&pages(&mut s, json!({"sheets": "all"}))), ["Sheet A", "Sheet B", "Macro1"]);
}

#[test]
fn a_hidden_macro_sheet_prints_nothing() {
    let hidden = Tab { state: Some("hidden"), ..tab("Macro1", true, MACRO) };
    let mut s = open(&[tab("Sheet A", false, DATA_A), hidden, tab("Sheet B", false, DATA_B)]);
    assert_eq!(names(&pages(&mut s, json!({"sheets": "all"}))), ["Sheet A", "Sheet B"]);
}

#[test]
fn an_empty_macro_sheet_prints_nothing() {
    let mut s = open(&[tab("Sheet A", false, DATA_A), tab("Macro1", true, ""), tab("Sheet B", false, DATA_B)]);
    assert_eq!(names(&pages(&mut s, json!({"sheets": "all"}))), ["Sheet A", "Sheet B"]);
}

#[test]
fn printing_the_active_sheet_does_not_add_the_macro_sheet() {
    let mut s = open(&[tab("Sheet A", false, DATA_A), tab("Macro1", true, MACRO), tab("Sheet B", false, DATA_B)]);
    assert_eq!(names(&pages(&mut s, json!({}))), ["Sheet A"]);
}

#[test]
fn the_pdf_counts_the_macro_sheet_page() {
    let mut s = open(&[tab("Sheet A", false, DATA_A), tab("Macro1", true, MACRO)]);
    let out = s.execute("file.print", json!({"sheets": "all"})).unwrap();
    assert_eq!(out["pages"], json!(2));
}

#[test]
fn the_macro_sheet_stays_out_of_the_grid() {
    let s = open(&[tab("Sheet A", false, DATA_A), tab("Macro1", true, MACRO), tab("Sheet B", false, DATA_B)]);
    let wb = &s.doc().unwrap().wb;
    let grid: Vec<&str> = wb.sheets.iter().map(|sh| sh.name.as_str()).collect();
    assert_eq!(grid, ["Sheet A", "Sheet B"]);
}
