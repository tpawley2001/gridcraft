//! A table's saved `<sortState>`: missing, hostile and stale cases.

use super::minimal;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const SHEET_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="t1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#;
const BODY: &str = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row><row r="2"><c r="B2"><v>2</v></c></row><row r="3"><c r="B3"><v>1</v></c></row></sheetData><tableParts count="1"><tablePart r:id="t1"/></tableParts>"#;
const STYLES: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dxfs count="1"><dxf><fill><patternFill patternType="solid"><bgColor rgb="FFFF0000"/></patternFill></fill></dxf></dxfs></styleSheet>"#;
const STYLES_REL: &str =
    r#"<Relationship Id="st" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;

fn book(sort: &str) -> Vec<u8> {
    let table = format!(
        r#"<table xmlns="{MAIN}" id="1" name="Sales" displayName="Sales" ref="A1:B3" totalsRowShown="0"><autoFilter ref="A1:B3"/>{sort}<tableColumns count="2"><tableColumn id="1" name="Name"/><tableColumn id="2" name="Amount"/></tableColumns></table>"#
    );
    minimal(BODY, &[("xl/worksheets/_rels/sheet1.xml.rels", SHEET_RELS), ("xl/tables/table1.xml", &table), ("xl/styles.xml", STYLES)], STYLES_REL, "")
}

fn table_part(bytes: &[u8]) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut b = Vec::new();
    z.by_name("xl/tables/table1.xml").unwrap().read_to_end(&mut b).unwrap();
    parse(&b).unwrap()
}

fn sort_of(xml: &El) -> Option<&El> {
    xml.child("sortState").or_else(|| xml.path(&["autoFilter", "sortState"]))
}

#[test]
fn missing_sort_state_writes_none() {
    let (wb, _) = read_xlsx(&book("")).unwrap();
    assert!(wb.sheets[0].tables[0].sort.is_none());
    let xml = table_part(&write_xlsx(&wb).unwrap());
    assert!(sort_of(&xml).is_none());
}

#[test]
fn malformed_refs_are_dropped_without_panic() {
    let (wb, _) = read_xlsx(&book(r#"<sortState ref="not a range"><sortCondition ref="B2:B3"/></sortState>"#)).unwrap();
    assert!(wb.sheets[0].tables[0].sort.is_none());
    let (wb, _) =
        read_xlsx(&book(r#"<sortState ref="A2:B3"><sortCondition ref="??"/><sortCondition ref="B2:B3" descending="1"/></sortState>"#)).unwrap();
    let xml = table_part(&write_xlsx(&wb).unwrap());
    let s = sort_of(&xml).unwrap();
    let c: Vec<_> = s.kids("sortCondition").collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].attr("ref"), Some("B2:B3"));
}

#[test]
fn sort_conditions_are_capped() {
    let conds: String = (0..500).map(|_| r#"<sortCondition ref="B2:B3"/>"#).collect();
    let (wb, report) = read_xlsx(&book(&format!(r#"<sortState ref="A2:B3">{conds}</sortState>"#))).unwrap();
    assert_eq!(wb.sheets[0].tables[0].sort.as_ref().unwrap().conditions.len(), gridcraft_model::MAX_SORT_CONDITIONS);
    assert!(!report.warnings.is_empty());
}

#[test]
fn colour_sort_with_missing_format_is_dropped_with_one_warning() {
    let (wb, report) = read_xlsx(&book(
        r#"<sortState ref="A2:B3"><sortCondition sortBy="cellColor" dxfId="7" ref="A2:A3"/><sortCondition sortBy="fontColor" dxfId="9" ref="A2:A3"/><sortCondition ref="B2:B3"/></sortState>"#,
    ))
    .unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.contains("colour sort")).count(), 1);
    let xml = table_part(&write_xlsx(&wb).unwrap());
    let c: Vec<_> = sort_of(&xml).unwrap().kids("sortCondition").collect();
    assert_eq!(c.len(), 1);
    assert!(c[0].attr("dxfId").is_none());
}

#[test]
fn colour_sort_format_is_remapped() {
    let (wb, _) = read_xlsx(&book(r#"<sortState ref="A2:B3"><sortCondition sortBy="cellColor" dxfId="0" ref="A2:A3"/></sortState>"#)).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let c = sort_of(&table_part(&bytes)).unwrap().child("sortCondition").unwrap().clone();
    assert_eq!(c.attr("sortBy"), Some("cellColor"));
    let id: usize = c.attr("dxfId").unwrap().parse().unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap();
    let mut b = Vec::new();
    z.by_name("xl/styles.xml").unwrap().read_to_end(&mut b).unwrap();
    let styles = parse(&b).unwrap();
    assert!(styles.child("dxfs").unwrap().kids("dxf").nth(id).is_some());
}

#[test]
fn resized_table_drops_stale_sort() {
    let (mut wb, _) = read_xlsx(&book(r#"<sortState ref="A2:B3"><sortCondition ref="B2:B3"/></sortState>"#)).unwrap();
    let t = &mut std::sync::Arc::make_mut(&mut wb.sheets[0]).tables[0];
    t.range = gridcraft_core::RangeRef::parse("A1:B5").unwrap();
    let xml = table_part(&write_xlsx(&wb).unwrap());
    assert!(sort_of(&xml).is_none());
}
