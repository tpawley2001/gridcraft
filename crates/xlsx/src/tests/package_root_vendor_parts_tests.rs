//! The Dell Data Protection parts a workbook carries at the package root (`xenFile`,
//! `metadataFile`, `hmacFile` relationships in `_rels/.rels`) are kept on save, byte for byte, as
//! Excel keeps them. All bytes here are made up.
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx, xml};

const DELL: &str = "http://schemas.dell.com/ddp/2016/relationships/";
const XEN: &[u8] = &[0x00, 0xFF, 0xFE, 0x80, b'<', b'&', 0x00, 0x1F, 0xC3, 0x28, 0x0D, 0x0A];
const METADATA: &str =
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<items version=\"1\">\n  <item><key>sample</key><value>false</value></item>\n</items>";
const HMAC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<hmacs version=\"2\" key=\"k\"><hmac file=\"content\" type=\"sha256\">AAAA</hmac></hmacs>";

fn package(xen_by_default: bool) -> Vec<u8> {
    let xen_type = if xen_by_default {
        r#"<Default Extension="xen" ContentType="application/octet-stream"/>"#
    } else {
        r#"<Override PartName="/ddp/ddpfile.xen" ContentType="application/octet-stream"/>"#
    };
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{xen_type}<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#
    );
    let root = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/><Relationship Id="rDellEncryptedDoc" Type="{DELL}xenFile" Target="ddp/ddpfile.xen"/><Relationship Id="rDellMetadataFile" Type="{DELL}metadataFile" Target="docProps/metadata.xml"/><Relationship Id="rDellHmacFile" Type="{DELL}hmacFile" Target="docProps/hmac.xml"/></Relationships>"#
    );
    let wb = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let wb_rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let sheet = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>7</v></c></row></sheetData></worksheet>"#;
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("ddp/ddpfile.xen", XEN),
        ("docProps/metadata.xml", METADATA.as_bytes()),
        ("docProps/hmac.xml", HMAC.as_bytes()),
    ])
}

fn part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).ok()?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).unwrap();
    Some(out)
}

/// The target (without a leading `/`) of the package-root relationship of type `ty`.
fn root_target(bytes: &[u8], ty: &str) -> Option<String> {
    let rels = xml::parse(&part(bytes, "_rels/.rels").unwrap()).unwrap();
    let rel = rels.kids("Relationship").find(|r| r.attr("Type") == Some(ty))?;
    assert_ne!(rel.attr("TargetMode"), Some("External"));
    Some(rel.attr("Target").unwrap().trim_start_matches('/').to_string())
}

/// The content type the package declares for `name` (an Override, else the extension's Default).
fn content_type(bytes: &[u8], name: &str) -> Option<String> {
    let types = xml::parse(&part(bytes, "[Content_Types].xml").unwrap()).unwrap();
    let part_name = format!("/{name}");
    if let Some(o) = types.kids("Override").find(|o| o.attr("PartName") == Some(part_name.as_str())) {
        return o.attr("ContentType").map(str::to_string);
    }
    let ext = name.rsplit('.').next().unwrap();
    types
        .kids("Default")
        .find(|d| d.attr("Extension").is_some_and(|e| e.eq_ignore_ascii_case(ext)))
        .and_then(|d| d.attr("ContentType").map(str::to_string))
}

fn assert_kept(out: &[u8]) {
    assert_eq!(part(out, "ddp/ddpfile.xen").as_deref(), Some(XEN));
    assert_eq!(part(out, "docProps/metadata.xml").as_deref(), Some(METADATA.as_bytes()));
    assert_eq!(part(out, "docProps/hmac.xml").as_deref(), Some(HMAC.as_bytes()));
    assert_eq!(root_target(out, &format!("{DELL}xenFile")).as_deref(), Some("ddp/ddpfile.xen"));
    assert_eq!(root_target(out, &format!("{DELL}metadataFile")).as_deref(), Some("docProps/metadata.xml"));
    assert_eq!(root_target(out, &format!("{DELL}hmacFile")).as_deref(), Some("docProps/hmac.xml"));
    assert_eq!(content_type(out, "ddp/ddpfile.xen").as_deref(), Some("application/octet-stream"));
    assert_eq!(content_type(out, "docProps/metadata.xml").as_deref(), Some("application/xml"));
    assert_eq!(content_type(out, "docProps/hmac.xml").as_deref(), Some("application/xml"));
}

#[test]
fn dell_protection_parts_survive_open_and_save() {
    for xen_by_default in [true, false] {
        let (wb, _) = read_xlsx(&package(xen_by_default)).unwrap();
        assert_kept(&write_xlsx(&wb).unwrap());
    }
}

#[test]
fn dell_protection_parts_survive_a_second_round_trip() {
    let (wb, _) = read_xlsx(&package(true)).unwrap();
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_kept(&write_xlsx(&again).unwrap());
}

#[test]
fn another_vendors_metadata_file_relationship_is_not_taken_for_dells() {
    let pkg = package(true);
    let root = String::from_utf8(part(&pkg, "_rels/.rels").unwrap()).unwrap().replace(DELL, "http://example.com/other/2016/relationships/");
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for name in [
        "[Content_Types].xml",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
        "ddp/ddpfile.xen",
        "docProps/metadata.xml",
        "docProps/hmac.xml",
    ] {
        parts.push((name.to_string(), part(&pkg, name).unwrap()));
    }
    parts.push(("_rels/.rels".to_string(), root.into_bytes()));
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    let (wb, _) = read_xlsx(&make_zip(&refs)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    // Unknown root relationships stay dropped, and the workbook still saves and reads back.
    assert_eq!(root_target(&out, "http://example.com/other/2016/relationships/metadataFile"), None);
    assert_eq!(part(&out, "ddp/ddpfile.xen"), None);
    read_xlsx(&out).unwrap();
}

#[test]
fn a_dell_relationship_naming_a_missing_part_does_not_break_the_save() {
    let pkg = package(true);
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for name in
        ["[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels", "xl/worksheets/sheet1.xml", "docProps/metadata.xml"]
    {
        parts.push((name.to_string(), part(&pkg, name).unwrap()));
    }
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    let (wb, _) = read_xlsx(&make_zip(&refs)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_eq!(part(&out, "docProps/metadata.xml").as_deref(), Some(METADATA.as_bytes()));
    assert_eq!(root_target(&out, &format!("{DELL}xenFile")), None);
    assert_eq!(root_target(&out, &format!("{DELL}hmacFile")), None);
    read_xlsx(&out).unwrap();
}
