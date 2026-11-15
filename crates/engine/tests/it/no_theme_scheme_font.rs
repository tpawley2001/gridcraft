//! A package with no theme part: fonts linked to the theme's body face (`<scheme val="minor"/>`,
//! ECMA-376 Part 1 §18.8.35) take the spreadsheet application's built-in default body face, Aptos
//! Narrow, not the face written in `<name>`. Measured in Excel: 141 default-height rows print 44 to a
//! Letter page (4 pages) in Aptos Narrow, 47 to a page (3 pages) in plain Calibri.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use serde_json::json;

const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OD_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// A one-sheet workbook with no theme part: 141 rows (A = the row number), Letter portrait,
/// margins 0.7/0.7/0.75/0.75, header 0.3 with a centred header line. `scheme` links the only
/// font to the theme's body face.
fn package(scheme: bool) -> Vec<u8> {
    let link = if scheme { r#"<scheme val="minor"/>"# } else { "" };
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet {NS}><fonts count="1"><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/>{link}</font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#
    );
    let rows: String = (1..=141).map(|r| format!(r#"<row r="{r}"><c r="A{r}"><v>{r}</v></c></row>"#)).collect();
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet {NS}><sheetFormatPr defaultRowHeight="15"/><sheetData>{rows}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait" scale="100"/><headerFooter><oddHeader>&amp;C&amp;A p&amp;P</oddHeader></headerFooter></worksheet>"#
    );
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="{CT}.styles+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/></Types>"#
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="{PKG_R}"><Relationship Id="rId1" Type="{OD_R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            ),
        ),
        ("xl/workbook.xml", format!(r#"<?xml version="1.0"?><workbook {NS}><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#)),
        (
            "xl/_rels/workbook.xml.rels",
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="{PKG_R}"><Relationship Id="rId1" Type="{OD_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{OD_R}/styles" Target="styles.xml"/></Relationships>"#
            ),
        ),
        ("xl/styles.xml", styles),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "no-theme.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn pages(s: &mut Session) -> u64 {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    r["pages"].as_u64().unwrap()
}

#[test]
fn scheme_linked_font_without_a_theme_prints_in_the_default_body_face() {
    // Aptos Narrow 11: 44 rows a page, so 141 rows take 4 pages (Excel: 1-44, 45-88, 89-132, 133-141).
    assert_eq!(pages(&mut open(&package(true))), 4);
}

#[test]
fn unlinked_font_without_a_theme_keeps_its_written_face() {
    // Plain Calibri 11: 47 rows a page, 3 pages.
    assert_eq!(pages(&mut open(&package(false))), 3);
}

#[test]
fn saved_and_reopened_it_keeps_the_default_body_face() {
    let s = open(&package(true));
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut styles = String::new();
    zip.by_name("xl/styles.xml").unwrap().read_to_string(&mut styles).unwrap();
    let fonts = styles.split("<fonts").nth(1).unwrap().split("</fonts>").next().unwrap();
    let first = fonts.split("<font>").nth(1).unwrap();
    assert!(first.contains(r#"<name val="Aptos Narrow"/>"#), "{fonts}");
    assert_eq!(pages(&mut open(&bytes)), 4);
}
