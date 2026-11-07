//! Synthetic XLSX acceptance tests; no vendor-produced fixtures.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> El {
    parse(&Package::open(bytes).unwrap().read(name).unwrap().unwrap()).unwrap()
}
fn save(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}
fn twice(bytes: &[u8], name: &str) -> El {
    part(&save(&save(bytes)), name)
}

fn styled(font: &str, border: &str, alignment: &str, dxf: &str) -> Vec<u8> {
    let styles = format!(
        r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Example Sans"/>{font}</font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border>{border}</border></borders><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0">{alignment}</xf></cellXfs><dxfs count="1"><dxf>{dxf}</dxf></dxfs></styleSheet>"#
    );
    minimal(
        r#"<sheetData><row r="1"><c r="A1" s="0"><v>7</v></c></row></sheetData><conditionalFormatting sqref="A1"><cfRule type="cellIs" dxfId="0" priority="1" operator="lessThan"><formula>10</formula></cfRule></conditionalFormatting>"#,
        &[("xl/styles.xml", &styles)],
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        "",
    )
}
fn styles(bytes: &[u8]) -> El {
    twice(bytes, "xl/styles.xml")
}

#[test]
fn differential_inside_borders_survive_with_their_own_colors() {
    let x = styles(&styled(
        "",
        "",
        "",
        r#"<border outline="0"><horizontal style="dashed"><color rgb="FF123456"/></horizontal><vertical style="double"><color rgb="FFABCDEF"/></vertical></border>"#,
    ));
    let b = x.child("dxfs").unwrap().child("dxf").unwrap().child("border").unwrap();
    assert_eq!(b.attr_bool("outline"), Some(false));
    for (tag, style, color) in [("horizontal", "dashed", "FF123456"), ("vertical", "double", "FFABCDEF")] {
        let line = b.child(tag).unwrap();
        assert_eq!(line.attr("style"), Some(style));
        assert_eq!(line.child("color").unwrap().attr("rgb"), Some(color));
    }
}
#[test]
fn normal_border_inside_lines_are_retained_via_the_xf_border_id() {
    let x = styles(&styled(
        "",
        r#"<horizontal style="thin"><color rgb="FF102030"/></horizontal><vertical style="thick"><color rgb="FF405060"/></vertical>"#,
        "",
        "",
    ));
    let id = x.child("cellXfs").unwrap().child("xf").unwrap().attr_u32("borderId").unwrap() as usize;
    let b = x.child("borders").unwrap().kids("border").nth(id).unwrap();
    assert_eq!(b.child("horizontal").unwrap().attr("style"), Some("thin"));
    assert_eq!(b.child("vertical").unwrap().attr("style"), Some("thick"));
}
#[test]
fn explicit_outline_is_independent_of_outer_edges() {
    for value in ["1", "0"] {
        let x = styles(&styled("", "", "", &format!(r#"<border outline="{value}"><left style="thin"/></border>"#)));
        let b = x.child("dxfs").unwrap().child("dxf").unwrap().child("border").unwrap();
        assert_eq!(b.attr_bool("outline"), Some(value == "1"));
        assert_eq!(b.child("left").unwrap().attr("style"), Some("thin"));
    }
}
