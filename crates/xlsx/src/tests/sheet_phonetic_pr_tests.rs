//! A worksheet's own phonetic (furigana) settings survive an open + save.
//!
//! Behaviour reference: ISO/IEC 29500-1 §18.4.3 `phoneticPr` as a child of `CT_Worksheet`
//! (§18.3.1.99), placed after `mergeCells` and before `conditionalFormatting`; attributes as
//! §18.4.7 `CT_PhoneticPr` (`fontId` required, `type` §18.18.57, `alignment` §18.18.56).
//! Microsoft Excel keeps the element, with the same values, when it re-saves a workbook.

use super::minimal;
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

const DATA: &str = r#"<sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row></sheetData>"#;
const MARGINS: &str = r#"<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>"#;

/// The saved `xl/worksheets/sheet1.xml` after reading a one-sheet package with `sheet_body`.
fn saved_sheet(sheet_body: &str) -> String {
    let (wb, _) = read_xlsx(&minimal(sheet_body, &[], "", "")).unwrap();
    sheet_part(&write_xlsx(&wb).unwrap())
}

fn sheet_part(bytes: &[u8]) -> String {
    let mut pkg = Package::open(bytes).unwrap();
    String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
}

/// The `<phoneticPr .../>` elements of a sheet part.
fn phonetic_elements(xml: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<phoneticPr") {
        let tail = &rest[i..];
        let end = tail.find('>').map(|e| e + 1).unwrap_or(tail.len());
        found.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    found
}

fn pos(xml: &str, needle: &str) -> usize {
    xml.find(needle).unwrap_or_else(|| panic!("{needle} not in saved sheet: {xml}"))
}

#[test]
fn sheet_phonetic_pr_with_type_is_kept() {
    let xml = saved_sheet(&format!(r#"{DATA}<phoneticPr fontId="1" type="noConversion"/>{MARGINS}"#));
    assert_eq!(phonetic_elements(&xml), vec![r#"<phoneticPr fontId="1" type="noConversion"/>"#.to_string()], "{xml}");
}

#[test]
fn sheet_phonetic_pr_with_font_only_is_kept() {
    let xml = saved_sheet(&format!(r#"{DATA}<phoneticPr fontId="1"/>{MARGINS}"#));
    assert_eq!(phonetic_elements(&xml), vec![r#"<phoneticPr fontId="1"/>"#.to_string()], "{xml}");
}

#[test]
fn sheet_phonetic_pr_alignment_is_kept_in_schema_attribute_order() {
    let xml = saved_sheet(&format!(r#"{DATA}<phoneticPr alignment="distributed" type="Hiragana" fontId="0"/>{MARGINS}"#));
    assert_eq!(phonetic_elements(&xml), vec![r#"<phoneticPr fontId="0" type="Hiragana" alignment="distributed"/>"#.to_string()], "{xml}");
}

#[test]
fn sheet_phonetic_pr_unknown_values_are_dropped_and_missing_font_is_zero() {
    let xml = saved_sheet(&format!(r#"{DATA}<phoneticPr type="bogus" alignment="middle"/>{MARGINS}"#));
    assert_eq!(phonetic_elements(&xml), vec![r#"<phoneticPr fontId="0"/>"#.to_string()], "{xml}");
}

#[test]
fn sheet_phonetic_pr_goes_after_merge_cells_and_before_conditional_formats() {
    let body = format!(
        r#"{DATA}<mergeCells count="1"><mergeCell ref="C3:D4"/></mergeCells><phoneticPr fontId="2" type="noConversion"/><conditionalFormatting sqref="A1:B1"><cfRule type="colorScale" priority="1"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FF63BE7B"/></colorScale></cfRule></conditionalFormatting><dataValidations count="1"><dataValidation type="whole" operator="between" allowBlank="1" sqref="E1"><formula1>1</formula1><formula2>9</formula2></dataValidation></dataValidations>{MARGINS}"#
    );
    let xml = saved_sheet(&body);
    assert_eq!(phonetic_elements(&xml), vec![r#"<phoneticPr fontId="2" type="noConversion"/>"#.to_string()], "{xml}");
    let at = pos(&xml, "<phoneticPr");
    assert!(pos(&xml, "</mergeCells>") < at, "phoneticPr must follow mergeCells: {xml}");
    assert!(at < pos(&xml, "<conditionalFormatting"), "phoneticPr must precede conditionalFormatting: {xml}");
    assert!(at < pos(&xml, "<dataValidations"), "phoneticPr must precede dataValidations: {xml}");
}

#[test]
fn sheet_phonetic_pr_without_neighbours_sits_between_sheet_data_and_page_margins() {
    let xml = saved_sheet(&format!(r#"{DATA}<phoneticPr fontId="1" type="noConversion"/>{MARGINS}"#));
    let at = pos(&xml, "<phoneticPr");
    assert!(pos(&xml, "</sheetData>") < at, "{xml}");
    assert!(at < pos(&xml, "<pageMargins"), "{xml}");
}

#[test]
fn sheet_phonetic_pr_survives_a_second_save() {
    let (wb, _) =
        read_xlsx(&minimal(&format!(r#"{DATA}<phoneticPr fontId="3" type="fullwidthKatakana" alignment="center"/>{MARGINS}"#), &[], "", "")).unwrap();
    let first = write_xlsx(&wb).unwrap();
    let (again, _) = read_xlsx(&first).unwrap();
    let second = sheet_part(&write_xlsx(&again).unwrap());
    assert_eq!(phonetic_elements(&second), vec![r#"<phoneticPr fontId="3" type="fullwidthKatakana" alignment="center"/>"#.to_string()], "{second}");
}

/// Guard: a sheet without the element gets none, and the neighbours the ordering test relies on
/// are written at all.
#[test]
fn sheet_without_phonetic_pr_writes_none() {
    let body = format!(
        r#"{DATA}<mergeCells count="1"><mergeCell ref="C3:D4"/></mergeCells><conditionalFormatting sqref="A1:B1"><cfRule type="colorScale" priority="1"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FF63BE7B"/></colorScale></cfRule></conditionalFormatting><dataValidations count="1"><dataValidation type="whole" operator="between" allowBlank="1" sqref="E1"><formula1>1</formula1><formula2>9</formula2></dataValidation></dataValidations>{MARGINS}"#
    );
    let xml = saved_sheet(&body);
    assert!(phonetic_elements(&xml).is_empty(), "{xml}");
    for needle in ["</mergeCells>", "<conditionalFormatting", "<dataValidations", "<pageMargins"] {
        pos(&xml, needle);
    }
}
