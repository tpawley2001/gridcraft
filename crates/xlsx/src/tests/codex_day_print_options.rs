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

fn sheet(attrs: &str) -> El {
    twice(&minimal(&format!("<sheetData/><pageSetup {attrs}/>"), &[], "", ""), "xl/worksheets/sheet1.xml")
}
#[test]
fn flags_and_print_policies_survive_twice() {
    for comments in ["none", "asDisplayed", "atEnd"] {
        for errors in ["displayed", "blank", "dash", "NA"] {
            let x = sheet(&format!(r#"blackAndWhite="1" draft="true" cellComments="{comments}" errors="{errors}" usePrinterDefaults="0""#));
            let p = x.child("pageSetup").unwrap();
            assert_eq!(p.attr_bool("blackAndWhite"), Some(true));
            assert_eq!(p.attr_bool("draft"), Some(true));
            assert_eq!(p.attr_bool("usePrinterDefaults"), Some(false));
            assert_eq!(p.attr("cellComments"), Some(comments));
            assert_eq!(p.attr("errors"), Some(errors));
        }
    }
}
#[test]
fn numbering_resolution_and_copies_are_independent() {
    let x = sheet(r#"firstPageNumber="5" useFirstPageNumber="0" horizontalDpi="200" verticalDpi="300" copies="2""#);
    let p = x.child("pageSetup").unwrap();
    for (k, v) in [("firstPageNumber", 5), ("horizontalDpi", 200), ("verticalDpi", 300), ("copies", 2)] {
        assert_eq!(p.attr_u32(k), Some(v), "{k}");
    }
    assert_eq!(p.attr_bool("useFirstPageNumber"), Some(false));
}
#[test]
fn explicit_false_zero_and_unsigned_maximum_survive() {
    let x = sheet(
        r#"blackAndWhite="false" draft="0" firstPageNumber="4294967295" useFirstPageNumber="1" horizontalDpi="0" verticalDpi="4294967293" copies="0""#,
    );
    let p = x.child("pageSetup").unwrap();
    assert_eq!(p.attr_bool("blackAndWhite"), Some(false));
    assert_eq!(p.attr_bool("draft"), Some(false));
    assert_eq!(p.attr_u32("firstPageNumber"), Some(u32::MAX));
    assert_eq!(p.attr_u32("horizontalDpi"), Some(0));
    assert_eq!(p.attr_u32("verticalDpi"), Some(u32::MAX - 2));
    assert_eq!(p.attr_u32("copies"), Some(0));
}
#[test]
fn malformed_supplemental_values_are_omitted() {
    let x = sheet(
        r#"blackAndWhite="yes" draft="no" firstPageNumber="-1" horizontalDpi="4294967296" verticalDpi="1.5" copies="NaN" errors="unknown" cellComments="all""#,
    );
    let p = x.child("pageSetup").unwrap();
    for k in ["blackAndWhite", "draft", "firstPageNumber", "horizontalDpi", "verticalDpi", "copies", "errors", "cellComments"] {
        assert!(p.attr(k).is_none(), "{k}");
    }
}
#[test]
fn existing_page_setup_still_survives() {
    let x = sheet(r#"orientation="landscape" scale="75" pageOrder="overThenDown""#);
    let p = x.child("pageSetup").unwrap();
    assert_eq!(p.attr("orientation"), Some("landscape"));
    assert_eq!(p.attr_u32("scale"), Some(75));
    assert_eq!(p.attr("pageOrder"), Some("overThenDown"));
}
