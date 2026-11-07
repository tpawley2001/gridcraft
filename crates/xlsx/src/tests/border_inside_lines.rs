//! Inside border lines and `outline` (ECMA-376 Part 1 §18.8.4, `CT_Border`): worker tests for the
//! cases the acceptance module leaves out.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> El {
    parse(&Package::open(bytes).unwrap().read(name).unwrap().unwrap()).unwrap()
}

fn styled(border: &str, dxf: &str) -> Vec<u8> {
    let styles = format!(
        r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Example Sans"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border>{border}</border></borders><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellXfs><dxfs count="1"><dxf>{dxf}</dxf></dxfs></styleSheet>"#
    );
    minimal(
        r#"<sheetData><row r="1"><c r="A1" s="0"><v>7</v></c></row></sheetData><conditionalFormatting sqref="A1"><cfRule type="cellIs" dxfId="0" priority="1" operator="lessThan"><formula>10</formula></cfRule></conditionalFormatting>"#,
        &[("xl/styles.xml", &styles)],
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        "",
    )
}

fn saved(bytes: &[u8]) -> El {
    let (wb, _) = read_xlsx(bytes).unwrap();
    let once = write_xlsx(&wb).unwrap();
    let (wb, _) = read_xlsx(&once).unwrap();
    part(&write_xlsx(&wb).unwrap(), "xl/styles.xml")
}

fn dxf_border(x: &El) -> &El {
    x.child("dxfs").unwrap().child("dxf").unwrap().child("border").unwrap()
}

/// The border a cell format points at, by its `borderId`.
fn xf_border(x: &El) -> &El {
    let id = x.child("cellXfs").unwrap().child("xf").unwrap().attr_u32("borderId").unwrap() as usize;
    x.child("borders").unwrap().kids("border").nth(id).unwrap()
}

#[test]
fn an_absent_outline_stays_absent_and_an_explicit_one_is_kept() {
    let x = saved(&styled(r#"<left style="thin"/>"#, r#"<border outline="0"><horizontal style="thin"/></border>"#));
    assert_eq!(xf_border(&x).attr("outline"), None);
    assert_eq!(dxf_border(&x).attr_bool("outline"), Some(false));
    let x = saved(&styled(r#"<left style="thin"/>"#, r#"<border outline="1"><horizontal style="thin"/></border>"#));
    assert_eq!(dxf_border(&x).attr_bool("outline"), Some(true));
}

#[test]
fn inside_lines_keep_theme_colours_and_automatic_colours() {
    let x = saved(&styled(r#"<horizontal style="hair"><color theme="4" tint="0.5"/></horizontal><vertical style="dotted"/>"#, ""));
    let b = xf_border(&x);
    let h = b.child("horizontal").unwrap();
    assert_eq!(h.attr("style"), Some("hair"));
    let c = h.child("color").unwrap();
    assert_eq!((c.attr("theme"), c.attr("tint")), (Some("4"), Some("0.5")));
    let v = b.child("vertical").unwrap();
    assert_eq!(v.attr("style"), Some("dotted"));
    assert_eq!(v.child("color").unwrap().attr("indexed"), Some("64"));
}

#[test]
fn inside_lines_are_written_in_schema_order_after_the_diagonal() {
    let x = saved(&styled(r#"<left style="thin"/><vertical style="thin"/><horizontal style="thin"/>"#, ""));
    let b = xf_border(&x);
    let tags: Vec<&str> = b.children.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(tags, ["left", "right", "top", "bottom", "diagonal", "vertical", "horizontal"]);
}

#[test]
fn a_border_without_inside_lines_writes_none() {
    let x = saved(&styled(r#"<left style="thin"/>"#, ""));
    let b = xf_border(&x);
    assert_eq!(b.child("horizontal").unwrap().attr("style"), None);
    assert_eq!(b.child("vertical").unwrap().attr("style"), None);
}

#[test]
fn an_unknown_inside_style_reads_as_no_line() {
    let x = saved(&styled(r#"<horizontal style="wavy"><color rgb="FF123456"/></horizontal>"#, ""));
    let b = xf_border(&x);
    assert_eq!(b.child("horizontal").unwrap().attr("style"), None);
}
