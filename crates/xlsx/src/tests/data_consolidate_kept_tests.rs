//! A sheet's Data > Consolidate settings (`dataConsolidate`, ECMA-376 Part 1 18.3.1.29) are kept on
//! XLSX save, with the `externalLinkPath` relationships of sources in other workbooks.

use std::collections::HashMap;
use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const EXT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath";
const MARGINS: &str = r#"<pageMargins left="0.5" right="0.5" top="1" bottom="1" header="0.3" footer="0.3"/>"#;
const DATA: &str = r#"<sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row></sheetData>"#;

fn unzip(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut m = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        m.insert(f.name().to_string(), String::from_utf8_lossy(&b).into_owned());
    }
    m
}

/// The value of attribute `name` in the start tag `el`.
fn attr(el: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let start = el.find(&key)? + key.len();
    let end = el[start..].find('"')? + start;
    Some(el[start..end].to_string())
}

/// The start tags of every `tag` element that has attributes.
fn tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag} ");
    xml.match_indices(&open).map(|(i, _)| &xml[i..i + xml[i..].find('>').unwrap()]).collect()
}

fn rels_xml(rels: &[(&str, &str)]) -> String {
    let body: String =
        rels.iter().map(|(id, target)| format!(r#"<Relationship Id="{id}" Type="{EXT}" Target="{target}" TargetMode="External"/>"#)).collect();
    format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#)
}

/// Opens a one-sheet workbook with this worksheet body and sheet relationships, and saves it.
fn save(body: &str, rels: &[(&str, &str)]) -> Vec<u8> {
    let rx = rels_xml(rels);
    let extra: Vec<(&str, &str)> = if rels.is_empty() { Vec::new() } else { vec![("xl/worksheets/_rels/sheet1.xml.rels", rx.as_str())] };
    let (wb, _) = read_xlsx(&minimal(body, &extra, "", "")).unwrap();
    write_xlsx(&wb).unwrap()
}

/// (sheet XML, sheet relationships XML or "") of a saved workbook.
fn sheet_of(bytes: &[u8]) -> (String, String) {
    let parts = unzip(bytes);
    let sheet = parts.get("xl/worksheets/sheet1.xml").expect("sheet part").clone();
    (sheet, parts.get("xl/worksheets/_rels/sheet1.xml.rels").cloned().unwrap_or_default())
}

/// Each `dataRef` of the saved sheet as (ref, name, sheet, external target its r:id resolves to).
fn data_refs(sheet: &str, rels: &str) -> Vec<(Option<String>, Option<String>, Option<String>, Option<String>)> {
    tags(sheet, "dataRef")
        .into_iter()
        .map(|d| {
            let target = attr(d, "r:id").map(|id| {
                let rel = tags(rels, "Relationship")
                    .into_iter()
                    .find(|r| attr(r, "Id").as_deref() == Some(id.as_str()))
                    .unwrap_or_else(|| panic!("dataRef r:id {id} has no relationship in {rels}"));
                assert_eq!(attr(rel, "Type").as_deref(), Some(EXT), "{rel}");
                assert_eq!(attr(rel, "TargetMode").as_deref(), Some("External"), "{rel}");
                attr(rel, "Target").unwrap()
            });
            (attr(d, "ref"), attr(d, "name"), attr(d, "sheet"), target)
        })
        .collect()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

const LINKED: &str = r#"<dataConsolidate function="average" leftLabels="1" topLabels="1" link="1"><dataRefs count="3"><dataRef ref="B2:C5" sheet="Jan" r:id="rId1"/><dataRef ref="E2:F5" sheet="Feb" r:id="rId2"/><dataRef ref="A1:B1" sheet="Data"/></dataRefs></dataConsolidate>"#;
const LINKED_RELS: [(&str, &str); 2] = [("rId1", "/books/north.xlsx"), ("rId2", "file:///C:/books/south%20region.xlsx")];

fn assert_linked(bytes: &[u8]) {
    let (sheet, rels) = sheet_of(bytes);
    let dc = tags(&sheet, "dataConsolidate");
    assert_eq!(dc.len(), 1, "{sheet}");
    for (name, value) in [("function", "average"), ("leftLabels", "1"), ("topLabels", "1"), ("link", "1")] {
        assert_eq!(attr(dc[0], name).as_deref(), Some(value), "dataConsolidate {name}: {sheet}");
    }
    assert_eq!(
        data_refs(&sheet, &rels),
        [
            (s("B2:C5"), None, s("Jan"), s("/books/north.xlsx")),
            (s("E2:F5"), None, s("Feb"), s("file:///C:/books/south%20region.xlsx")),
            (s("A1:B1"), None, s("Data"), None),
        ],
        "{sheet}\n{rels}"
    );
    // Schema position: after the cell data, before the page setup.
    let at = sheet.find("<dataConsolidate").unwrap();
    assert!(sheet.find("<sheetData").unwrap() < at, "{sheet}");
    assert!(at < sheet.find("<pageMargins").expect("pageMargins is written"), "{sheet}");
}

#[test]
fn data_consolidate_kept_with_sources_in_other_workbooks() {
    assert_linked(&save(&format!("{DATA}{LINKED}{MARGINS}"), &LINKED_RELS));
}

#[test]
fn data_consolidate_kept_on_a_second_save() {
    let first = save(&format!("{DATA}{LINKED}{MARGINS}"), &LINKED_RELS);
    let (wb, _) = read_xlsx(&first).unwrap();
    assert_linked(&write_xlsx(&wb).unwrap());
}

#[test]
fn data_consolidate_kept_with_sources_in_this_workbook() {
    let body = format!(
        r#"{DATA}<dataConsolidate><dataRefs count="2"><dataRef ref="A1:B1" sheet="Data"/><dataRef name="Totals" sheet="Data"/></dataRefs></dataConsolidate>{MARGINS}"#
    );
    let (sheet, rels) = sheet_of(&save(&body, &[]));
    assert!(sheet.contains("<dataConsolidate>"), "{sheet}");
    assert_eq!(data_refs(&sheet, &rels), [(s("A1:B1"), None, s("Data"), None), (None, s("Totals"), s("Data"), None)], "{sheet}");
    assert!(!rels.contains("externalLinkPath"), "{rels}");
}

#[test]
fn data_consolidate_kept_drops_an_id_that_names_no_relationship() {
    let body = format!(
        r#"{DATA}<dataConsolidate topLabels="1"><dataRefs count="1"><dataRef ref="B2:C5" sheet="Jan" r:id="rId9"/></dataRefs></dataConsolidate>{MARGINS}"#
    );
    let (sheet, rels) = sheet_of(&save(&body, &[("rId1", "/books/north.xlsx")]));
    assert_eq!(data_refs(&sheet, &rels), [(s("B2:C5"), None, s("Jan"), None)], "{sheet}");
}

#[test]
fn data_consolidate_kept_writes_nothing_for_a_sheet_without_it() {
    let (sheet, rels) = sheet_of(&save(&format!("{DATA}{MARGINS}"), &[]));
    assert!(!sheet.contains("dataConsolidate"), "{sheet}");
    assert!(!rels.contains("externalLinkPath"), "{rels}");
}
