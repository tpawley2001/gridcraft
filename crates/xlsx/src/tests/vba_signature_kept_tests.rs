//! The digital signature of a VBA project (the parts related to `xl/vbaProject.bin`) is kept on
//! save while the project's bytes are unchanged, and dropped once they change.

use std::collections::HashMap;
use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const VBA: &str = "VBA PROJECT BYTES";
const VBA_REL: &str = r#"<Relationship Id="rId5" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/>"#;

/// (relationship type, content type, part name under `xl/`, bytes) of the three signature kinds.
const KINDS: [(&str, &str, &str, &str); 3] = [
    (
        "http://schemas.microsoft.com/office/2006/relationships/vbaProjectSignature",
        "application/vnd.ms-office.vbaProjectSignature",
        "vbaProjectSignature.bin",
        "LEGACY SIGNATURE BYTES",
    ),
    (
        "http://schemas.microsoft.com/office/2014/relationships/vbaProjectSignatureAgile",
        "application/vnd.ms-office.vbaProjectSignatureAgile",
        "vbaProjectSignatureAgile.bin",
        "AGILE SIGNATURE BYTES",
    ),
    (
        "http://schemas.microsoft.com/office/2020/07/relationships/vbaProjectSignatureV3",
        "application/vnd.ms-office.vbaProjectSignatureV3",
        "vbaProjectSignatureV3.bin",
        "V3 SIGNATURE BYTES",
    ),
];

fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut m = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        m.insert(f.name().to_string(), b);
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

/// A one-sheet macro workbook whose VBA project carries the first `n` signature kinds.
fn signed(n: usize) -> Vec<u8> {
    let rels: String = KINDS[..n]
        .iter()
        .enumerate()
        .map(|(i, (ty, _, part, _))| format!(r#"<Relationship Id="rId{}" Type="{ty}" Target="{part}"/>"#, i + 1))
        .collect();
    let rels =
        format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#);
    let names: Vec<String> = KINDS[..n].iter().map(|k| format!("xl/{}", k.2)).collect();
    let mut extra: Vec<(&str, &str)> = vec![("xl/vbaProject.bin", VBA), ("xl/_rels/vbaProject.bin.rels", rels.as_str())];
    for (name, kind) in names.iter().zip(&KINDS[..n]) {
        extra.push((name.as_str(), kind.3));
    }
    minimal("<sheetData/>", &extra, VBA_REL, "")
}

/// Checks the saved package keeps the VBA project and exactly the first `n` signature kinds.
fn assert_signatures(bytes: &[u8], n: usize) {
    let parts = unzip(bytes);
    assert_eq!(parts.get("xl/vbaProject.bin").map(Vec::as_slice), Some(VBA.as_bytes()), "the VBA project itself");
    let rels = String::from_utf8(parts.get("xl/_rels/vbaProject.bin.rels").cloned().unwrap_or_default()).unwrap();
    let ct = String::from_utf8(parts["[Content_Types].xml"].clone()).unwrap();
    let found = tags(&rels, "Relationship");
    assert_eq!(found.len(), n, "relationships of the VBA project: {rels}");
    for (ty, content_type, _, data) in &KINDS[..n] {
        let rel = found.iter().find(|r| attr(r, "Type").as_deref() == Some(*ty)).unwrap_or_else(|| panic!("no {ty} relationship in {rels}"));
        assert_ne!(attr(rel, "TargetMode").as_deref(), Some("External"), "{rel}");
        let target = attr(rel, "Target").unwrap();
        let part = if let Some(abs) = target.strip_prefix('/') { abs.to_string() } else { format!("xl/{target}") };
        assert_eq!(parts.get(&part).map(Vec::as_slice), Some(data.as_bytes()), "signature part {part}");
        let over = tags(&ct, "Override").into_iter().find(|o| attr(o, "PartName").as_deref() == Some(&format!("/{part}")));
        let over = over.unwrap_or_else(|| panic!("no content type Override for /{part}: {ct}"));
        assert_eq!(attr(over, "ContentType").as_deref(), Some(*content_type), "content type of /{part}");
    }
    if n == 0 {
        assert!(!parts.contains_key("xl/_rels/vbaProject.bin.rels"), "no relationships part without a signature");
        assert!(!parts.values().any(|d| d.windows(15).any(|w| w == b"SIGNATURE BYTES")), "a signature part was written");
    }
}

fn open(bytes: &[u8]) -> gridcraft_model::Workbook {
    let (wb, _) = read_xlsx(bytes).unwrap();
    assert_eq!(wb.vba_project.as_deref().map(Vec::as_slice), Some(VBA.as_bytes()), "fixture must read as a macro workbook");
    wb
}

#[test]
fn vba_signature_kept_legacy() {
    assert_signatures(&write_xlsx(&open(&signed(1))).unwrap(), 1);
}

#[test]
fn vba_signature_kept_legacy_agile_and_v3() {
    assert_signatures(&write_xlsx(&open(&signed(3))).unwrap(), 3);
}

#[test]
fn vba_signature_kept_on_a_second_save() {
    let first = write_xlsx(&open(&signed(3))).unwrap();
    assert_signatures(&write_xlsx(&open(&first)).unwrap(), 3);
}

#[test]
fn vba_signature_kept_unsigned_project_writes_no_signature() {
    // Guards the fixture and the checker: an unsigned project round-trips as before.
    let bytes = minimal("<sheetData/>", &[("xl/vbaProject.bin", VBA)], VBA_REL, "");
    assert_signatures(&write_xlsx(&open(&bytes)).unwrap(), 0);
}

#[test]
fn vba_signature_dropped_when_the_project_changes() {
    let mut wb = open(&signed(3));
    wb.vba_project = Some(b"OTHER VBA PROJECT".to_vec().into());
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert_eq!(parts.get("xl/vbaProject.bin").map(Vec::as_slice), Some(&b"OTHER VBA PROJECT"[..]));
    assert!(!parts.contains_key("xl/_rels/vbaProject.bin.rels"), "a changed project keeps no signature relationships");
    assert!(!parts.values().any(|d| d.windows(15).any(|w| w == b"SIGNATURE BYTES")), "a stale signature was written");
}

#[test]
fn vba_signature_dropped_when_the_project_is_removed() {
    let mut wb = open(&signed(1));
    wb.vba_project = None;
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(!parts.contains_key("xl/vbaProject.bin"));
    assert!(!parts.keys().any(|n| n.contains("vbaProjectSignature")), "{:?}", parts.keys());
    assert!(!parts.values().any(|d| d.windows(15).any(|w| w == b"SIGNATURE BYTES")), "a stale signature was written");
}
