//! Rows without `customHeight` and without a stored `ht` (ECMA-376 Part 1, 18.3.1.73) in a sheet
//! read from .xlsx print at the height their content fits. Expected values are Microsoft 365
//! Excel's own print of the same workbook (Arial 10 Normal font, 300 rows: a number in A, the case's
//! cell in B; Letter portrait, margins 0.5 / 0.75 in, header and footer 0.3 in, a centre header):
//! rows on page 1 and the page count.
use serde_json::json;

use super::*;

const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

fn font(extra: &str, sz: u32, name: &str) -> String {
    format!(r#"<font>{extra}<sz val="{sz}"/><name val="{name}"/><family val="2"/></font>"#)
}

fn border(side: &str, style: &str) -> String {
    let top = if side == "top" { format!(r#"<top style="{style}"><color auto="1"/></top>"#) } else { "<top/>".to_string() };
    let bottom = if side == "bottom" { format!(r#"<bottom style="{style}"><color auto="1"/></bottom>"#) } else { "<bottom/>".to_string() };
    format!("<border><left/><right/>{top}{bottom}<diagonal/></border>")
}

/// cellXfs: 0 plain, 1 superscript, 2 subscript, 3..=7 borders (bottom thin, medium, thick,
/// double; top thick), 8 Calibri 11, 9 bold superscript. All fonts Arial 10 unless named.
fn styles() -> String {
    let fonts = [
        font("", 10, "Arial"),
        font(r#"<vertAlign val="superscript"/>"#, 10, "Arial"),
        font(r#"<vertAlign val="subscript"/>"#, 10, "Arial"),
        font("", 11, "Calibri"),
        font(r#"<b/><vertAlign val="superscript"/>"#, 10, "Arial"),
    ]
    .concat();
    let borders = [
        "<border><left/><right/><top/><bottom/><diagonal/></border>".to_string(),
        border("bottom", "thin"),
        border("bottom", "medium"),
        border("bottom", "thick"),
        border("bottom", "double"),
        border("top", "thick"),
    ]
    .concat();
    let mut xfs = String::from(r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#);
    xfs.push_str(r#"<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>"#);
    xfs.push_str(r#"<xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyFont="1"/>"#);
    for b in 1..=5 {
        xfs.push_str(&format!(r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="{b}" xfId="0" applyBorder="1"/>"#));
    }
    xfs.push_str(r#"<xf numFmtId="0" fontId="3" fillId="0" borderId="0" xfId="0" applyFont="1"/>"#);
    xfs.push_str(r#"<xf numFmtId="0" fontId="4" fillId="0" borderId="0" xfId="0" applyFont="1"/>"#);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet {NS}><fonts count="5">{fonts}</fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="6">{borders}</borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="10">{xfs}</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#
    )
}

/// What cell B of every row holds.
#[derive(Clone, Copy)]
enum B {
    /// Inline text "x" with cellXfs index `s`.
    Styled(u32),
    /// Inline rich text: "Hires" then a superscript run of Arial `size`.
    SupRun(u32),
    /// Inline rich text: "H" then a subscript Arial 10 run "2".
    SubRun,
}

fn cell_b(b: B, r: u32) -> String {
    match b {
        B::Styled(s) => format!(r#"<c r="B{r}" s="{s}" t="inlineStr"><is><t>x</t></is></c>"#),
        B::SupRun(sz) => format!(
            r#"<c r="B{r}" t="inlineStr"><is><r><t xml:space="preserve">Hires</t></r><r><rPr><vertAlign val="superscript"/><sz val="{sz}"/><rFont val="Arial"/><family val="2"/></rPr><t>(1)</t></r></is></c>"#
        ),
        B::SubRun => format!(
            r#"<c r="B{r}" t="inlineStr"><is><r><t xml:space="preserve">H</t></r><r><rPr><vertAlign val="subscript"/><sz val="10"/><rFont val="Arial"/><family val="2"/></rPr><t>2</t></r></is></c>"#
        ),
    }
}

/// A one-sheet .xlsx: rows 1..=300, `<row r="N">` with no `ht`, A = N, B per `b`.
fn xlsx(b: B) -> Vec<u8> {
    use std::io::Write;
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wbx = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook {NS}><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId99" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let rows: String = (1..=300).map(|r| format!(r#"<row r="{r}"><c r="A{r}"><v>{r}</v></c>{}</row>"#, cell_b(b, r))).collect();
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
fn first_page(b: B) -> (String, usize) {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "probe.xlsx", "base64": crate::io::base64_encode(&xlsx(b))})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    (list[0]["rows"].as_str().unwrap().to_string(), list.len())
}

fn check(b: B, rows: u32, pages: usize) {
    assert_eq!(first_page(b), (format!("1:{rows}"), pages));
}

#[test]
fn xlsx_fit_probe_1114_plain_rows() {
    check(B::Styled(0), 55, 6);
}

#[test]
fn xlsx_fit_probe_1114_superscript_cell_font() {
    check(B::Styled(1), 49, 7);
}

#[test]
fn xlsx_fit_probe_1114_bold_superscript_cell_font() {
    check(B::Styled(9), 49, 7);
}

#[test]
fn xlsx_fit_probe_1114_subscript_cell_font_is_taller_than_superscript() {
    check(B::Styled(2), 44, 7);
}

#[test]
fn xlsx_fit_probe_1114_superscript_run() {
    check(B::SupRun(10), 49, 7);
}

#[test]
fn xlsx_fit_probe_1114_subscript_run() {
    check(B::SubRun, 44, 7);
}

#[test]
fn xlsx_fit_probe_1114_a_14_pt_superscript_run_fits_its_own_size() {
    check(B::SupRun(14), 33, 10);
}

#[test]
fn xlsx_fit_probe_1114_thin_medium_double_bottom_borders_add_nothing() {
    for s in [3, 4, 6] {
        check(B::Styled(s), 55, 6);
    }
}

#[test]
fn xlsx_fit_probe_1114_thick_bottom_on_a_row_without_ht_adds_nothing() {
    check(B::Styled(5), 55, 6);
}

#[test]
fn xlsx_fit_probe_1114_thick_top_on_a_row_without_ht_adds_nothing() {
    check(B::Styled(7), 55, 6);
}

#[test]
fn xlsx_fit_probe_1114_calibri_11_cell_font() {
    check(B::Styled(8), 47, 7);
}
