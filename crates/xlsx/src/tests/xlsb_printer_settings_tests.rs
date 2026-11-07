//! XLSB printer settings: `BrtPageSetup.szRelID` ([MS-XLSB] 2.4.722) names the sheet's printer settings
//! part. Like the `.xlsx` reader (`<pageSetup r:id>`), the part is kept byte for byte on Save As .xlsx.
use std::collections::BTreeMap;
use std::io::Read;

use super::make_zip;
use super::xlsb_print_tests::{record, worksheet};
use crate::{read_xlsb, write_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
const BOOK_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/></Relationships>"#;
const SHEET_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/printerSettings" Target="../printerSettings/printerSettings1.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"/><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/></Types>"#;

/// A stand-in DEVMODE blob: arbitrary bytes that must come back unchanged.
const DEVMODE: &[u8] = b"DEVMODE\0\x01\x04\x00\x05\xdc\x00\x94\x05\xff\xfe\x00\x01";

fn wide(text: Option<&str>) -> Vec<u8> {
    let Some(text) = text else { return u32::MAX.to_le_bytes().to_vec() };
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}

fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(Some("")));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    let mut sheet = 0u32.to_le_bytes().to_vec();
    sheet.extend(1u32.to_le_bytes());
    sheet.extend(wide(Some("rId1")));
    sheet.extend(wide(Some("Data")));
    out.extend(record(156, &sheet));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}

/// `BrtPageSetup` with Letter paper, 75% scale, landscape and the given `szRelID`.
fn page_setup(rel_id: Option<&str>) -> Vec<u8> {
    let mut out = 1u32.to_le_bytes().to_vec();
    for value in [75u32, 600, 600, 1, 1, 1, 1] {
        out.extend(value.to_le_bytes());
    }
    out.extend(0b0000_0010u16.to_le_bytes()); // fLandscape
    out.extend(wide(rel_id));
    record(478, &out)
}

fn package(rel_id: Option<&str>, with_sheet_rels: bool) -> Vec<u8> {
    let sheet = worksheet(&page_setup(rel_id));
    let book = book();
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", BOOK_RELS.as_bytes()),
        ("xl/worksheets/sheet1.bin", &sheet),
    ];
    if with_sheet_rels {
        parts.push(("xl/worksheets/_rels/sheet1.bin.rels", SHEET_RELS.as_bytes()));
        parts.push(("xl/printerSettings/printerSettings1.bin", DEVMODE));
    }
    make_zip(&parts)
}

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

fn text(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(parts.get(name).unwrap_or_else(|| panic!("no part {name}: {:?}", parts.keys())).clone()).unwrap()
}

fn printer_parts(parts: &BTreeMap<String, Vec<u8>>) -> Vec<(&String, &Vec<u8>)> {
    parts.iter().filter(|(name, _)| name.to_ascii_lowercase().contains("printersettings") && !name.ends_with(".rels")).collect()
}

#[test]
fn xlsb_printer_settings_part_is_kept_on_save_as_xlsx() {
    let (wb, _) = read_xlsb(&package(Some("rId7"), true)).unwrap();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    let kept = printer_parts(&parts);
    assert_eq!(kept.len(), 1, "{:?}", parts.keys());
    assert_eq!(kept[0].1.as_slice(), DEVMODE);
    let types = text(&parts, "[Content_Types].xml");
    assert!(types.contains("application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"), "{types}");
}

#[test]
fn xlsb_printer_settings_pagesetup_points_at_the_part() {
    let (wb, _) = read_xlsb(&package(Some("rId7"), true)).unwrap();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    let sheet = text(&parts, "xl/worksheets/sheet1.xml");
    let rels = text(&parts, "xl/worksheets/_rels/sheet1.xml.rels");
    let start = sheet.find("<pageSetup").expect("pageSetup written");
    let page_setup = &sheet[start..start + sheet[start..].find("/>").unwrap()];
    assert!(page_setup.contains("orientation=\"landscape\""), "{page_setup}");
    let id_at = page_setup.find("r:id=\"").expect("pageSetup carries r:id") + 6;
    let id = &page_setup[id_at..id_at + page_setup[id_at..].find('"').unwrap()];
    let rel_at = rels.find(&format!("Id=\"{id}\"")).unwrap_or_else(|| panic!("no relationship {id}: {rels}"));
    let rel_end = rel_at + rels[rel_at..].find("/>").unwrap();
    let rel = &rels[rels[..rel_at].rfind('<').unwrap()..rel_end];
    assert!(rel.contains("relationships/printerSettings\""), "{rel}");
    let target = rel.split("Target=\"").nth(1).unwrap().split('"').next().unwrap();
    let name = target.trim_start_matches("../");
    assert_eq!(parts.get(&format!("xl/{name}")).map(Vec::as_slice), Some(DEVMODE), "{target}");
}

#[test]
fn xlsb_dangling_printer_settings_id_warns_and_keeps_page_setup() {
    let (wb, report) = read_xlsb(&package(Some("rId7"), false)).unwrap();
    let print = &wb.sheet(0).unwrap().print;
    assert_eq!(print.scale, 75);
    assert_eq!(print.orientation, gridcraft_model::Orientation::Landscape);
    let warned: Vec<_> = report.warnings.iter().filter(|w| w.to_ascii_lowercase().contains("printer settings")).collect();
    assert_eq!(warned.len(), 1, "{:?}", report.warnings);
    assert!(warned[0].contains("Data"), "the warning names the sheet: {}", warned[0]);
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(printer_parts(&parts).is_empty(), "{:?}", parts.keys());
}

#[test]
fn xlsb_page_setup_without_printer_settings_writes_no_part() {
    let (wb, report) = read_xlsb(&package(None, false)).unwrap();
    assert!(!report.warnings.iter().any(|w| w.to_ascii_lowercase().contains("printer settings")), "{:?}", report.warnings);
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(printer_parts(&parts).is_empty(), "{:?}", parts.keys());
    let sheet = text(&parts, "xl/worksheets/sheet1.xml");
    assert!(!sheet.contains("r:id=\"") || !sheet[sheet.find("<pageSetup").unwrap_or(0)..].split("/>").next().unwrap().contains("r:id"));
}
