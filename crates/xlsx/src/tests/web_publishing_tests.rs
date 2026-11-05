//! Web publishing fidelity and bounded optional-attribute handling.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};

fn saved(body: &str) -> El {
    let bytes = minimal("<sheetData/>", &[], "", body);
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let first = write_xlsx(&wb).unwrap();
    let (again, _) = read_xlsx(&first).unwrap();
    let second = write_xlsx(&again).unwrap();
    let xml = Package::open(&second).unwrap().read("xl/workbook.xml").unwrap().unwrap();
    parse(&xml).unwrap()
}

#[test]
fn all_preferences_false_flags_and_escaped_text_survive_twice() {
    let xml = saved(
        r#"<webPublishing css="false" thicket="0" longFileNames="false" vml="0" allowPng="false" usePt="0" codePage="1251" dpi="144" targetScreenSize="1152x882" characterSet="A&amp;B&lt;&quot;é&#9;&#10;&#13;"/>"#,
    );
    let p = xml.child("webPublishing").unwrap();
    for name in ["css", "thicket", "longFileNames", "vml", "allowPng", "usePt"] {
        assert_eq!(p.attr(name), Some("0"), "{name}");
    }
    assert_eq!(p.attr("codePage"), Some("1251"));
    assert_eq!(p.attr("dpi"), Some("144"));
    assert_eq!(p.attr("targetScreenSize"), Some("1152x882"));
    assert_eq!(p.attr("characterSet"), Some("A&B<\"é\t\n\r"));
    assert!(p.children.is_empty());
    let names: Vec<_> = xml.children.iter().map(|e| e.name.as_str()).collect();
    assert!(names.iter().position(|n| *n == "calcPr") < names.iter().position(|n| *n == "webPublishing"));
}

#[test]
fn absent_and_explicitly_empty_remain_distinct() {
    assert!(saved("").child("webPublishing").is_none());
    let xml = saved("<webPublishing/>");
    assert!(xml.child("webPublishing").unwrap().attrs.is_empty());
}

#[test]
fn invalid_numbers_and_unknown_enum_are_omitted() {
    for value in ["-1", "4294967296", "1.5", "NaN", "", "1e3"] {
        let xml = saved(&format!(r#"<webPublishing codePage="{value}" dpi="{value}" targetScreenSize="999x888" css="yes"/>"#));
        let p = xml.child("webPublishing").unwrap();
        assert!(p.attrs.is_empty(), "{value}");
    }
    let xml = saved(r#"<webPublishing codePage="4294967295" dpi="0"/>"#);
    let p = xml.child("webPublishing").unwrap();
    assert_eq!(p.attr_u32("codePage"), Some(u32::MAX));
    assert_eq!(p.attr_u32("dpi"), Some(0));
}

#[test]
fn oversized_text_unknown_attributes_and_child_markup_are_dropped() {
    let xml = saved(&format!(
        r#"<webPublishing xmlns:e="urn:example" characterSet="{}" e:unknown="payload" href="https://example.invalid/" codePage="1252"><e:payload/></webPublishing>"#,
        "x".repeat(1025)
    ));
    let p = xml.child("webPublishing").unwrap();
    assert_eq!(p.attrs, vec![("codePage".into(), "1252".into())]);
    assert!(p.children.is_empty());
}

#[test]
fn preservation_merges_with_other_workbook_content() {
    let xml = saved(r#"<fileVersion appName="xl" lastEdited="7"/><webPublishing codePage="1252"/>"#);
    assert_eq!(xml.child("fileVersion").unwrap().attr("lastEdited"), Some("7"));
    assert_eq!(xml.child("webPublishing").unwrap().attr("codePage"), Some("1252"));
}
