//! A worksheet `<conditionalFormatting pivot="1">` (ECMA-376 Part 1, 18.3.1.18) belongs to a PivotTable, which links to
//! it by rule priority. Excel repairs a file whose PivotTable lists a conditional format when the sheet's matching rule
//! lost `pivot="1"`, so the flag must survive save. A rule without the flag must not gain it.

use std::io::{Read, Write};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn xlsx() -> Vec<u8> {
    let rows: String = (1..=7).map(|r| format!(r#"<row r="{r}"><c r="E{r}"><v>{r}</v></c><c r="F{r}"><v>{}</v></c></row>"#, -r)).collect();
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData><conditionalFormatting pivot="1" sqref="E4:E7"><cfRule type="iconSet" priority="1"><iconSet iconSet="3Signs" showValue="0"><cfvo type="num" val="-1"/><cfvo type="num" val="-0.5"/><cfvo type="num" val="0.5"/></iconSet></cfRule></conditionalFormatting><conditionalFormatting sqref="F1:F3"><cfRule type="iconSet" priority="2"><iconSet iconSet="3Arrows"><cfvo type="percent" val="0"/><cfvo type="percent" val="33"/><cfvo type="percent" val="67"/></iconSet></cfRule></conditionalFormatting><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", types.to_string()),
        ("_rels/.rels", rels),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", wb_rels),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, text) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(text.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn sheet_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    let mut s = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

/// (has pivot="1", sqref) of every `<conditionalFormatting>` start tag, in order.
fn cf_elements(xml: &str) -> Vec<(bool, String)> {
    xml.split("<conditionalFormatting ")
        .skip(1)
        .map(|rest| {
            let tag = &rest[..rest.find('>').unwrap()];
            let sqref = tag.split("sqref=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
            (tag.contains("pivot=\"1\"") || tag.contains("pivot=\"true\""), sqref)
        })
        .collect()
}

fn save(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = gridcraft_xlsx::read_xlsx(bytes).unwrap();
    gridcraft_xlsx::write_xlsx(&wb).unwrap()
}

#[test]
fn a_pivot_conditional_format_keeps_its_pivot_flag() {
    let mut cfs = cf_elements(&sheet_xml(&save(&xlsx())));
    cfs.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(cfs, vec![(true, "E4:E7".to_string()), (false, "F1:F3".to_string())]);
}

#[test]
fn the_pivot_flag_survives_a_second_save_and_keeps_its_priority() {
    let xml = sheet_xml(&save(&save(&xlsx())));
    let mut cfs = cf_elements(&xml);
    cfs.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(cfs, vec![(true, "E4:E7".to_string()), (false, "F1:F3".to_string())]);
    let pivot = xml.split("<conditionalFormatting ").find(|e| e.contains("E4:E7")).unwrap();
    assert!(pivot.contains("priority=\"1\""), "the PivotTable links to its rule by priority: {pivot}");
}
