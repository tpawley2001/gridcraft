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

fn table(attrs: &str) -> El {
    let xml = format!(
        r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="7" name="Sample" displayName="Sample" ref="A1:A2" {attrs}><tableColumns count="1"><tableColumn id="1" name="Value"/></tableColumns></table>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="table" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#;
    let bytes = minimal(
        r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Value</t></is></c></row><row r="2"><c r="A2"><v>7</v></c></row></sheetData><tableParts count="1"><tablePart r:id="table"/></tableParts>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/tables/table1.xml", &xml)],
        "",
        "",
    );
    twice(&bytes, "xl/tables/table1.xml")
}
#[test]
fn table_flags_preserve_both_values_independently() {
    for on in [true, false] {
        let v = if on { "1" } else { "0" };
        let x = table(&format!(r#"insertRow="{v}" insertRowShift="{v}" published="{v}""#));
        for k in ["insertRow", "insertRowShift", "published"] {
            assert_eq!(x.attr_bool(k), Some(on), "{k}");
        }
    }
}
#[test]
fn comments_escape_and_decode_once_on_each_save() {
    let x = table(r#"comment="A&amp;B&lt;&quot;é&#10;_x005F_x0041_""#);
    assert_eq!(crate::xml::decode_xstring(x.attr("comment").unwrap()), "A&B<\"é\n_x0041_");
}
#[test]
fn empty_comment_is_distinct_from_absent() {
    assert_eq!(table(r#"comment="""#).attr("comment"), Some(""));
    assert!(table("").attr("comment").is_none());
}
#[test]
fn unknown_flags_are_omitted_and_table_stays_valid() {
    let x = table(r#"insertRow="yes" insertRowShift="2" published="no""#);
    for k in ["insertRow", "insertRowShift", "published"] {
        assert!(x.attr(k).is_none());
    }
    assert_eq!(x.attr("ref"), Some("A1:A2"));
    assert_eq!(x.child("tableColumns").unwrap().kids("tableColumn").count(), 1);
}
