//! A differential format's font keeps its family and charset through a save (ECMA-376 Part 1
//! §18.8.17 dxf, §18.8.18 family, §18.8.5 charset). Dropped, a conditional format's font loses the
//! character set the workbook stated for it.

use super::minimal;
use crate::read_xlsx;

/// Reads a workbook whose dxfs are `dxfs`, saves it and returns the written `<dxfs>` body.
fn dxfs_after_save(dxfs: &str, count: usize) -> String {
    let styles = format!(
        r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
      <fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
      <fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>
      <borders count="1"><border/></borders>
      <cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellXfs>
      <dxfs count="{count}">{dxfs}</dxfs>
    </styleSheet>"#
    );
    let rels = r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
    let mut body = String::from(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#);
    for i in 0..count {
        body.push_str(&format!(
            r#"<conditionalFormatting sqref="A{r}"><cfRule type="cellIs" dxfId="{i}" priority="{r}" operator="lessThan"><formula>1</formula></cfRule></conditionalFormatting>"#,
            r = i + 1
        ));
    }
    let bytes = minimal(&body, &[("xl/styles.xml", &styles)], rels, "");
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let out = crate::write_xlsx(&wb).unwrap();
    let st = crate::package::Package::open(&out).unwrap().read("xl/styles.xml").unwrap().unwrap();
    let st = String::from_utf8(st).unwrap();
    st.split_once("<dxfs").and_then(|(_, r)| r.split_once("</dxfs>")).map(|(d, _)| d.to_string()).unwrap_or_default()
}

#[test]
fn dxf_font_family_and_charset_survive_save() {
    let dxfs = r#"<dxf><font><sz val="10"/><name val="Arial"/><family val="2"/><charset val="1"/></font></dxf>"#;
    assert_eq!(dxfs_after_save(dxfs, 1), format!(r#" count="1">{dxfs}"#));
}

#[test]
fn dxf_font_stating_only_bold_and_charset_keeps_both() {
    let dxfs = r#"<dxf><font><b/><charset val="128"/></font></dxf>"#;
    assert_eq!(dxfs_after_save(dxfs, 1), format!(r#" count="1">{dxfs}"#));
}
