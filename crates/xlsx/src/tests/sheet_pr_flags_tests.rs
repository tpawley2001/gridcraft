//! `<sheetPr published>` (ECMA-376 Part 1 §18.3.1.82) and `<outlinePr showOutlineSymbols>`
//! (§18.3.1.31) survive open + save. Packages are built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

/// Open + save a sheet whose XML starts with `sheet_pr`; the saved sheet's root element.
fn saved(sheet_pr: &str, views: &str) -> El {
    let body = format!(r#"{sheet_pr}{views}<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#);
    let out = write_xlsx(&read_xlsx(&minimal(&body, &[], "", "")).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    xml::parse(s.as_bytes()).unwrap()
}

#[test]
fn sheet_pr_flags_published_off_is_kept() {
    let sheet = saved(r#"<sheetPr published="0"/>"#, "");
    let pr = sheet.child("sheetPr").expect("sheetPr is written for published=\"0\" alone");
    assert_eq!(pr.attr("published"), Some("0"));
    assert!(pr.child("outlinePr").is_none());

    let sheet = saved(r#"<sheetPr published="false" codeName="Sheet7"><tabColor rgb="FFFF0000"/></sheetPr>"#, "");
    let pr = sheet.child("sheetPr").expect("sheetPr");
    assert!(!pr.flag("published", true));
    assert_eq!(pr.attr("codeName"), Some("Sheet7"));
    assert!(pr.child("tabColor").is_some());
}

#[test]
fn sheet_pr_flags_outline_symbols_off_is_kept() {
    let sheet = saved(r#"<sheetPr><outlinePr applyStyles="1" summaryBelow="0" summaryRight="0" showOutlineSymbols="0"/></sheetPr>"#, "");
    let o = sheet.child("sheetPr").and_then(|p| p.child("outlinePr")).expect("outlinePr");
    assert_eq!(o.attr("showOutlineSymbols"), Some("0"));
    assert_eq!((o.attr("applyStyles"), o.attr("summaryBelow"), o.attr("summaryRight")), (Some("1"), Some("0"), Some("0")));

    // The flag alone still needs its element.
    let sheet = saved(r#"<sheetPr><outlinePr showOutlineSymbols="0"/></sheetPr>"#, "");
    let pr = sheet.child("sheetPr").expect("sheetPr is written for showOutlineSymbols=\"0\" alone");
    let o = pr.child("outlinePr").expect("outlinePr is written for showOutlineSymbols=\"0\" alone");
    assert_eq!(o.attr("showOutlineSymbols"), Some("0"));
    assert_eq!((o.attr("applyStyles"), o.attr("summaryBelow"), o.attr("summaryRight")), (None, None, None));
    assert_eq!(pr.attr("published"), None);
}

#[test]
fn sheet_pr_flags_keep_the_element_order() {
    let sheet = saved(
        r#"<sheetPr published="0"><tabColor rgb="FF00B050"/><outlinePr showOutlineSymbols="0"/><pageSetUpPr autoPageBreaks="0"/></sheetPr>"#,
        "",
    );
    let pr = sheet.child("sheetPr").expect("sheetPr");
    assert_eq!(pr.attr("published"), Some("0"));
    let kids: Vec<&str> = pr.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(kids, ["tabColor", "outlinePr", "pageSetUpPr"]);
}

#[test]
fn sheet_pr_flags_at_their_defaults_are_not_written() {
    let sheet = saved(r#"<sheetPr published="1"><outlinePr showOutlineSymbols="1"/></sheetPr>"#, "");
    assert!(sheet.child("sheetPr").is_none(), "nothing but defaults: no sheetPr");

    let sheet = saved(r#"<sheetPr><outlinePr summaryBelow="0"/></sheetPr>"#, "");
    let pr = sheet.child("sheetPr").expect("sheetPr");
    assert_eq!(pr.attr("published"), None);
    let o = pr.child("outlinePr").expect("outlinePr");
    assert_eq!((o.attr("summaryBelow"), o.attr("showOutlineSymbols")), (Some("0"), None));
}

/// `sheetView showOutlineSymbols` (§18.3.1.87) is a separate setting: neither one sets the other.
#[test]
fn sheet_pr_flags_outline_symbols_are_independent_of_the_view_flag() {
    let view_off = r#"<sheetViews><sheetView showOutlineSymbols="0" workbookViewId="0"/></sheetViews>"#;
    let sheet = saved("", view_off);
    assert!(sheet.child("sheetPr").is_none(), "the view flag alone writes no sheetPr");
    let view = sheet.child("sheetViews").and_then(|v| v.child("sheetView")).expect("sheetView");
    assert_eq!(view.attr("showOutlineSymbols"), Some("0"));

    let body = r#"<sheetPr><outlinePr showOutlineSymbols="0"/></sheetPr><sheetData/>"#;
    let wb = read_xlsx(&minimal(body, &[], "", "")).unwrap().0;
    assert!(wb.sheets[0].show_outline_symbols, "outlinePr does not hide the view's outline symbols");
    let sheet = saved(r#"<sheetPr><outlinePr showOutlineSymbols="0"/></sheetPr>"#, "");
    let view = sheet.child("sheetViews").and_then(|v| v.child("sheetView")).expect("sheetView");
    assert_eq!(view.attr("showOutlineSymbols"), None);
}
