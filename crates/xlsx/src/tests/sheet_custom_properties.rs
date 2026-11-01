//! Worksheet custom properties (`<customProperties><customPr name r:id/>`, written by add-ins to
//! tag sheets) survive open + save with their `customProperty` parts. Packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CP_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.customProperty";
const PRINTER: &[u8] = &[0, 1, 2, 3, 250, 251, 252, 253, 9, 9, 9];

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.customProperty"/><Override PartName="/xl/printerSettings/printerSettings1.bin" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

/// A customProperty part as add-ins write it: UTF-16LE XML text.
fn prop_bytes(marker: &str) -> Vec<u8> {
    let text = format!("<?xml version=\"1.0\"?><ORB>{marker}</ORB>");
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Sheet "Data" with two custom properties (and printer settings, whose `.bin` part has another
/// content type); sheet "Other" with one.
fn package() -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{NS_R}/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup orientation="landscape" r:id="rId3"/><customProperties><customPr name="ORB_SHEETNAME" r:id="rId1"/><customPr name="TAG_TWO" r:id="rId2"/></customProperties></worksheet>"#
    );
    let sheet2 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>2</v></c></row></sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><customProperties><customPr name="ORB_SHEETNAME" r:id="rId1"/></customProperties></worksheet>"#
    );
    let s1_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/customProperty" Target="../customProperty1.bin"/><Relationship Id="rId2" Type="{NS_R}/customProperty" Target="../customProperty2.bin"/><Relationship Id="rId3" Type="{NS_R}/printerSettings" Target="../printerSettings/printerSettings1.bin"/></Relationships>"#
    );
    let s2_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/customProperty" Target="../customProperty3.bin"/></Relationships>"#
    );
    let (p1, p2, p3) = (prop_bytes("DATA-ONE"), prop_bytes("DATA-TWO"), prop_bytes("OTHER-ONE"));
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", s1_rels.as_bytes()),
        ("xl/worksheets/_rels/sheet2.xml.rels", s2_rels.as_bytes()),
        ("xl/customProperty1.bin", &p1),
        ("xl/customProperty2.bin", &p2),
        ("xl/customProperty3.bin", &p3),
        ("xl/printerSettings/printerSettings1.bin", PRINTER),
    ])
}

fn unpack(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = vec![];
        f.read_to_end(&mut v).unwrap();
        parts.insert(f.name().to_string(), v);
    }
    parts
}

fn text(p: &HashMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8_lossy(p.get(name).unwrap_or_else(|| panic!("no part {name}"))).into_owned()
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    Some(s[i..i + s[i..].find(b)?].to_string())
}

fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Relationship `id` of `part`: (type, resolved part name).
fn rel(p: &HashMap<String, Vec<u8>>, part: &str, id: &str) -> Option<(String, String)> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let rels = String::from_utf8_lossy(p.get(&format!("{dir}/_rels/{file}.rels"))?).into_owned();
    let i = rels.find(&format!("Id=\"{id}\""))?;
    let start = rels[..i].rfind('<')?;
    let r = &rels[start..start + rels[start..].find("/>")?];
    let target = between(r, "Target=\"", "\"")?;
    let full = match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => normalize(&format!("{dir}/{target}")),
    };
    Some((between(r, "Type=\"", "\"")?, full))
}

fn sheet_part(p: &HashMap<String, Vec<u8>>, name: &str) -> String {
    let wb = text(p, "xl/workbook.xml");
    let i = wb.find(&format!("name=\"{name}\"")).unwrap_or_else(|| panic!("no sheet {name}"));
    let rid = between(&wb[i..], "r:id=\"", "\"").unwrap();
    rel(p, "xl/workbook.xml", &rid).unwrap().1
}

/// The content type the package gives `part` (Override first, then Default by extension).
fn content_type(p: &HashMap<String, Vec<u8>>, part: &str) -> Option<String> {
    let ct = text(p, "[Content_Types].xml");
    if let Some(i) = ct.find(&format!("PartName=\"/{part}\"")) {
        let start = ct[..i].rfind('<')?;
        return between(&ct[start..], "ContentType=\"", "\"");
    }
    let ext = part.rsplit('.').next()?;
    let i = ct.find(&format!("Extension=\"{ext}\""))?;
    let start = ct[..i].rfind('<')?;
    between(&ct[start..], "ContentType=\"", "\"")
}

/// The custom properties written for sheet `name`: (property name, part bytes), in order.
fn written_props(p: &HashMap<String, Vec<u8>>, name: &str) -> Vec<(String, Vec<u8>)> {
    let sheet = sheet_part(p, name);
    let sx = text(p, &sheet);
    let Some(block) = between(&sx, "<customProperties>", "</customProperties>") else { return vec![] };
    let mut out = Vec::new();
    for (i, _) in block.match_indices("<customPr ") {
        let tag = &block[i..i + block[i..].find("/>").unwrap()];
        let pname = between(tag, "name=\"", "\"").unwrap();
        let rid = between(tag, "r:id=\"", "\"").unwrap();
        let (ty, part) = rel(p, &sheet, &rid).unwrap_or_else(|| panic!("{rid} does not resolve from {sheet}"));
        assert!(ty.ends_with("/customProperty"), "relationship type {ty}");
        assert_eq!(content_type(p, &part).as_deref(), Some(CP_TYPE), "content type of {part}");
        out.push((pname, p.get(&part).cloned().unwrap_or_else(|| panic!("no part {part}"))));
    }
    out
}

#[test]
fn custom_properties_survive_two_saves() {
    let want_data = vec![("ORB_SHEETNAME".to_string(), prop_bytes("DATA-ONE")), ("TAG_TWO".to_string(), prop_bytes("DATA-TWO"))];
    let want_other = vec![("ORB_SHEETNAME".to_string(), prop_bytes("OTHER-ONE"))];
    let (wb, _) = read_xlsx(&package()).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let p = unpack(&out);
    assert_eq!(written_props(&p, "Data"), want_data);
    assert_eq!(written_props(&p, "Other"), want_other);
    let (wb2, _) = read_xlsx(&out).unwrap();
    let p2 = unpack(&write_xlsx(&wb2).unwrap());
    assert_eq!(written_props(&p2, "Data"), want_data);
    assert_eq!(written_props(&p2, "Other"), want_other);
}

#[test]
fn custom_properties_are_in_schema_order() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let p = unpack(&write_xlsx(&wb).unwrap());
    let sx = text(&p, &sheet_part(&p, "Data"));
    let at = |tag: &str| sx.find(tag);
    let cp = at("<customProperties>").expect("customProperties not written");
    for before in ["<pageMargins", "<pageSetup"] {
        if let Some(i) = at(before) {
            assert!(i < cp, "{before} must come before customProperties: {sx}");
        }
    }
    for after in ["<drawing ", "<legacyDrawing ", "<tableParts", "<extLst"] {
        if let Some(i) = at(after) {
            assert!(cp < i, "customProperties must come before {after}: {sx}");
        }
    }
}

#[test]
fn printer_settings_keep_their_own_content_type() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let p = unpack(&write_xlsx(&wb).unwrap());
    let sheet = sheet_part(&p, "Data");
    let sx = text(&p, &sheet);
    let rid = between(&sx[sx.find("<pageSetup").expect("pageSetup lost")..], "r:id=\"", "\"").expect("printer settings link lost");
    let (_, part) = rel(&p, &sheet, &rid).unwrap();
    assert_eq!(content_type(&p, &part).as_deref(), Some("application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"));
}

#[test]
fn deleted_sheet_drops_its_custom_properties() {
    let (mut wb, _) = read_xlsx(&package()).unwrap();
    wb.sheets.remove(0);
    let p = unpack(&write_xlsx(&wb).unwrap());
    let data_one = prop_bytes("DATA-ONE");
    assert!(!p.values().any(|v| *v == data_one), "custom property of a deleted sheet was written");
    assert_eq!(written_props(&p, "Other"), vec![("ORB_SHEETNAME".to_string(), prop_bytes("OTHER-ONE"))]);
}
