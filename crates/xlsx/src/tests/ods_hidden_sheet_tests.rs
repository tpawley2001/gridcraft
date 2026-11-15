//! ODS sheets hidden with `table:display="false"` on their table style open hidden.
//! Original ODS packages assembled in Rust, not files from another spreadsheet application.

use gridcraft_model::Visibility;

use super::make_zip;
use crate::{read_ods, read_xlsx, write_xlsx};

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;

/// `styles`: the `office:automatic-styles` children; `tables`: the `office:spreadsheet` children.
fn ods(styles: &str, tables: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:spreadsheet>{tables}</office:spreadsheet></office:body></office:document-content>"#
    );
    make_zip(&[("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_bytes())])
}

fn table_style(name: &str, display: Option<&str>) -> String {
    let props = match display {
        Some(d) => format!(r#"<style:table-properties table:display="{d}" style:writing-mode="lr-tb"/>"#),
        None => r#"<style:table-properties style:writing-mode="lr-tb"/>"#.to_string(),
    };
    format!(r#"<style:style style:name="{name}" style:family="table" style:master-page-name="Default">{props}</style:style>"#)
}

fn sheet(name: &str, style: Option<&str>, text: &str) -> String {
    let style = style.map(|s| format!(r#" table:style-name="{s}""#)).unwrap_or_default();
    format!(
        r#"<table:table table:name="{name}"{style}><table:table-row><table:table-cell office:value-type="string"><text:p>{text}</text:p></table:table-cell></table:table-row></table:table>"#
    )
}

fn visibility(bytes: &[u8]) -> Vec<Visibility> {
    let (wb, _) = read_ods(bytes).expect("ODS opens");
    wb.sheets.iter().map(|s| s.visibility).collect()
}

#[test]
fn display_false_hides_the_sheet() {
    let styles = table_style("ta1", Some("true")) + &table_style("ta2", Some("false"));
    let bytes = ods(&styles, &(sheet("Shown", Some("ta1"), "a") + &sheet("Gone", Some("ta2"), "b")));
    assert_eq!(visibility(&bytes), vec![Visibility::Visible, Visibility::Hidden]);
}

#[test]
fn no_style_no_display_attribute_or_unknown_style_stays_visible() {
    let styles = table_style("ta1", None);
    let tables = sheet("NoStyle", None, "a") + &sheet("NoAttr", Some("ta1"), "b") + &sheet("Unknown", Some("ta9"), "c");
    assert_eq!(visibility(&ods(&styles, &tables)), vec![Visibility::Visible; 3]);
}

#[test]
fn two_sheets_sharing_a_hidden_style_are_both_hidden() {
    let styles = table_style("ta1", Some("true")) + &table_style("ta2", Some("false"));
    let tables = sheet("A", Some("ta1"), "a") + &sheet("B", Some("ta2"), "b") + &sheet("C", Some("ta2"), "c");
    assert_eq!(visibility(&ods(&styles, &tables)), vec![Visibility::Visible, Visibility::Hidden, Visibility::Hidden]);
}

/// A workbook needs one visible sheet (Excel refuses to hide the last one): the first sheet stays visible.
#[test]
fn every_sheet_hidden_keeps_the_first_visible() {
    let styles = table_style("ta2", Some("false"));
    let tables = sheet("A", Some("ta2"), "a") + &sheet("B", Some("ta2"), "b");
    assert_eq!(visibility(&ods(&styles, &tables)), vec![Visibility::Visible, Visibility::Hidden]);
}

/// The active sheet is never a hidden one: it moves to the first visible sheet.
#[test]
fn hidden_first_sheet_is_not_active() {
    let styles = table_style("ta1", Some("true")) + &table_style("ta2", Some("false"));
    let tables = sheet("Gone", Some("ta2"), "a") + &sheet("Shown", Some("ta1"), "b");
    let (wb, _) = read_ods(&ods(&styles, &tables)).expect("ODS opens");
    assert_eq!(wb.sheets[0].visibility, Visibility::Hidden);
    assert_eq!(wb.active_sheet(), 1);
}

#[test]
fn hidden_ods_sheet_saves_hidden_in_xlsx() {
    let styles = table_style("ta1", Some("true")) + &table_style("ta2", Some("false"));
    let bytes = ods(&styles, &(sheet("Shown", Some("ta1"), "a") + &sheet("Gone", Some("ta2"), "b")));
    let (wb, _) = read_ods(&bytes).expect("ODS opens");
    let xlsx = write_xlsx(&wb).expect("saves as xlsx");
    let (back, _) = read_xlsx(&xlsx).expect("xlsx reopens");
    assert_eq!(back.sheets.iter().map(|s| s.visibility).collect::<Vec<_>>(), vec![Visibility::Visible, Visibility::Hidden]);
}
