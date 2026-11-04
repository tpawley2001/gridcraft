//! ODS packages without a `mimetype` entry (ODF 1.2 Part 3 §3.3 makes it optional): the
//! manifest's root `file-entry` media type decides. Packages assembled in Rust.

use gridcraft_core::{CellRef, Value};

use super::make_zip;
use crate::{Format, read_ods, sniff};

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const CONTENT: &[u8] = br#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet><table:table table:name="Data"><table:table-row><table:table-cell office:value-type="float" office:value="42"/></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#;

fn manifest(prefix: &str, media: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><{prefix}:manifest xmlns:{prefix}="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" {prefix}:version="1.2"><{prefix}:file-entry {prefix}:full-path="/" {prefix}:version="1.2" {prefix}:media-type="{media}"/><{prefix}:file-entry {prefix}:full-path="content.xml" {prefix}:media-type="text/xml"/></{prefix}:manifest>"#
    )
    .into_bytes()
}

#[test]
fn ods_without_mimetype_opens_when_the_manifest_root_is_a_spreadsheet() {
    for prefix in ["manifest", "m"] {
        let m = manifest(prefix, "application/vnd.oasis.opendocument.spreadsheet");
        let bytes = make_zip(&[("META-INF/manifest.xml", &m), ("content.xml", CONTENT)]);
        assert_eq!(sniff(&bytes), Format::Ods, "prefix {prefix}");
        let (wb, _) = read_ods(&bytes).unwrap();
        assert_eq!(wb.sheets[0].name, "Data");
        assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
    }
}

#[test]
fn ods_without_mimetype_needs_a_spreadsheet_manifest_and_content() {
    let text = manifest("manifest", "application/vnd.oasis.opendocument.text");
    let no_root = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/></manifest:manifest>"#;
    let wrong_ns = br#"<manifest:manifest xmlns:manifest="urn:example:not-the-manifest"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/></manifest:manifest>"#;
    let good = manifest("manifest", "application/vnd.oasis.opendocument.spreadsheet");
    let cases: [(&str, Vec<u8>); 5] = [
        ("text document", make_zip(&[("META-INF/manifest.xml", &text), ("content.xml", CONTENT)])),
        ("no root entry", make_zip(&[("META-INF/manifest.xml", no_root), ("content.xml", CONTENT)])),
        ("wrong namespace", make_zip(&[("META-INF/manifest.xml", wrong_ns), ("content.xml", CONTENT)])),
        ("no manifest", make_zip(&[("content.xml", CONTENT)])),
        ("no content", make_zip(&[("META-INF/manifest.xml", &good)])),
    ];
    for (what, bytes) in cases {
        assert_ne!(sniff(&bytes), Format::Ods, "{what}");
        assert!(read_ods(&bytes).is_err(), "{what}");
    }
}

#[test]
fn a_present_but_wrong_mimetype_is_still_refused_even_with_a_good_manifest() {
    let good = manifest("manifest", "application/vnd.oasis.opendocument.spreadsheet");
    for mime in [b"application/zip".as_slice(), b"application/vnd.oasis.opendocument.spreadsheet "] {
        let bytes = make_zip(&[("mimetype", mime), ("META-INF/manifest.xml", &good), ("content.xml", CONTENT)]);
        assert_ne!(sniff(&bytes), Format::Ods);
        assert!(read_ods(&bytes).is_err());
    }
    // The exact mimetype still works on its own, as before.
    let bytes = make_zip(&[("mimetype", MIME), ("content.xml", CONTENT)]);
    assert_eq!(sniff(&bytes), Format::Ods);
    assert!(read_ods(&bytes).is_ok());
}

#[test]
fn an_oversized_manifest_is_not_read_when_sniffing() {
    let mut huge = manifest("manifest", "application/vnd.oasis.opendocument.spreadsheet");
    let pad = format!("<!--{}-->", "x".repeat(1024 * 1024 + 16));
    huge.extend_from_slice(pad.as_bytes());
    let bytes = make_zip(&[("META-INF/manifest.xml", &huge), ("content.xml", CONTENT)]);
    assert_ne!(sniff(&bytes), Format::Ods);
}
