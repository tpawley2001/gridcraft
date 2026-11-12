//! `<sheetView>` flags `showRuler`, `showWhiteSpace` and `windowProtection` (ECMA-376 Part 1
//! §18.3.1.87) and `<printOptions gridLinesSet>` (§18.3.1.70) survive open + save. Packages are
//! built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

/// Open + save a one-sheet workbook whose `<sheetView>` carries `view_attrs` and whose sheet has
/// `after_data` after `<sheetData>`; the saved worksheet.
fn saved_sheet(view_attrs: &str, after_data: &str) -> El {
    let body = format!(
        r#"<sheetViews><sheetView{view_attrs} workbookViewId="0"/></sheetViews><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>{after_data}"#
    );
    let out = write_xlsx(&read_xlsx(&minimal(&body, &[], "", "")).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    xml::parse(s.as_bytes()).unwrap()
}

fn saved_view(view_attrs: &str) -> El {
    saved_sheet(view_attrs, "").child("sheetViews").expect("sheetViews").child("sheetView").expect("sheetView").clone()
}

/// A boolean attribute as written: `1`/`true` and `0`/`false`; absent = `default`.
fn flag(e: &El, name: &str, default: bool) -> bool {
    match e.attr(name) {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        Some(other) => panic!("{name}={other:?}"),
        None => default,
    }
}

#[test]
fn a_hidden_ruler_is_kept() {
    let v = saved_view(r#" showRuler="0" view="pageLayout""#);
    assert!(!flag(&v, "showRuler", true));
    assert!(flag(&v, "showWhiteSpace", true));
    assert_eq!(v.attr("view"), Some("pageLayout"));
}

#[test]
fn a_hidden_ruler_is_kept_in_normal_view() {
    // Excel keeps the Page Layout ruler setting while the sheet is in Normal view.
    let v = saved_view(r#" tabSelected="1" showRuler="0""#);
    assert!(!flag(&v, "showRuler", true));
}

#[test]
fn hidden_white_space_is_kept() {
    let v = saved_view(r#" showWhiteSpace="0" view="pageLayout""#);
    assert!(!flag(&v, "showWhiteSpace", true));
    assert!(flag(&v, "showRuler", true));
}

#[test]
fn window_protection_is_kept() {
    let v = saved_view(r#" windowProtection="1" tabSelected="1" view="pageLayout""#);
    assert!(flag(&v, "windowProtection", false));
    assert_eq!(v.attr("view"), Some("pageLayout"));
}

#[test]
fn default_view_flags_are_not_written() {
    for attrs in ["", r#" showRuler="1" showWhiteSpace="1" windowProtection="0""#] {
        let v = saved_view(attrs);
        for a in ["showRuler", "showWhiteSpace", "windowProtection"] {
            assert_eq!(v.attr(a), None, "{a} after {attrs:?}");
        }
    }
}

#[test]
fn view_flags_are_stable_over_a_second_save() {
    let first = saved_view(r#" windowProtection="1" showRuler="0" showWhiteSpace="0" view="pageLayout""#);
    let mut attrs = String::new();
    for a in ["windowProtection", "showRuler", "showWhiteSpace", "view"] {
        if let Some(val) = first.attr(a) {
            attrs.push_str(&format!(r#" {a}="{val}""#));
        }
    }
    let second = saved_view(&attrs);
    assert!(flag(&second, "windowProtection", false));
    assert!(!flag(&second, "showRuler", true));
    assert!(!flag(&second, "showWhiteSpace", true));
    assert_eq!(second.attr("view"), Some("pageLayout"));
}

#[test]
fn grid_lines_set_off_is_kept_next_to_grid_lines() {
    let sheet = saved_sheet("", r#"<printOptions gridLines="1" gridLinesSet="0"/>"#);
    let p = sheet.child("printOptions").expect("printOptions");
    assert!(flag(p, "gridLines", false));
    assert!(!flag(p, "gridLinesSet", true));
}

#[test]
fn grid_lines_set_off_alone_is_kept() {
    let sheet = saved_sheet("", r#"<printOptions gridLinesSet="0"/>"#);
    let p = sheet.child("printOptions").expect("printOptions");
    assert!(!flag(p, "gridLinesSet", true));
    assert!(!flag(p, "gridLines", false));
}

#[test]
fn grid_lines_set_default_is_not_written() {
    let sheet = saved_sheet("", r#"<printOptions gridLines="1" gridLinesSet="1"/>"#);
    let p = sheet.child("printOptions").expect("printOptions");
    assert!(flag(p, "gridLines", false));
    assert_eq!(p.attr("gridLinesSet"), None);
    assert!(saved_sheet("", "").child("printOptions").is_none());
}
