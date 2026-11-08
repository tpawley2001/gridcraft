//! Outline Settings' Automatic styles (`<outlinePr applyStyles="1"/>`, ECMA-376 Part 1 §18.3.1.61)
//! is read and written back instead of dropped.
use super::minimal;
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};
use gridcraft_model::Workbook;

fn sheet_xml(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    String::from_utf8(Package::open(&bytes).unwrap().read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
}

fn open(sheet_pr: &str) -> Workbook {
    read_xlsx(&minimal(&format!("{sheet_pr}<sheetData/>"), &[], "", "")).unwrap().0
}

#[test]
fn apply_styles_is_read_and_written_back() {
    let wb = open(r#"<sheetPr><outlinePr applyStyles="1"/></sheetPr>"#);
    assert!(wb.sheet(0).unwrap().outline_apply_styles);
    let xml = sheet_xml(&wb);
    assert!(xml.contains(r#"<outlinePr applyStyles="1"/>"#), "{xml}");
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let sh = back.sheet(0).unwrap();
    assert!(sh.outline_apply_styles && sh.summary_below && sh.summary_right);
}

#[test]
fn apply_styles_comes_first_with_the_summary_directions() {
    let wb = open(r#"<sheetPr><outlinePr applyStyles="1" summaryBelow="0" summaryRight="0"/></sheetPr>"#);
    let xml = sheet_xml(&wb);
    assert!(xml.contains(r#"<outlinePr applyStyles="1" summaryBelow="0" summaryRight="0"/>"#), "{xml}");
}

#[test]
fn absent_or_off_means_no_automatic_styles_and_writes_nothing() {
    for pr in ["", r#"<sheetPr><outlinePr applyStyles="0"/></sheetPr>"#] {
        let wb = open(pr);
        assert!(!wb.sheet(0).unwrap().outline_apply_styles, "{pr}");
        assert!(!sheet_xml(&wb).contains("outlinePr"), "{pr}");
    }
}

#[test]
fn summary_directions_alone_still_write_no_apply_styles() {
    let wb = open(r#"<sheetPr><outlinePr summaryBelow="0"/></sheetPr>"#);
    let xml = sheet_xml(&wb);
    assert!(xml.contains(r#"<outlinePr summaryBelow="0"/>"#), "{xml}");
}
