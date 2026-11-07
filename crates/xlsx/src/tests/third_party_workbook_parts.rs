//! Parts that other applications link from the workbook with their own relationship types (Google
//! Sheets' `http://customschemas.google.com/relationships/workbookmetadata`, a binary part at
//! `xl/metadata`) survive open + save byte for byte, with their relationship type and content type,
//! as Excel keeps them. Unknown types in the OOXML / Microsoft namespaces stay dropped. Packages are
//! built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const GOOGLE_TYPE: &str = "http://customschemas.google.com/relationships/workbookmetadata";
const OTHER_TYPE: &str = "urn:example:addin/relationships/state";
const MS_UNKNOWN_TYPE: &str = "http://schemas.microsoft.com/office/2099/relationships/notAThing";
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

/// Opaque binary content, as Google writes it (not XML, not UTF-8).
const GOOGLE_BYTES: &[u8] = &[0x78, 0x9C, 0x00, 0xFF, 0xE5, 0x10, 0x80, 0x42, 0x0A, 0x0D, 0x00, 0x7F, 0xC3, 0x28];
const OTHER_BYTES: &[u8] = b"<state xmlns=\"urn:example:addin\">kept</state>";
const MS_BYTES: &[u8] = b"<x/>";

fn package(with_ms_unknown: bool) -> Vec<u8> {
    let mut ct = String::from(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/metadata" ContentType="application/binary"/><Override PartName="/xl/addin/state.xml" ContentType="application/vnd.example.addin-state+xml"/>"#,
    );
    if with_ms_unknown {
        ct.push_str(r#"<Override PartName="/xl/notathing.xml" ContentType="application/vnd.ms-notathing+xml"/>"#);
    }
    ct.push_str("</Types>");
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let mut wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId22" Type="{GOOGLE_TYPE}" Target="metadata"/><Relationship Id="rId23" Type="{OTHER_TYPE}" Target="addin/state.xml"/>"#
    );
    if with_ms_unknown {
        wb_rels.push_str(&format!(r#"<Relationship Id="rId24" Type="{MS_UNKNOWN_TYPE}" Target="notathing.xml"/>"#));
    }
    wb_rels.push_str("</Relationships>");
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>7</v></c></row></sheetData></worksheet>"#
    );
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/metadata", GOOGLE_BYTES),
        ("xl/addin/state.xml", OTHER_BYTES),
    ];
    if with_ms_unknown {
        parts.push(("xl/notathing.xml", MS_BYTES));
    }
    make_zip(&parts)
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

/// Targets (resolved to part names) of the workbook relationships of type `rel_type`.
fn workbook_targets(p: &HashMap<String, Vec<u8>>, rel_type: &str) -> Vec<String> {
    let rels = text(p, "xl/_rels/workbook.xml.rels");
    rels.split("<Relationship ")
        .skip(1)
        .filter(|r| r.contains(&format!("Type=\"{rel_type}\"")))
        .map(|r| {
            let target = between(r, "Target=\"", "\"").unwrap();
            match target.strip_prefix('/') {
                Some(abs) => abs.to_string(),
                None => format!("xl/{target}"),
            }
        })
        .collect()
}

/// The content type the package gives `part` (Override first, then Default by extension).
fn content_type(p: &HashMap<String, Vec<u8>>, part: &str) -> Option<String> {
    let ct = text(p, "[Content_Types].xml");
    if let Some(i) = ct.find(&format!("PartName=\"/{part}\"")) {
        let start = ct[..i].rfind('<')?;
        return between(&ct[start..], "ContentType=\"", "\"");
    }
    let file = part.rsplit('/').next()?;
    let ext = file.rsplit_once('.')?.1;
    let i = ct.find(&format!("Extension=\"{ext}\""))?;
    let start = ct[..i].rfind('<')?;
    between(&ct[start..], "ContentType=\"", "\"")
}

fn saved(with_ms_unknown: bool) -> HashMap<String, Vec<u8>> {
    let (wb, _) = read_xlsx(&package(with_ms_unknown)).unwrap();
    unpack(&write_xlsx(&wb).unwrap())
}

#[test]
fn google_workbook_metadata_part_is_kept_byte_for_byte() {
    let p = saved(false);
    let targets = workbook_targets(&p, GOOGLE_TYPE);
    assert_eq!(targets.len(), 1, "one {GOOGLE_TYPE} relationship: {targets:?}");
    let part = &targets[0];
    assert_eq!(p.get(part).map(Vec::as_slice), Some(GOOGLE_BYTES), "{part} bytes");
    assert_eq!(content_type(&p, part).as_deref(), Some("application/binary"), "{part} content type");
}

#[test]
fn other_third_party_workbook_part_is_kept_with_its_content_type() {
    let p = saved(false);
    let targets = workbook_targets(&p, OTHER_TYPE);
    assert_eq!(targets.len(), 1, "one {OTHER_TYPE} relationship: {targets:?}");
    let part = &targets[0];
    assert_eq!(p.get(part).map(Vec::as_slice), Some(OTHER_BYTES), "{part} bytes");
    assert_eq!(content_type(&p, part).as_deref(), Some("application/vnd.example.addin-state+xml"));
}

#[test]
fn kept_parts_survive_a_second_open_and_save() {
    let (wb, _) = read_xlsx(&package(false)).unwrap();
    let (wb2, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let p = unpack(&write_xlsx(&wb2).unwrap());
    let g = workbook_targets(&p, GOOGLE_TYPE);
    let o = workbook_targets(&p, OTHER_TYPE);
    assert_eq!((g.len(), o.len()), (1, 1), "{g:?} {o:?}");
    assert_eq!(p.get(&g[0]).map(Vec::as_slice), Some(GOOGLE_BYTES));
    assert_eq!(p.get(&o[0]).map(Vec::as_slice), Some(OTHER_BYTES));
}

#[test]
fn unknown_microsoft_namespace_type_is_still_dropped() {
    let p = saved(true);
    assert!(workbook_targets(&p, MS_UNKNOWN_TYPE).is_empty(), "{MS_UNKNOWN_TYPE} must not be kept");
    assert!(!p.contains_key("xl/notathing.xml"));
    // The third-party parts are still kept next to it.
    assert_eq!(workbook_targets(&p, GOOGLE_TYPE).len(), 1);
}

#[test]
fn cell_values_and_sheet_metadata_path_unaffected() {
    let p = saved(false);
    let (wb, _) = read_xlsx(&package(false)).unwrap();
    assert_eq!(wb.sheets.len(), 1);
    // Guard: the kept Google part never lands on the sheetMetadata part name.
    for part in workbook_targets(&p, GOOGLE_TYPE) {
        assert_ne!(part, "xl/metadata.xml");
    }
    assert!(text(&p, "xl/worksheets/sheet1.xml").contains("<v>7</v>"));
}
