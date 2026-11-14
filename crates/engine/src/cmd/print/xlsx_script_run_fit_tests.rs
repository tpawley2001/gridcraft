//! An .xlsx row without `customHeight` whose cell string has a superscript run (`<r><rPr>
//! <vertAlign val="superscript"/>`, ECMA-376 Part 1, 18.4.7 / 18.4.14) prints like a superscript
//! cell font: Excel fits such an Arial 10 row (Arial 10 Normal font) to 14.25 pt, 19 px.
use serde_json::json;

use super::*;

/// A one-sheet .xlsx with Arial 10 as the Normal font, Letter portrait with 0.75 in top/bottom
/// margins, and rows 1..=`n` each holding shared string `si` in A with `row_attrs`.
fn xlsx(n: u32, si: u32, row_attrs: &str) -> Vec<u8> {
    use std::io::Write;
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>"#;
    let root = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wbx = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#;
    let styles = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="10"/><name val="Arial"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="1"><xf/></cellXfs></styleSheet>"#;
    let sst = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>m2</t></si><si><r><t>m</t></r><r><rPr><vertAlign val="superscript"/><sz val="10"/><rFont val="Arial"/></rPr><t>2</t></r></si></sst>"#;
    let rows: String = (1..=n).map(|r| format!(r#"<row r="{r}" {row_attrs}><c r="A{r}" t="s"><v>{si}</v></c></row>"#)).collect();
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetFormatPr defaultRowHeight="12.75"/><sheetData>{rows}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait"/></worksheet>"#
    );
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("xl/workbook.xml", wbx),
        ("xl/_rels/workbook.xml.rels", rels),
        ("xl/styles.xml", styles),
        ("xl/sharedStrings.xml", sst),
        ("xl/worksheets/sheet1.xml", sheet.as_str()),
    ] {
        z.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(data.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// Opens `bytes` and returns each printed page's `rows`.
fn page_rows(bytes: &[u8]) -> Vec<String> {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "runs.xlsx", "base64": crate::io::base64_encode(bytes)})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

const N: u32 = 200;

#[test]
fn a_superscript_run_fits_its_row_to_14_25_pt() {
    let got = page_rows(&xlsx(N, 1, r#"ht="12.75""#));
    let want = page_rows(&xlsx(N, 1, r#"ht="14.25" customHeight="1""#));
    let plain = page_rows(&xlsx(N, 1, r#"ht="12.75" customHeight="1""#));
    assert_ne!(want, plain, "the case must tell 14.25 pt from 12.75 pt rows");
    assert_eq!(got, want);
}

#[test]
fn the_same_text_without_a_run_keeps_the_plain_row() {
    let got = page_rows(&xlsx(N, 0, r#"ht="12.75""#));
    let want = page_rows(&xlsx(N, 0, r#"ht="12.75" customHeight="1""#));
    assert_eq!(got, want);
}
