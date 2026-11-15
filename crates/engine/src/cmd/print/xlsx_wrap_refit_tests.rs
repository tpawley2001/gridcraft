//! Rows with a stored `ht` but no `customHeight` (ECMA-376 Part 1, 18.3.1.73) that hold wrapped text
//! with a value print at the height their text fits, not at `ht`: Microsoft 365 Excel re-fits them
//! (measured on Excel's re-saves: stored 18 pt rows of one line come back at the default row, 28 pt
//! rows of two Arial 10 lines at 25.5 pt). Rows whose wrapped cells hold no value keep `ht`, and
//! `customHeight` rows always do. Arial 10 Normal font, 300 rows (A = row number, B per case),
//! Letter portrait; plain rows print 55 rows on page 1 and 6 pages (see xlsx_fit_probe_1114_tests).
use serde_json::json;

use super::*;

const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// cellXfs: 0 plain, 1 wrapped. Arial 10.
fn styles() -> String {
    let font = r#"<font><sz val="10"/><name val="Arial"/><family val="2"/></font>"#;
    let xfs = concat!(
        r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#,
        r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment wrapText="1"/></xf>"#
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet {NS}><fonts count="1">{font}</fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2">{xfs}</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#
    )
}

/// One case: the `<row>` attributes after `r` and cell B of every row.
#[derive(Clone, Copy)]
struct Case {
    row_attrs: &'static str,
    /// cellXfs index of B.
    s: u32,
    /// B's inline text; `None` = a styled cell with no value.
    text: Option<&'static str>,
}

fn xlsx(case: Case) -> Vec<u8> {
    use std::io::Write;
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wbx = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook {NS}><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId99" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let rows: String = (1..=300)
        .map(|r| {
            let b = match case.text {
                Some(t) => format!(r#"<c r="B{r}" s="{}" t="inlineStr"><is><t xml:space="preserve">{t}</t></is></c>"#, case.s),
                None => format!(r#"<c r="B{r}" s="{}"/>"#, case.s),
            };
            format!(r#"<row r="{r}" {}><c r="A{r}"><v>{r}</v></c>{b}</row>"#, case.row_attrs)
        })
        .collect();
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet {NS}><sheetFormatPr defaultRowHeight="12.75"/><sheetData>{rows}</sheetData><pageMargins left="0.5" right="0.5" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait" scale="100"/><headerFooter><oddHeader>&amp;C&amp;A p&amp;P</oddHeader></headerFooter></worksheet>"#
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

/// Opens the case's workbook and returns (page 1's rows, page count).
fn first_page(case: Case) -> (String, usize) {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "wrap.xlsx", "base64": crate::io::base64_encode(&xlsx(case))})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    (list[0]["rows"].as_str().unwrap().to_string(), list.len())
}

const HT18: &str = r#"ht="18""#;
const CUSTOM18: &str = r#"ht="18" customHeight="1""#;
const CUSTOM25_5: &str = r#"ht="25.5" customHeight="1""#;

fn plain() -> (String, usize) {
    ("1:55".to_string(), 6)
}

#[test]
fn xlsx_wrap_refit_unwrapped_row_with_ht_refits_to_the_default_row() {
    assert_eq!(first_page(Case { row_attrs: HT18, s: 0, text: Some("x") }), plain());
}

#[test]
fn xlsx_wrap_refit_custom_height_row_keeps_its_height() {
    let custom = first_page(Case { row_attrs: CUSTOM18, s: 0, text: Some("x") });
    assert_ne!(custom, plain());
    assert_eq!(first_page(Case { row_attrs: CUSTOM18, s: 1, text: Some("x") }), custom);
}

#[test]
fn xlsx_wrap_refit_wrapped_one_line_row_refits_to_the_default_row() {
    assert_eq!(first_page(Case { row_attrs: HT18, s: 1, text: Some("x") }), plain());
}

#[test]
fn xlsx_wrap_refit_wrapped_two_line_row_fits_two_lines() {
    let two_lines = first_page(Case { row_attrs: CUSTOM25_5, s: 0, text: Some("x") });
    assert_eq!(first_page(Case { row_attrs: HT18, s: 1, text: Some("x\ny") }), two_lines);
}

#[test]
fn xlsx_wrap_refit_wrapped_cell_without_a_value_keeps_the_stored_height() {
    let stored = first_page(Case { row_attrs: CUSTOM18, s: 0, text: Some("x") });
    assert_eq!(first_page(Case { row_attrs: HT18, s: 1, text: None }), stored);
}
