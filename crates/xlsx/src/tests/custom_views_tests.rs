//! Custom Views (View > Custom Views) survive open + save: the workbook's `<customWorkbookViews>`
//! and each sheet's `<customSheetViews>`, with the printer settings parts the views' page setups
//! name. Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PRINTER: &[u8] = &[7, 7, 1, 2, 3, 250, 251, 0, 9];
const GUID_A: &str = "{11111111-AAAA-4000-8000-000000000001}";
const GUID_B: &str = "{22222222-BBBB-4000-8000-000000000002}";
const GUID_C: &str = "{33333333-CCCC-4000-8000-000000000003}";

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet3.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

fn view(guid: &str, page_setup_rid: Option<&str>) -> String {
    let ps = page_setup_rid.map(|r| format!(r#"<pageSetup orientation="landscape" r:id="{r}"/>"#)).unwrap_or_default();
    format!(
        r#"<customSheetView guid="{guid}" scale="85" showGridLines="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="B3" sqref="B3"/><pageMargins left="0.5" right="0.5" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>{ps}</customSheetView>"#
    )
}

/// Sheets "Data" (id 1), "Other" (id 2) and "Gone" (id 5); workbook views A (on Data) and B (on
/// Gone), given as `wb_views`. Data holds views A (with printer settings), B and C (no workbook
/// view); Other holds A (guid in lower case); Gone holds B.
fn package_with(wb_views: &str) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><bookViews><workbookView/></bookViews><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="Gone" sheetId="5" r:id="rId3"/></sheets><calcPr calcId="191029"/>{wb_views}</workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{NS_R}/worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="{NS_R}/worksheet" Target="worksheets/sheet3.xml"/></Relationships>"#
    );
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Head</t></is></c><c r="B1" t="inlineStr"><is><t>Two</t></is></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c></row></sheetData><autoFilter ref="A1:B2"/><customSheetViews>{}{}{}</customSheetViews><mergeCells count="1"><mergeCell ref="C1:D1"/></mergeCells><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>"#,
        view(GUID_A, Some("rId1")),
        view(GUID_B, None),
        view(GUID_C, None)
    );
    let sheet2 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>2</v></c></row></sheetData><customSheetViews>{}</customSheetViews><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>"#,
        view(&GUID_A.to_ascii_lowercase(), None)
    );
    let sheet3 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>3</v></c></row></sheetData><customSheetViews>{}</customSheetViews><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>"#,
        view(GUID_B, None)
    );
    let s1_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/printerSettings" Target="../printerSettings/printerSettings1.bin"/></Relationships>"#
    );
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
        ("xl/worksheets/sheet3.xml", sheet3.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", s1_rels.as_bytes()),
        ("xl/printerSettings/printerSettings1.bin", PRINTER),
    ])
}

fn package() -> Vec<u8> {
    package_with(&format!(
        r#"<customWorkbookViews><customWorkbookView name="Print" guid="{GUID_A}" includePrintSettings="1" windowWidth="800" windowHeight="600" activeSheetId="1"/><customWorkbookView name="Mine" guid="{GUID_B}" personalView="1" windowWidth="900" windowHeight="700" activeSheetId="5"/></customWorkbookViews>"#
    ))
}

fn unpack(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = vec![];
        f.read_to_end(&mut v).unwrap();
        parts.insert(f.name().to_string(), v);
    }
    parts
}

fn text(p: &HashMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8_lossy(p.get(name).unwrap_or_else(|| panic!("no part {name}"))).into_owned()
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    Some(s[i..i + s[i..].find(b)?].to_string())
}

/// The element `<tag …>…</tag>` of `xml` (first one), if any.
fn element(xml: &str, tag: &str) -> Option<String> {
    let i = xml.find(&format!("<{tag}>")).or_else(|| xml.find(&format!("<{tag} ")))?;
    let close = format!("</{tag}>");
    let end = xml[i..].find(&close)? + close.len();
    Some(xml[i..i + end].to_string())
}

/// The part of the written sheet `name`.
fn sheet_part(p: &HashMap<String, Vec<u8>>, name: &str) -> String {
    let wb = text(p, "xl/workbook.xml");
    let i = wb.find(&format!("name=\"{name}\"")).unwrap_or_else(|| panic!("no sheet {name}"));
    let rid = between(&wb[i..], "r:id=\"", "\"").unwrap();
    let rels = text(p, "xl/_rels/workbook.xml.rels");
    let j = rels.find(&format!("Id=\"{rid}\"")).unwrap();
    let start = rels[..j].rfind('<').unwrap();
    let target = between(&rels[start..], "Target=\"", "\"").unwrap();
    match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => format!("xl/{target}"),
    }
}

/// Relationship `id` of the sheet part `part`: (type, resolved part name).
fn sheet_rel(p: &HashMap<String, Vec<u8>>, part: &str, id: &str) -> Option<(String, String)> {
    let (dir, file) = part.rsplit_once('/')?;
    let rels = String::from_utf8_lossy(p.get(&format!("{dir}/_rels/{file}.rels"))?).into_owned();
    let i = rels.find(&format!("Id=\"{id}\""))?;
    let start = rels[..i].rfind('<')?;
    let r = &rels[start..start + rels[start..].find("/>")?];
    let target = between(r, "Target=\"", "\"")?;
    let full = match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => {
            let mut segs: Vec<&str> = dir.split('/').collect();
            for s in target.split('/') {
                if s == ".." {
                    segs.pop();
                } else {
                    segs.push(s);
                }
            }
            segs.join("/")
        }
    };
    Some((between(r, "Type=\"", "\"")?, full))
}

fn views_of(sheet_xml: &str) -> Vec<String> {
    let Some(block) = element(sheet_xml, "customSheetViews") else { return vec![] };
    block.match_indices("<customSheetView ").map(|(i, _)| between(&block[i..], "guid=\"", "\"").unwrap().to_ascii_uppercase()).collect()
}

fn roundtrip(bytes: &[u8]) -> (HashMap<String, Vec<u8>>, Vec<String>) {
    let (wb, report) = read_xlsx(bytes).unwrap();
    (unpack(&write_xlsx(&wb).unwrap()), report.warnings)
}

#[test]
fn views_survive_open_and_save() {
    let (p, _) = roundtrip(&package());
    let wb = text(&p, "xl/workbook.xml");
    assert_eq!(wb.matches("<customWorkbookViews>").count(), 1, "{wb}");
    assert_eq!(wb.matches("<customWorkbookView ").count(), 2, "{wb}");
    assert!(wb.contains(&format!("name=\"Print\" guid=\"{GUID_A}\"")), "{wb}");
    assert!(wb.contains("personalView=\"1\""), "{wb}");
    let data = text(&p, &sheet_part(&p, "Data"));
    let other = text(&p, &sheet_part(&p, "Other"));
    let gone = text(&p, &sheet_part(&p, "Gone"));
    assert_eq!(data.matches("<customSheetViews>").count(), 1, "{data}");
    assert_eq!(views_of(&data), vec![GUID_A.to_string(), GUID_B.to_string()], "view C has no workbook view: {data}");
    assert_eq!(views_of(&other), vec![GUID_A.to_string()], "guids match case-insensitively: {other}");
    assert_eq!(views_of(&gone), vec![GUID_B.to_string()]);
    assert!(data.contains("<pane ySplit=\"1\" topLeftCell=\"A2\""), "{data}");
    // Cells are untouched.
    assert!(data.contains("<v>2</v>"), "{data}");
}

#[test]
fn page_setup_rel_of_a_view_resolves_to_printer_settings() {
    let (p, _) = roundtrip(&package());
    let part = sheet_part(&p, "Data");
    let data = text(&p, &part);
    let block = element(&data, "customSheetViews").unwrap();
    let i = block.find("<pageSetup ").expect("the view keeps its page setup");
    let tag = &block[i..i + block[i..].find("/>").unwrap()];
    let rid = between(tag, "r:id=\"", "\"").expect("printer settings are kept, so is the r:id");
    let (ty, target) = sheet_rel(&p, &part, &rid).unwrap_or_else(|| panic!("{rid} is not in the sheet's rels"));
    assert!(ty.ends_with("/printerSettings"), "{ty}");
    assert_eq!(p.get(&target).map(Vec::as_slice), Some(PRINTER), "{target}");
}

#[test]
fn page_setup_rel_is_dropped_when_the_part_cannot_be_kept() {
    // The view's r:id names a relationship the sheet doesn't have.
    let bytes = package();
    let mut parts = unpack(&bytes);
    parts.remove("xl/worksheets/_rels/sheet1.xml.rels");
    let list: Vec<(&str, &[u8])> = parts.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
    let (p, _) = roundtrip(&make_zip(&list));
    let data = text(&p, &sheet_part(&p, "Data"));
    let block = element(&data, "customSheetViews").unwrap();
    assert!(block.contains("<pageSetup orientation=\"landscape\"/>"), "{block}");
    assert!(!block.contains("r:id"), "{block}");
}

#[test]
fn elements_are_written_in_schema_order() {
    let (p, _) = roundtrip(&package());
    let data = text(&p, &sheet_part(&p, "Data"));
    let at = |tag: &str| data.find(&format!("<{tag}")).unwrap_or_else(|| panic!("no <{tag}> in {data}"));
    assert!(at("autoFilter") < at("customSheetViews"), "{data}");
    assert!(at("customSheetViews") < at("mergeCells"), "{data}");
    assert!(at("customSheetViews") < at("pageMargins "), "{data}");
    let other = text(&p, &sheet_part(&p, "Other"));
    assert!(other.find("<customSheetViews>").unwrap() < other.find("<pageMargins ").unwrap(), "{other}");
    let wb = text(&p, "xl/workbook.xml");
    let views = wb.find("<customWorkbookViews>").unwrap();
    assert!(wb.find("<calcPr").unwrap() < views, "{wb}");
    let after = wb.find("<pivotCaches").or_else(|| wb.find("<extLst")).unwrap_or_else(|| wb.find("</workbook>").unwrap());
    assert!(views < after, "{wb}");
    // Every written XML part is well formed.
    for (name, data) in &p {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            crate::xml::parse(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

#[test]
fn active_sheet_of_a_deleted_sheet_is_rewritten() {
    let (mut wb, _) = read_xlsx(&package()).unwrap();
    let gone = wb.sheets.iter().position(|s| s.name == "Gone").unwrap();
    wb.sheets.remove(gone);
    let p = unpack(&write_xlsx(&wb).unwrap());
    let x = text(&p, "xl/workbook.xml");
    let ids: Vec<String> = x.match_indices("sheetId=\"").map(|(i, _)| between(&x[i..], "sheetId=\"", "\"").unwrap()).collect();
    assert!(!ids.contains(&"5".to_string()), "{x}");
    let views = element(&x, "customWorkbookViews").unwrap();
    let active: Vec<String> = views.match_indices("activeSheetId=\"").map(|(i, _)| between(&views[i..], "activeSheetId=\"", "\"").unwrap()).collect();
    assert_eq!(active, vec!["1".to_string(), "1".to_string()], "{views}");
    assert!(active.iter().all(|a| ids.contains(a)));
}

#[test]
fn damaged_workbook_views_are_dropped_with_one_warning() {
    let bytes = package_with(&format!(
        r#"<customWorkbookViews><customWorkbookView name="Print" windowWidth="800" windowHeight="600" activeSheetId="1"/><customWorkbookView name="Mine" guid="{GUID_B}" activeSheetId="oops"/></customWorkbookViews>"#
    ));
    let (p, warnings) = roundtrip(&bytes);
    let dropped: Vec<&String> = warnings.iter().filter(|w| w.contains("custom views were dropped")).collect();
    assert_eq!(dropped.len(), 1, "{warnings:?}");
    let wb = text(&p, "xl/workbook.xml");
    assert!(!wb.contains("customWorkbookView"), "{wb}");
    for name in ["Data", "Other", "Gone"] {
        let x = text(&p, &sheet_part(&p, name));
        assert!(!x.contains("customSheetView"), "{name}: {x}");
    }
    let data = text(&p, &sheet_part(&p, "Data"));
    assert!(data.contains("<v>1</v>") && data.contains("<v>2</v>"), "{data}");
}

#[test]
fn sheet_views_are_dropped_without_workbook_views() {
    let (p, _) = roundtrip(&package_with(""));
    for name in ["Data", "Other", "Gone"] {
        let x = text(&p, &sheet_part(&p, name));
        assert!(!x.contains("customSheetView"), "{name}: {x}");
    }
}

#[test]
fn second_save_is_identical_and_clean() {
    let (wb1, report1) = read_xlsx(&package()).unwrap();
    let saved1 = write_xlsx(&wb1).unwrap();
    let (wb2, report2) = read_xlsx(&saved1).unwrap();
    let new: Vec<&String> = report2.warnings.iter().filter(|w| !report1.warnings.contains(w)).collect();
    assert!(new.is_empty(), "{new:?}");
    let saved2 = write_xlsx(&wb2).unwrap();
    let (p1, p2) = (unpack(&saved1), unpack(&saved2));
    let wb_views = |p: &HashMap<String, Vec<u8>>| element(&text(p, "xl/workbook.xml"), "customWorkbookViews");
    assert!(wb_views(&p1).is_some());
    assert_eq!(wb_views(&p1), wb_views(&p2));
    for name in ["Data", "Other", "Gone"] {
        let v1 = element(&text(&p1, &sheet_part(&p1, name)), "customSheetViews");
        let v2 = element(&text(&p2, &sheet_part(&p2, name)), "customSheetViews");
        assert!(v1.is_some(), "{name}");
        assert_eq!(v1, v2, "{name}");
    }
}
