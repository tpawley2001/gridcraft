//! A workbook with pictures in cells is saved with the rich value types part
//! (`xl/richData/rdRichValueTypes.xml`, [MS-XLSX] 2.3.6 rich data), as Microsoft 365 Excel saves it:
//! Excel keeps the part through its own open + save, so dropping it is a round-trip loss. The key
//! names and flags below are the ones Excel writes for a workbook with pictures in cells.

use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{CellPicture, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

const TYPES_PART: &str = "xl/richData/rdRichValueTypes.xml";
const TYPES_REL: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes";
const TYPES_CT: &str = "application/vnd.ms-excel.rdrichvaluetypes+xml";
const NS_RICH2: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2";

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
    let pixel = [0u8, 30, 120, 210];
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

/// A workbook with a picture in A1, A2, ... for each alt text given, saved.
fn saved(alts: &[&str]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap();
    for (i, alt) in alts.iter().enumerate() {
        let picture = Arc::new(CellPicture { data: png(), mime: "image/png".into(), alt: (*alt).into() });
        sheet.set_picture(CellRef::parse(&format!("A{}", i + 1)).unwrap(), picture);
    }
    write_xlsx(&wb).unwrap()
}

fn text(bytes: &[u8], part: &str) -> String {
    let mut package = Package::open(bytes).unwrap();
    String::from_utf8(package.read(part).unwrap().unwrap_or_else(|| panic!("{part} missing"))).unwrap()
}

/// The targets of the workbook relationships of the rich value types kind.
fn types_rel_targets(bytes: &[u8]) -> Vec<String> {
    let rels = text(bytes, "xl/_rels/workbook.xml.rels");
    rels.split("<Relationship ")
        .filter(|el| el.contains(&format!("Type=\"{TYPES_REL}\"")))
        .map(|el| {
            let t = el.split("Target=\"").nth(1).expect("Target");
            t[..t.find('"').unwrap()].to_string()
        })
        .collect()
}

fn types_parts(bytes: &[u8]) -> Vec<String> {
    Package::open(bytes).unwrap().names().into_iter().filter(|n| n.to_ascii_lowercase().ends_with("rdrichvaluetypes.xml")).collect()
}

fn assert_one_types_part(bytes: &[u8]) {
    assert_eq!(types_parts(bytes), [TYPES_PART], "exactly one rich value types part, spelled as Excel spells it");
    assert_eq!(types_rel_targets(bytes), ["richData/rdRichValueTypes.xml"], "exactly one workbook relationship to it");
    let types = text(bytes, "[Content_Types].xml");
    assert!(types.contains(&format!("PartName=\"/{TYPES_PART}\" ContentType=\"{TYPES_CT}\"")), "content type override: {types}");
}

#[test]
fn a_cell_picture_writes_the_rich_value_types_part() {
    assert_one_types_part(&saved(&[""]));
}

#[test]
fn pictures_with_and_without_alt_text_write_one_types_part() {
    assert_one_types_part(&saved(&["", "A red square", ""]));
}

#[test]
fn the_types_part_lists_excels_key_flags_in_order() {
    let bytes = saved(&["logo"]);
    let raw = text(&bytes, TYPES_PART);
    assert!(raw.starts_with("<?xml"), "XML declaration first: {raw}");
    assert!(raw.contains(&format!("xmlns=\"{NS_RICH2}\"")), "richdata2 namespace: {raw}");
    let root = Package::open(&bytes).unwrap().read_xml(TYPES_PART).unwrap().unwrap();
    assert_eq!(root.name, "rvTypesInfo");
    let keys: Vec<(String, Vec<(String, String)>)> = root
        .child("global")
        .expect("<global>")
        .child("keyFlags")
        .expect("<keyFlags>")
        .kids("key")
        .map(|k| {
            let flags = k.kids("flag").map(|f| (f.attr("name").unwrap().to_string(), f.attr("value").unwrap().to_string())).collect();
            (k.attr("name").unwrap().to_string(), flags)
        })
        .collect();
    let one = |flag: &str| (flag.to_string(), "1".to_string());
    let mut expected = vec![("_Self".to_string(), vec![one("ExcludeFromFile"), one("ExcludeFromCalcComparison")])];
    for name in
        ["_DisplayString", "_Flags", "_Format", "_SubLabel", "_Attribution", "_Icon", "_Display", "_CanonicalPropertyNames", "_ClassificationId"]
    {
        expected.push((name.to_string(), vec![one("ExcludeFromCalcComparison")]));
    }
    assert_eq!(keys, expected);
}

#[test]
fn open_and_save_keeps_exactly_one_types_part() {
    let first = saved(&["", "photo"]);
    let (wb, _) = read_xlsx(&first).unwrap();
    let second = write_xlsx(&wb).unwrap();
    assert_one_types_part(&second);
    // The pictures themselves still come back (guard: passes before and after the change).
    let (again, _) = read_xlsx(&second).unwrap();
    let sheet = again.sheet(0).unwrap();
    assert!(sheet.cell_pictures.contains_key(&CellRef::parse("A1").unwrap()));
    assert_eq!(sheet.cell_pictures.get(&CellRef::parse("A2").unwrap()).map(|p| p.alt.clone()), Some("photo".to_string()));
}

#[test]
fn a_workbook_without_cell_pictures_writes_no_rich_data_parts() {
    let bytes = write_xlsx(&Workbook::new()).unwrap();
    let names = Package::open(&bytes).unwrap().names();
    assert!(!names.iter().any(|n| n.starts_with("xl/richData/")), "no rich data parts: {names:?}");
    assert!(types_rel_targets(&bytes).is_empty());
}
