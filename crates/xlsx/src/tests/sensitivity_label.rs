//! The sensitivity label part (`docMetadata/LabelInfo.xml`, package-root relationship
//! `classificationlabels`) is kept byte for byte on save. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_LABELS: &str = "http://schemas.microsoft.com/office/2020/02/relationships/classificationlabels";
const CT_LABELS: &str = "application/vnd.ms-office.classificationlabels+xml";
const LABEL_INFO: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?><clbl:labelList xmlns:clbl="http://schemas.microsoft.com/office/2020/mipLabelMetadata"><clbl:label id="{11111111-2222-3333-4444-555555555555}" enabled="1" method="Standard" siteId="{66666666-7777-8888-9999-000000000000}" contentBits="0" removed="0" /></clbl:labelList>"#;

fn package(labelled: bool) -> Vec<u8> {
    let label_ct = if labelled { format!(r#"<Override PartName="/docMetadata/LabelInfo.xml" ContentType="{CT_LABELS}"/>"#) } else { String::new() };
    let ct = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>{label_ct}</Types>"#
    );
    let label_rel =
        if labelled { format!(r#"<Relationship Id="rId2" Type="{REL_LABELS}" Target="docMetadata/LabelInfo.xml"/>"#) } else { String::new() };
    let root = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/>{label_rel}</Relationships>"#
    );
    let wb = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let sheet =
        format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#);
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ];
    if labelled {
        parts.push(("docMetadata/LabelInfo.xml", LABEL_INFO.as_bytes()));
    }
    make_zip(&parts)
}

fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            (f.name().to_string(), data)
        })
        .collect()
}

fn text(parts: &[(String, Vec<u8>)], name: &str) -> String {
    let (_, data) = parts.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} missing"));
    String::from_utf8(data.clone()).unwrap()
}

/// The target of the package-root relationship of the sensitivity label type, if any.
fn label_target(parts: &[(String, Vec<u8>)]) -> Option<String> {
    let rels = text(parts, "_rels/.rels");
    let at = rels.find(&format!(r#"Type="{REL_LABELS}""#))?;
    let start = rels[..at].rfind("<Relationship")?;
    let end = at + rels[at..].find("/>")?;
    let el = &rels[start..end];
    let t = el.find("Target=\"")? + 8;
    Some(el[t..t + el[t..].find('"')?].trim_start_matches('/').to_string())
}

fn assert_label_kept(out: &[u8]) {
    let parts = unzip(out);
    let target = label_target(&parts).expect("package-root classificationlabels relationship kept");
    assert_eq!(text(&parts, &target), LABEL_INFO, "label part kept byte for byte");
    let ct = text(&parts, "[Content_Types].xml");
    assert!(ct.contains(&format!(r#"PartName="/{target}" ContentType="{CT_LABELS}""#)), "content type override kept: {ct}");
}

#[test]
fn sensitivity_label_survives_open_and_save() {
    let (wb, _) = read_xlsx(&package(true)).unwrap();
    assert_label_kept(&write_xlsx(&wb).unwrap());
}

#[test]
fn sensitivity_label_survives_an_edit() {
    let (mut wb, _) = read_xlsx(&package(true)).unwrap();
    wb.sheet_mut(0).unwrap().set_value(gridcraft_core::CellRef::new(1, 1), gridcraft_core::Value::Number(2.0));
    assert_label_kept(&write_xlsx(&wb).unwrap());
}

#[test]
fn unlabelled_workbook_writes_no_label() {
    let (wb, _) = read_xlsx(&package(false)).unwrap();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(label_target(&parts).is_none());
    assert!(!parts.iter().any(|(n, _)| n.starts_with("docMetadata/")));
}
