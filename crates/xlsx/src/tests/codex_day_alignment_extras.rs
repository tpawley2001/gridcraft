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
fn justify_last_line_is_not_lost_even_if_only_alignment_property() {
    for v in ["1", "0"] {
        let x = styles(&styled("", "", &format!(r#"<alignment justifyLastLine="{v}"/>"#), ""));
        let a = x.child("cellXfs").unwrap().child("xf").unwrap().child("alignment").unwrap();
        assert_eq!(a.attr_bool("justifyLastLine"), Some(v == "1"));
    }
}
#[test]
fn relative_indent_is_signed_and_independent_from_absolute_indent() {
    for n in [-2, 0, 2, i32::MIN, i32::MAX] {
        let x = styles(&styled("", "", "", &format!(r#"<alignment indent="3" relativeIndent="{n}" justifyLastLine="0"/>"#)));
        let a = x.child("dxfs").unwrap().child("dxf").unwrap().child("alignment").unwrap();
        assert_eq!(a.attr_i64("relativeIndent"), Some(i64::from(n)));
        assert_eq!(a.attr_u32("indent"), Some(3));
        assert_eq!(a.attr_bool("justifyLastLine"), Some(false));
    }
}
#[test]
fn invalid_extras_do_not_replace_existing_alignment() {
    let x = styles(&styled("", "", r#"<alignment horizontal="right" justifyLastLine="yes"/>"#, r#"<alignment relativeIndent="2147483648"/>"#));
    let a = x.child("cellXfs").unwrap().child("xf").unwrap().child("alignment").unwrap();
    assert_eq!(a.attr("horizontal"), Some("right"));
    assert!(a.attr("justifyLastLine").is_none());
    for d in x.child("dxfs").unwrap().kids("dxf") {
        if let Some(a) = d.child("alignment") {
            assert!(a.attr("relativeIndent").is_none());
        }
    }
}
