//! `<pageSetup fitToWidth fitToHeight>` (ECMA-376 Part 1 §18.3.1.63) kept while Fit To is not
//! selected (`pageSetUpPr fitToPage` absent, §18.3.1.65), and `pageSetUpPr autoPageBreaks="0"`
//! kept, through open + save. Packages are built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

/// What open + save leaves of the page setup.
struct Saved {
    /// (fitToWidth, fitToHeight) of `<pageSetup>`; an absent attribute (or element) is 1.
    fit: (u32, u32),
    /// `<pageSetup scale>`; absent is 100.
    scale: u32,
    /// `<sheetPr><pageSetUpPr fitToPage>`; absent is false.
    fit_to_page: bool,
    /// `<sheetPr><pageSetUpPr autoPageBreaks>`; absent is true.
    auto_page_breaks: bool,
}

fn saved(sheet_pr: &str, page_setup: &str) -> Saved {
    let body = format!(r#"{sheet_pr}<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>{page_setup}"#);
    let out = write_xlsx(&read_xlsx(&minimal(&body, &[], "", "")).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let sheet = xml::parse(s.as_bytes()).unwrap();
    let setup: Option<&El> = sheet.child("pageSetup");
    let num = |a: &str, default: u32| setup.and_then(|e| e.attr_u32(a)).unwrap_or(default);
    let pr: Option<&El> = sheet.child("sheetPr").and_then(|p| p.child("pageSetUpPr"));
    Saved {
        fit: (num("fitToWidth", 1), num("fitToHeight", 1)),
        scale: num("scale", 100),
        fit_to_page: pr.is_some_and(|p| p.flag("fitToPage", false)),
        auto_page_breaks: pr.is_none_or(|p| p.flag("autoPageBreaks", true)),
    }
}

#[test]
fn fit_to_stored_automatic_height_kept_while_fit_is_off() {
    // As Excel saves a sheet printed at 80% whose Fit To boxes read "1 wide by (automatic) tall".
    let s = saved("", r#"<pageSetup paperSize="9" scale="80" fitToHeight="0" orientation="portrait"/>"#);
    assert_eq!(s.fit, (1, 0));
    assert!(!s.fit_to_page, "Fit To stays off");
    assert_eq!(s.scale, 80);
}

#[test]
fn fit_to_stored_page_counts_kept_while_fit_is_off() {
    let s = saved("", r#"<pageSetup fitToWidth="2" fitToHeight="3" orientation="landscape"/>"#);
    assert_eq!(s.fit, (2, 3));
    assert!(!s.fit_to_page, "Fit To stays off");
    let s = saved(r#"<sheetPr><pageSetUpPr fitToPage="0"/></sheetPr>"#, r#"<pageSetup fitToWidth="4"/>"#);
    assert_eq!(s.fit, (4, 1));
    assert!(!s.fit_to_page, "Fit To stays off");
}

#[test]
fn fit_to_stored_fit_on_is_unchanged() {
    let s = saved(r#"<sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>"#, r#"<pageSetup fitToHeight="0"/>"#);
    assert_eq!(s.fit, (1, 0));
    assert!(s.fit_to_page);
    assert!(s.auto_page_breaks);
}

#[test]
fn fit_to_stored_plain_sheet_writes_no_fit() {
    let s = saved("", r#"<pageSetup orientation="landscape"/>"#);
    assert_eq!(s.fit, (1, 1));
    assert!(!s.fit_to_page);
    assert!(s.auto_page_breaks);
}

#[test]
fn fit_to_stored_auto_page_breaks_off_kept() {
    let s = saved(r#"<sheetPr><pageSetUpPr autoPageBreaks="0"/></sheetPr>"#, "");
    assert!(!s.auto_page_breaks, "autoPageBreaks=\"0\" kept");
    assert!(!s.fit_to_page);
    let s = saved(r#"<sheetPr><pageSetUpPr autoPageBreaks="0" fitToPage="1"/></sheetPr>"#, r#"<pageSetup fitToWidth="1" fitToHeight="4"/>"#);
    assert!(!s.auto_page_breaks, "autoPageBreaks=\"0\" kept next to fitToPage");
    assert!(s.fit_to_page);
    assert_eq!(s.fit, (1, 4));
}
