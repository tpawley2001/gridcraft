//! A worksheet's saved sort settings (`<sortState>`, ECMA-376 Part 1 §18.3.1.92) are kept on
//! save, both as a direct child of `<worksheet>` (CT_Worksheet, after `<autoFilter>`) and nested
//! in the sheet's `<autoFilter>` (CT_AutoFilter), as Excel keeps them.

use super::minimal;
use crate::{read_xlsx, write_xlsx};

/// The first sheet's XML as written.
fn sheet_xml(bytes: &[u8]) -> String {
    String::from_utf8(crate::package::Package::open(bytes).unwrap().read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
}

const DATA: &str = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Key</t></is></c><c r="B1" t="inlineStr"><is><t>Val</t></is></c></row><row r="2"><c r="A2"><v>3</v></c><c r="B2"><v>30</v></c></row><row r="3"><c r="A3"><v>2</v></c><c r="B3"><v>20</v></c></row><row r="4"><c r="A4"><v>1</v></c><c r="B4"><v>10</v></c></row></sheetData>"#;
const MERGE: &str = r#"<mergeCells count="1"><mergeCell ref="D1:E1"/></mergeCells>"#;

/// Saves the sheet built from `after_data` (XML placed after `<sheetData>`), then saves our own
/// output again; returns both written sheet XMLs.
fn round_trip(after_data: &str) -> (String, String) {
    let body = format!("{DATA}{after_data}");
    let (wb, _) = read_xlsx(&minimal(&body, &[], "", "")).unwrap();
    let first = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&first).unwrap();
    let second = write_xlsx(&back).unwrap();
    (sheet_xml(&first), sheet_xml(&second))
}

fn pos(xml: &str, needle: &str) -> usize {
    xml.find(needle).unwrap_or_else(|| panic!("no {needle} in {xml}"))
}

#[test]
fn worksheet_level_sort_state_is_kept_before_merge_cells() {
    let sort = r#"<sortState ref="A2:B4"><sortCondition descending="1" ref="A2:A4"/></sortState>"#;
    let (first, second) = round_trip(&format!("{sort}{MERGE}"));
    for xml in [&first, &second] {
        let at = pos(xml, sort);
        assert!(at < pos(xml, "<mergeCells"), "sortState must come before mergeCells: {xml}");
        assert!(!xml[pos(xml, "<sheetData")..at].contains("<autoFilter"), "{xml}");
        assert_eq!(xml.matches("<sortState").count(), 1, "{xml}");
    }
}

#[test]
fn worksheet_level_sort_state_after_the_autofilter_keeps_its_options() {
    let sort = r#"<sortState columnSort="1" caseSensitive="1" sortMethod="stroke" ref="A1:B4"><sortCondition ref="A1:B1" customList="Lo,Mid,Hi"/></sortState>"#;
    let (first, second) = round_trip(&format!(r#"<autoFilter ref="A1:B4"/>{sort}{MERGE}"#));
    for xml in [&first, &second] {
        let af = pos(xml, "<autoFilter");
        let at = pos(xml, sort);
        assert!(af < at && at < pos(xml, "<mergeCells"), "order autoFilter, sortState, mergeCells: {xml}");
        // Directly under <worksheet>, not inside the AutoFilter.
        let af_xml = &xml[af..];
        let af_end = af_xml.find("</autoFilter>").map(|e| af + e).unwrap_or_else(|| af + af_xml.find("/>").unwrap());
        assert!(af_end < at, "sortState written inside the autoFilter: {xml}");
    }
}

#[test]
fn sort_state_nested_in_the_sheet_autofilter_is_kept_inside_it() {
    let filter = r#"<filterColumn colId="1"><filters><filter val="20"/></filters></filterColumn>"#;
    let sort = r#"<sortState ref="A2:B4"><sortCondition ref="B2:B4"/></sortState>"#;
    let (first, second) = round_trip(&format!(r#"<autoFilter ref="A1:B4">{filter}{sort}</autoFilter>"#));
    for xml in [&first, &second] {
        let af = pos(xml, r#"<autoFilter ref="A1:B4""#);
        let end = pos(xml, "</autoFilter>");
        let inner = &xml[af..end];
        assert!(inner.contains(r#"<filter val="20"/>"#), "{xml}");
        assert!(inner.ends_with(sort), "sortState must be the autoFilter's last child: {xml}");
        assert_eq!(xml.matches("<sortState").count(), 1, "{xml}");
    }
}

#[test]
fn nested_sort_state_is_dropped_once_the_autofilter_is_removed() {
    let sort = r#"<sortState ref="A2:B4"><sortCondition ref="B2:B4"/></sortState>"#;
    let body = format!(r#"{DATA}<autoFilter ref="A1:B4">{sort}</autoFilter>"#);
    let (mut wb, _) = read_xlsx(&minimal(&body, &[], "", "")).unwrap();
    // Guard against the test passing trivially: the sort is written while the filter stays.
    let kept = sheet_xml(&write_xlsx(&wb).unwrap());
    assert!(kept.contains(sort), "{kept}");
    wb.sheet_mut(0).unwrap().autofilter = None;
    let xml = sheet_xml(&write_xlsx(&wb).unwrap());
    assert!(!xml.contains("<sortState"), "{xml}");
    assert!(!xml.contains("<autoFilter"), "{xml}");
}

#[test]
fn sheet_without_sort_state_writes_none() {
    let (first, _) = round_trip(r#"<autoFilter ref="A1:B4"/>"#);
    assert!(!first.contains("<sortState"), "{first}");
}
