//! Excel refuses ("We can't open this file") a workbook with pictures in cells unless the rich-value
//! package is spelled the way Excel writes it ([MS-XLSX] 2.3.6 rich data parts, ECMA-376 Part 1
//! §18.9 metadata). Expected spellings are those of Microsoft 365 Excel's own save of a workbook
//! with pictures in cells. Related upstream: #1571, #2339.

use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{CellPicture, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

fn at(value: &str) -> CellRef {
    CellRef::parse(value).unwrap()
}

/// An original 1x1 RGB PNG: stored DEFLATE block, Adler-32 and PNG CRC-32 computed here.
fn png() -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8], body: &[u8]) {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        let mut crc = !0u32;
        for &byte in kind.iter().chain(body) {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        out.extend_from_slice(&(!crc).to_be_bytes());
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut png, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
    let pixel = [0u8, 200, 40, 40];
    let mut zlib = vec![0x78, 0x01, 1, 4, 0, 0xfb, 0xff];
    zlib.extend_from_slice(&pixel);
    let (mut a, mut b) = (1u32, 0u32);
    for n in pixel {
        a += u32::from(n);
        b += a;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn picture(alt: &str) -> Arc<CellPicture> {
    Arc::new(CellPicture { data: png(), mime: "image/png".into(), alt: alt.into() })
}

/// A workbook with a picture in A1 for each alt text given (A1, A2, ...), saved.
fn saved(alts: &[&str]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap();
    for (i, alt) in alts.iter().enumerate() {
        sheet.set_picture(at(&format!("A{}", i + 1)), picture(alt));
    }
    write_xlsx(&wb).unwrap()
}

fn text(package: &mut Package, part: &str) -> String {
    String::from_utf8(package.read(part).unwrap().unwrap_or_else(|| panic!("{part} missing"))).unwrap()
}

/// The ContentType of the Override for `part_name`, from `[Content_Types].xml`.
fn override_type(bytes: &[u8], part_name: &str) -> String {
    let mut package = Package::open(bytes).unwrap();
    let types = text(&mut package, "[Content_Types].xml");
    let needle = format!("PartName=\"{part_name}\"");
    let start = types.find(&needle).unwrap_or_else(|| panic!("no Override for {part_name}: {types}"));
    let tag_start = types[..start].rfind('<').unwrap();
    let tag = &types[tag_start..tag_start + types[tag_start..].find('>').unwrap()];
    let ct = tag.split("ContentType=\"").nth(1).unwrap();
    ct[..ct.find('"').unwrap()].to_string()
}

/// (key names in order, values of each rv) per structure / rv, as written.
fn layout(bytes: &[u8]) -> (Vec<Vec<(String, String)>>, Vec<(u32, Vec<String>)>) {
    let mut package = Package::open(bytes).unwrap();
    let s = package.read_xml("xl/richData/rdrichvaluestructure.xml").unwrap().unwrap();
    assert_eq!(s.attr_u32("count"), Some(s.kids("s").count() as u32), "rvStructures count");
    let structures = s
        .kids("s")
        .map(|st| {
            assert_eq!(st.attr("t"), Some("_localImage"));
            st.kids("k").map(|k| (k.attr("n").unwrap().to_string(), k.attr("t").unwrap().to_string())).collect()
        })
        .collect();
    let d = package.read_xml("xl/richData/rdrichvalue.xml").unwrap().unwrap();
    let values = d.kids("rv").map(|rv| (rv.attr_u32("s").unwrap(), rv.kids("v").map(|v| v.text.clone()).collect())).collect();
    (structures, values)
}

fn keys(names: &[&str]) -> Vec<(String, String)> {
    names.iter().map(|n| (n.to_string(), if *n == "Text" { "s" } else { "i" }.to_string())).collect()
}

#[test]
fn rich_value_parts_use_excels_lower_case_content_types() {
    let bytes = saved(&[""]);
    assert_eq!(override_type(&bytes, "/xl/richData/rdrichvalue.xml"), "application/vnd.ms-excel.rdrichvalue+xml");
    assert_eq!(override_type(&bytes, "/xl/richData/rdrichvaluestructure.xml"), "application/vnd.ms-excel.rdrichvaluestructure+xml");
    assert_eq!(override_type(&bytes, "/xl/richData/richValueRel.xml"), "application/vnd.ms-excel.richvaluerel+xml");
}

#[test]
fn xlrichvalue_metadata_type_has_coerce_and_a_lower_case_extension_uri() {
    let bytes = saved(&["", "alt"]);
    let mut package = Package::open(&bytes).unwrap();
    let metadata = package.read_xml("xl/metadata.xml").unwrap().unwrap();
    let ty = metadata.child("metadataTypes").unwrap().kids("metadataType").find(|t| t.attr("name") == Some("XLRICHVALUE")).unwrap();
    for (attr, value) in [
        ("minSupportedVersion", "120000"),
        ("copy", "1"),
        ("pasteAll", "1"),
        ("pasteValues", "1"),
        ("merge", "1"),
        ("splitFirst", "1"),
        ("rowColShift", "1"),
        ("clearFormats", "1"),
        ("clearComments", "1"),
        ("assign", "1"),
        ("coerce", "1"),
    ] {
        assert_eq!(ty.attr(attr), Some(value), "XLRICHVALUE @{attr}");
    }
    assert_eq!(ty.attr("cellMeta"), None, "Excel writes no cellMeta on XLRICHVALUE");
    let future = metadata.kids("futureMetadata").find(|m| m.attr("name") == Some("XLRICHVALUE")).unwrap();
    let uris: Vec<_> = future.kids("bk").map(|bk| bk.path(&["extLst", "ext"]).unwrap().attr("uri").unwrap().to_string()).collect();
    assert_eq!(uris, vec!["{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}"; 2]);
}

#[test]
fn pictures_without_alt_text_use_a_structure_without_a_text_key() {
    let (structures, values) = layout(&saved(&["", ""]));
    assert_eq!(structures, vec![keys(&["CalcOrigin", "_rvRel:LocalImageIdentifier"])]);
    assert_eq!(values, vec![(0, vec!["5".to_string(), "0".to_string()]), (0, vec!["5".to_string(), "1".to_string()])]);
}

#[test]
fn mixed_alt_text_writes_one_structure_per_key_set_like_excel() {
    let (structures, values) = layout(&saved(&["", "Some text"]));
    assert_eq!(structures, vec![keys(&["CalcOrigin", "_rvRel:LocalImageIdentifier"]), keys(&["CalcOrigin", "Text", "_rvRel:LocalImageIdentifier"])]);
    assert_eq!(values, vec![(0, vec!["5".to_string(), "0".to_string()]), (1, vec!["5".to_string(), "Some text".to_string(), "1".to_string()])]);
    // Every picture with alt text: only the Text structure is written.
    let (structures, values) = layout(&saved(&["a & <b>"]));
    assert_eq!(structures, vec![keys(&["CalcOrigin", "Text", "_rvRel:LocalImageIdentifier"])]);
    assert_eq!(values, vec![(0, vec!["5".to_string(), "a & <b>".to_string(), "0".to_string()])]);
}

/// Guard (passes before and after): whatever the structure layout, alt text and picture bytes
/// survive a save and reopen, and no warnings are raised.
#[test]
fn alt_text_and_bytes_round_trip_with_mixed_structures() {
    let alts = ["", "Some text", "", "Ω line\nbreak"];
    let (loaded, report) = read_xlsx(&saved(&alts)).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    for (i, alt) in alts.iter().enumerate() {
        let got = loaded.sheet(0).unwrap().cell_pictures.get(&at(&format!("A{}", i + 1))).unwrap();
        assert_eq!(got.alt, *alt);
        assert_eq!(got.data, png());
    }
}
