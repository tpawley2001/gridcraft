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
fn all_cell_font_effects_keep_true_and_false() {
    for val in ["1", "0"] {
        let body = format!(r#"<condense val="{val}"/><extend val="{val}"/><outline val="{val}"/><shadow val="{val}"/>"#);
        let x = styles(&styled(&body, "", "", ""));
        let f = x.child("fonts").unwrap().child("font").unwrap();
        for k in ["condense", "extend", "outline", "shadow"] {
            assert_eq!(f.child(k).unwrap().flag("val", true), val == "1", "{k}");
        }
    }
}
#[test]
fn differential_false_is_not_omitted_or_promoted_to_true() {
    let x = styles(&styled("", "", "", r#"<font><condense val="0"/><extend val="false"/><outline val="0"/><shadow val="false"/></font>"#));
    let f = x.child("dxfs").unwrap().child("dxf").unwrap().child("font").unwrap();
    for k in ["condense", "extend", "outline", "shadow"] {
        assert_eq!(f.child(k).unwrap().attr_bool("val"), Some(false), "{k}");
    }
}
#[test]
fn empty_effect_element_means_true_and_has_schema_order() {
    let x = styles(&styled("<shadow/><outline/><extend/><condense/>", "", "", ""));
    let f = x.child("fonts").unwrap().child("font").unwrap();
    let tags: Vec<_> =
        f.children.iter().filter(|e| ["condense", "extend", "outline", "shadow"].contains(&e.name.as_str())).map(|e| e.name.as_str()).collect();
    assert_eq!(tags, ["condense", "extend", "outline", "shadow"]);
    for k in tags {
        assert!(f.child(k).unwrap().flag("val", true));
    }
}
#[test]
fn invalid_effect_values_are_omitted() {
    let x = styles(&styled(r#"<outline val="yes"/><shadow val="2"/>"#, "", "", ""));
    let f = x.child("fonts").unwrap().child("font").unwrap();
    assert!(f.child("outline").is_none());
    assert!(f.child("shadow").is_none());
}
