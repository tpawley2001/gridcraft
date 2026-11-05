//! The Manager and Hyperlink base document properties: app.xml `Manager` / `HyperlinkBase`
//! (ECMA-376 Part 1 §22.2.2.17 / §22.2.2.14) and the .xls DocumentSummaryInformation PIDSI_MANAGER (14).

use std::collections::BTreeMap;
use std::io::{Read, Write};

use gridcraft_model::Workbook;

use super::binary_doc_props::{DOCUMENT, stream, text, xls};
use crate::{read_xls, read_xlsx, write_xlsx};

fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut m = BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        m.insert(f.name().to_string(), b);
    }
    m
}

fn rezip(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in parts {
        out.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn app_xml(bytes: &[u8]) -> String {
    String::from_utf8(unzip(bytes)["docProps/app.xml"].clone()).unwrap()
}

#[test]
fn xlsx_reads_manager_and_hyperlink_base() {
    let mut parts = unzip(&write_xlsx(&Workbook::new()).unwrap());
    parts.insert(
        "docProps/app.xml".into(),
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Microsoft Excel</Application><Manager>Ada Lovelace</Manager><Company>Example Co</Company><HyperlinkBase>http://example.com/docs/</HyperlinkBase></Properties>"#
            .to_vec(),
    );
    let (wb, _) = read_xlsx(&rezip(&parts)).unwrap();
    assert_eq!(wb.props.manager, "Ada Lovelace");
    assert_eq!(wb.props.hyperlink_base, "http://example.com/docs/");
    assert_eq!(wb.props.company, "Example Co");
}

#[test]
fn xlsx_writes_and_reopens_manager_and_hyperlink_base() {
    let mut wb = Workbook::new();
    wb.props.manager = "R&D <lead>".into();
    wb.props.hyperlink_base = "http://example.com/a?b=1&c=2".into();
    let bytes = write_xlsx(&wb).unwrap();
    let app = app_xml(&bytes);
    assert!(app.contains("<Manager>R&amp;D &lt;lead&gt;</Manager>"), "{app}");
    assert!(app.contains("<HyperlinkBase>http://example.com/a?b=1&amp;c=2</HyperlinkBase>"), "{app}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(back.props, wb.props);
}

#[test]
fn xlsx_empty_manager_and_hyperlink_base_write_nothing() {
    let mut wb = Workbook::new();
    wb.props.company = "Example Co".into();
    let app = app_xml(&write_xlsx(&wb).unwrap());
    assert!(!app.contains("Manager"), "{app}");
    assert!(!app.contains("HyperlinkBase"), "{app}");
    assert!(app.contains("<Company>Example Co</Company>"), "{app}");
}

#[test]
fn xls_manager_is_read_and_saved_as_xlsx() {
    let document = stream(
        DOCUMENT,
        &[(1, super::binary_doc_props::typed(2, &1252u16.to_le_bytes())), (14, text(0x1e, b"Grace Hopper\0")), (15, text(0x1e, b"Navy\0"))],
    );
    let (wb, _) = read_xls(&xls(None, Some(&document))).unwrap();
    assert_eq!(wb.props.manager, "Grace Hopper");
    assert_eq!(wb.props.company, "Navy");
    let (saved, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(saved.props.manager, "Grace Hopper");
}
