//! The volatile dependencies part (`xl/volatileDependencies.xml`, ECMA-376 Part 1 §18.17, reached
//! from the workbook by a `volatileDependencies` relationship) is kept byte for byte on save, as
//! Excel keeps it. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_VOL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/volatileDependencies";
const CT_VOL: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.volatileDependencies+xml";
const VOL_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<volTypes xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><volType type="realTimeData"><main first="my.server"><tp t="e"><v>#N/A</v><stp/><stp>1</stp><tr r="A2" s="1"/></tp></main></volType></volTypes>"#;

fn package(volatile: bool) -> Vec<u8> {
    let vol_ct = if volatile { format!(r#"<Override PartName="/xl/volatileDependencies.xml" ContentType="{CT_VOL}"/>"#) } else { String::new() };
    let ct = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>{vol_ct}</Types>"#
    );
    let root = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let wb = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let vol_rel = if volatile { format!(r#"<Relationship Id="rId2" Type="{REL_VOL}" Target="volatileDependencies.xml"/>"#) } else { String::new() };
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/>{vol_rel}</Relationships>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row><row r="2"><c r="A2" t="e"><f>RTD("my.server",,"1")</f><v>#N/A</v></c></row></sheetData></worksheet>"#
    );
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ];
    if volatile {
        parts.push(("xl/volatileDependencies.xml", VOL_TYPES.as_bytes()));
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

/// The part name the workbook's `volatileDependencies` relationship points at, if any.
fn volatile_target(parts: &[(String, Vec<u8>)]) -> Option<String> {
    let rels = text(parts, "xl/_rels/workbook.xml.rels");
    let at = rels.find(&format!(r#"Type="{REL_VOL}""#))?;
    let start = rels[..at].rfind("<Relationship")?;
    let end = at + rels[at..].find("/>")?;
    let el = &rels[start..end];
    let t = el.find("Target=\"")? + 8;
    let target = &el[t..t + el[t..].find('"')?];
    Some(match target.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("xl/{target}"),
    })
}

fn assert_volatile_kept(out: &[u8]) {
    let parts = unzip(out);
    let target = volatile_target(&parts).expect("workbook volatileDependencies relationship kept");
    assert_eq!(text(&parts, &target), VOL_TYPES, "volatile dependencies part kept byte for byte");
    let ct = text(&parts, "[Content_Types].xml");
    assert!(ct.contains(&format!(r#"PartName="/{target}" ContentType="{CT_VOL}""#)), "content type override kept: {ct}");
}

#[test]
fn volatile_dependencies_survive_open_and_save() {
    let (wb, _) = read_xlsx(&package(true)).unwrap();
    assert_volatile_kept(&write_xlsx(&wb).unwrap());
}

#[test]
fn volatile_dependencies_survive_an_edit() {
    let (mut wb, _) = read_xlsx(&package(true)).unwrap();
    wb.sheet_mut(0).unwrap().set_value(gridcraft_core::CellRef::new(2, 0), gridcraft_core::Value::Number(5.0));
    assert_volatile_kept(&write_xlsx(&wb).unwrap());
}

#[test]
fn workbook_without_volatile_dependencies_gains_none() {
    let (wb, _) = read_xlsx(&package(false)).unwrap();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(volatile_target(&parts).is_none());
    assert!(!parts.iter().any(|(n, _)| n.contains("volatileDependencies")));
}
