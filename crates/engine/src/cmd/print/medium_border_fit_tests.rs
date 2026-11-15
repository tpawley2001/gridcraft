//! A fitted `.xlsx` row (no `customHeight`, ECMA-376 Part 1, 18.3.1.73) that carries a stored `ht` grows
//! by one pixel for a MEDIUM top or bottom border, as it does for a thick one (`thickBot` / `thickTop`
//! mean "medium or thick"). Measured in Microsoft 365 Excel on a Calibri 11 workbook: a row with
//! `ht="15.75" thickBot="1"` and a medium bottom border keeps 21 px, and 47 rows with that one row in
//! them no longer fit a Letter page with 0.75 in top and bottom margins (row 47 prints on page 2).
use serde_json::json;

use super::*;

const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

fn border(top: &str, bottom: &str) -> String {
    let side = |name: &str, style: &str| {
        if style.is_empty() { format!("<{name}/>") } else { format!(r#"<{name} style="{style}"><color indexed="64"/></{name}>"#) }
    };
    format!("<border><left/><right/>{}{}<diagonal/></border>", side("top", top), side("bottom", bottom))
}

/// cellXfs: 0 plain, 1 medium bottom, 2 medium top, 3 thick bottom, 4 thin bottom. Calibri 11 only.
fn styles() -> String {
    let borders = [border("", ""), border("", "medium"), border("medium", ""), border("", "thick"), border("", "thin")].concat();
    let xfs: String = (0..5)
        .map(|b| {
            if b == 0 {
                r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#.to_string()
            } else {
                format!(r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="{b}" xfId="0" applyBorder="1"/>"#)
            }
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet {NS}><fonts count="1"><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="5">{borders}</borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="5">{xfs}</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#
    )
}

/// A one-sheet .xlsx of rows 1..=47 (A = row number). Row 3 gets `row3_attrs` and B3 cellXfs `b3_style`.
fn xlsx(row3_attrs: &str, b3_style: u32) -> Vec<u8> {
    use std::io::Write;
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wbx = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook {NS}><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId99" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let rows: String = (1..=47)
        .map(|r| {
            if r == 3 {
                format!(r#"<row r="3"{row3_attrs}><c r="A3"><v>3</v></c><c r="B3" s="{b3_style}"/></row>"#)
            } else {
                format!(r#"<row r="{r}"><c r="A{r}"><v>{r}</v></c></row>"#)
            }
        })
        .collect();
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet {NS}><sheetFormatPr defaultRowHeight="15"/><sheetData>{rows}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait" scale="100"/></worksheet>"#
    );
    let styles = styles();
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("xl/workbook.xml", wbx.as_str()),
        ("xl/_rels/workbook.xml.rels", rels),
        ("xl/styles.xml", styles.as_str()),
        ("xl/worksheets/sheet1.xml", sheet.as_str()),
    ] {
        z.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(data.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// (page 1's rows, page count) of the workbook's print.
fn pages(row3_attrs: &str, b3_style: u32) -> (String, usize) {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "medium.xlsx", "base64": crate::io::base64_encode(&xlsx(row3_attrs, b3_style))})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    (list[0]["rows"].as_str().unwrap().to_string(), list.len())
}

#[test]
fn medium_border_fit_medium_bottom_on_a_row_with_ht_adds_a_pixel() {
    assert_eq!(pages(r#" ht="15.75" thickBot="1""#, 1), ("1:46".to_string(), 2));
}

#[test]
fn medium_border_fit_medium_top_on_a_row_with_ht_adds_a_pixel() {
    assert_eq!(pages(r#" ht="15.75" thickTop="1""#, 2), ("1:46".to_string(), 2));
}

#[test]
fn medium_border_fit_thick_bottom_control() {
    assert_eq!(pages(r#" ht="15.75" thickBot="1""#, 3), ("1:46".to_string(), 2));
}

#[test]
fn medium_border_fit_thin_bottom_adds_nothing() {
    assert_eq!(pages(r#" ht="15""#, 4), ("1:47".to_string(), 1));
}

#[test]
fn medium_border_fit_medium_bottom_without_ht_adds_nothing() {
    assert_eq!(pages("", 1), ("1:47".to_string(), 1));
}

#[test]
fn medium_border_fit_plain_rows_fit_one_page() {
    assert_eq!(pages("", 0), ("1:47".to_string(), 1));
}
