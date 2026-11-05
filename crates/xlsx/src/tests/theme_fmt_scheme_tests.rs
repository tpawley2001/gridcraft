//! The workbook theme's format scheme (fill, line, effect and background fill style lists), its
//! object defaults and the pictures its fills use survive open + save, as Excel keeps them.
//! Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};
use gridcraft_model::Workbook;

const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="jpeg" ContentType="image/jpeg"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
const WORKBOOK: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const WORKBOOK_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/></Relationships>"#;
const SHEET: &str = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#;
const THEME_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.jpeg"/></Relationships>"#;
const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0 theme background picture \xFF\xD9";

const CLR: &str = r#"<a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F497D"/></a:dk2><a:lt2><a:srgbClr val="EEECE1"/></a:lt2><a:accent1><a:srgbClr val="4F81BD"/></a:accent1><a:accent2><a:srgbClr val="C0504D"/></a:accent2><a:accent3><a:srgbClr val="9BBB59"/></a:accent3><a:accent4><a:srgbClr val="8064A2"/></a:accent4><a:accent5><a:srgbClr val="4BACC6"/></a:accent5><a:accent6><a:srgbClr val="F79646"/></a:accent6><a:hlink><a:srgbClr val="0000FF"/></a:hlink><a:folHlink><a:srgbClr val="800080"/></a:folHlink></a:clrScheme>"#;
const FONTS: &str = r#"<a:fontScheme name="Office"><a:majorFont><a:latin typeface="Cambria"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>"#;
const FMT: &str = r#"<a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="50000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="80000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="16200000" scaled="1"/></a:gradFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="25400"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="38100"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="40000" dist="23000" dir="5400000" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></a:blipFill></a:bgFillStyleLst></a:fmtScheme>"#;
const OBJECT_DEFAULTS: &str = r#"<a:objectDefaults><a:spDef><a:spPr/><a:bodyPr/><a:lstStyle/><a:style><a:lnRef idx="1"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="2"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="3"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></a:style></a:spDef></a:objectDefaults>"#;

fn theme_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" name="Office Theme"><a:themeElements>{CLR}{FONTS}{FMT}</a:themeElements>{OBJECT_DEFAULTS}<a:extraClrSchemeLst/></a:theme>"#
    )
}

fn package() -> Vec<u8> {
    let theme = theme_xml();
    make_zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", WORKBOOK.as_bytes()),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.as_bytes()),
        ("xl/worksheets/sheet1.xml", SHEET.as_bytes()),
        ("xl/theme/theme1.xml", theme.as_bytes()),
        ("xl/theme/_rels/theme1.xml.rels", THEME_RELS.as_bytes()),
        ("xl/media/image1.jpeg", JPEG),
    ])
}

fn part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = zip.by_name(name).ok()?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).unwrap();
    Some(out)
}

fn text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(part(bytes, name).unwrap_or_else(|| panic!("no part {name}"))).unwrap()
}

/// The workbook's theme part name in a saved package (from the workbook relationships).
fn theme_part(bytes: &[u8]) -> String {
    let rels = text(bytes, "xl/_rels/workbook.xml.rels");
    let at = rels.find("relationships/theme\"").or_else(|| rels.find("relationships/theme'")).expect("theme relationship");
    let rel_start = rels[..at].rfind("<Relationship").unwrap();
    let rel = &rels[rel_start..rel_start + rels[rel_start..].find("/>").unwrap()];
    let target = rel.split("Target=\"").nth(1).unwrap().split('"').next().unwrap();
    format!("xl/{}", target.trim_start_matches('/').trim_start_matches("xl/"))
}

fn rels_name(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap();
    format!("{dir}/_rels/{file}.rels")
}

fn resolve(base_part: &str, target: &str) -> String {
    let mut segs: Vec<&str> = base_part.rsplit_once('/').unwrap().0.split('/').collect();
    for s in target.split('/') {
        match s {
            ".." => {
                segs.pop();
            }
            "." | "" => {}
            s => segs.push(s),
        }
    }
    segs.join("/")
}

fn round_trip(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}

#[test]
fn the_format_scheme_and_object_defaults_survive_save() {
    let out = round_trip(&package());
    let theme = text(&out, &theme_part(&out));
    assert!(theme.contains(r#"<a:fmtScheme name="Office">"#), "{theme}");
    assert!(!theme.contains(r#"<a:fmtScheme name="Craft">"#), "{theme}");
    assert!(theme.contains("<a:gradFill"), "{theme}");
    assert!(theme.contains("<a:outerShdw"), "{theme}");
    assert!(theme.contains("<a:spDef"), "{theme}");
}

#[test]
fn the_picture_a_theme_fill_uses_survives_save() {
    let out = round_trip(&package());
    let theme_name = theme_part(&out);
    let theme = text(&out, &theme_name);
    let fill = &theme[theme.find("<a:blipFill").expect("blipFill kept")..];
    let id = fill.split("r:embed=\"").nth(1).expect("r:embed kept").split('"').next().unwrap().to_string();
    assert!(theme.contains("xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\""), "{theme}");
    let rels = text(&out, &rels_name(&theme_name));
    let rel_start = rels.find(&format!("Id=\"{id}\"")).unwrap_or_else(|| panic!("no relationship {id} in {rels}"));
    let rel_open = rels[..rel_start].rfind("<Relationship").unwrap();
    let rel = &rels[rel_open..rel_open + rels[rel_open..].find("/>").unwrap()];
    assert!(rel.contains("relationships/image\""), "{rel}");
    let target = rel.split("Target=\"").nth(1).unwrap().split('"').next().unwrap();
    let media = if target.starts_with('/') { target.trim_start_matches('/').to_string() } else { resolve(&theme_name, target) };
    assert_eq!(part(&out, &media).unwrap_or_else(|| panic!("no part {media}")), JPEG);
}

#[test]
fn a_new_workbook_still_writes_the_generated_theme() {
    let out = write_xlsx(&Workbook::new()).unwrap();
    let theme_name = theme_part(&out);
    let theme = text(&out, &theme_name);
    assert!(theme.contains(r#"<a:fmtScheme name="Craft">"#), "{theme}");
    assert!(part(&out, &rels_name(&theme_name)).is_none());
}

#[test]
fn an_edited_theme_colour_is_written_with_the_kept_format_scheme() {
    let (mut wb, _) = read_xlsx(&package()).unwrap();
    wb.theme.colors[4] = 0xFF0000;
    let out = write_xlsx(&wb).unwrap();
    let theme = text(&out, &theme_part(&out));
    assert!(theme.contains(r#"<a:accent1><a:srgbClr val="FF0000"/></a:accent1>"#), "{theme}");
    assert!(theme.contains(r#"<a:fmtScheme name="Office">"#), "{theme}");
}
