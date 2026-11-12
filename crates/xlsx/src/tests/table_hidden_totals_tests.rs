//! A table whose Total Row is switched off keeps each column's remembered total function,
//! label and custom formula (`tableColumn totalsRowFunction`, `totalsRowLabel`,
//! `totalsRowFormula`; ECMA-376 Part 1 §18.5.1.3) through open + save, as Excel writes them.
//! Packages are built in code.

use std::io::Read;

use gridcraft_model::TotalsFn;

use super::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// A 2-column table (header row, data rows 2-3) with the given `<table>` attributes after
/// `displayName` and the given two `<tableColumn>` elements. `total_row` adds row 4.
fn package(table_attrs: &str, columns: &str, total_row: bool) -> Vec<u8> {
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/><Override PartName="/xl/tables/table1.xml" ContentType="{CT}.table+xml"/></Types>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let total = if total_row { r#"<row r="4"><c r="A4" t="inlineStr"><is><t>Total</t></is></c><c r="B4"><v>3</v></c></row>"# } else { "" };
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>City</t></is></c><c r="B1" t="inlineStr"><is><t>Population</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2"><v>1</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>b</t></is></c><c r="B3"><v>2</v></c></row>{total}</sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts></worksheet>"#
    );
    let last = if total_row { 4 } else { 3 };
    let table = format!(
        r#"<?xml version="1.0"?><table xmlns="{NS_MAIN}" id="1" name="T" displayName="T" ref="A1:B{last}"{table_attrs}><autoFilter ref="A1:B3"/><tableColumns count="2">{columns}</tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#
    );
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml")]);
    let sheet_rels = rels(&[("rId1", "table", "../tables/table1.xml")]);
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/tables/table1.xml", table.as_bytes()),
    ])
}

/// The Total Row is off: Excel writes `totalsRowShown="0"` and no `totalsRowCount`.
fn hidden(columns: &str) -> Vec<u8> {
    package(r#" totalsRowShown="0""#, columns, false)
}

/// The saved `<table>` root.
fn saved_table(pkg: &[u8]) -> El {
    let out = write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/tables/table")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    xml::parse(s.as_bytes()).unwrap()
}

fn saved_columns(table: &El) -> Vec<&El> {
    table.child("tableColumns").expect("tableColumns").kids("tableColumn").collect()
}

const LABEL_AND_SUM: &str =
    r#"<tableColumn id="1" name="City" totalsRowLabel="Total"/><tableColumn id="2" name="Population" totalsRowFunction="sum"/>"#;

#[test]
fn table_hidden_totals_are_read_into_the_model() {
    let (wb, _) = read_xlsx(&hidden(LABEL_AND_SUM)).unwrap();
    let t = &wb.sheets[0].tables[0];
    assert!(!t.totals_row, "the Total Row is off");
    assert_eq!(t.columns[0].totals, TotalsFn::None);
    assert_eq!(t.columns[0].totals_label.as_deref(), Some("Total"));
    assert_eq!(t.columns[1].totals, TotalsFn::Sum);
}

#[test]
fn table_hidden_totals_label_and_function_are_written_back() {
    let table = saved_table(&hidden(LABEL_AND_SUM));
    assert_eq!(table.attr("totalsRowShown"), Some("0"), "the Total Row stays off");
    assert_eq!(table.attr("totalsRowCount"), None, "the Total Row stays off");
    assert_eq!(table.attr("ref"), Some("A1:B3"));
    let cols = saved_columns(&table);
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].attr("totalsRowLabel"), Some("Total"));
    assert_eq!(cols[0].attr("totalsRowFunction"), None);
    assert_eq!(cols[1].attr("totalsRowFunction"), Some("sum"));
    assert_eq!(cols[1].attr("totalsRowLabel"), None);
}

#[test]
fn table_hidden_totals_keep_every_function() {
    for function in ["average", "count", "countNums", "max", "min", "stdDev", "sum", "var"] {
        let columns = format!(r#"<tableColumn id="1" name="City"/><tableColumn id="2" name="Population" totalsRowFunction="{function}"/>"#);
        let table = saved_table(&hidden(&columns));
        let cols = saved_columns(&table);
        assert_eq!(cols[1].attr("totalsRowFunction"), Some(function), "{function}");
        assert_eq!(cols[0].attr("totalsRowFunction"), None, "{function}: the other column gains nothing");
        assert_eq!(cols[0].attr("totalsRowLabel"), None, "{function}: the other column gains nothing");
        assert_eq!(table.attr("totalsRowCount"), None, "{function}: the Total Row stays off");
    }
}

#[test]
fn table_hidden_totals_keep_a_custom_formula() {
    let columns = r#"<tableColumn id="1" name="City" totalsRowLabel="All"/><tableColumn id="2" name="Population" totalsRowFunction="custom"><totalsRowFormula>1+1</totalsRowFormula></tableColumn>"#;
    let table = saved_table(&hidden(columns));
    let cols = saved_columns(&table);
    assert_eq!(cols[0].attr("totalsRowLabel"), Some("All"));
    assert_eq!(cols[1].attr("totalsRowFunction"), Some("custom"));
    assert_eq!(cols[1].child("totalsRowFormula").map(|f| f.text.trim()), Some("1+1"));
    assert_eq!(table.attr("totalsRowCount"), None, "the Total Row stays off");
}

#[test]
fn table_hidden_totals_survive_a_second_round_trip() {
    let first = write_xlsx(&read_xlsx(&hidden(LABEL_AND_SUM)).unwrap().0).unwrap();
    let (wb, _) = read_xlsx(&first).unwrap();
    let t = &wb.sheets[0].tables[0];
    assert!(!t.totals_row);
    assert_eq!(t.columns[0].totals_label.as_deref(), Some("Total"));
    assert_eq!(t.columns[1].totals, TotalsFn::Sum);
    let table = saved_table(&first);
    let cols = saved_columns(&table);
    assert_eq!(cols[0].attr("totalsRowLabel"), Some("Total"));
    assert_eq!(cols[1].attr("totalsRowFunction"), Some("sum"));
}

#[test]
fn table_hidden_totals_absent_stay_absent() {
    let table = saved_table(&hidden(r#"<tableColumn id="1" name="City"/><tableColumn id="2" name="Population"/>"#));
    assert_eq!(table.attr("totalsRowShown"), Some("0"));
    for col in saved_columns(&table) {
        assert_eq!(col.attr("totalsRowFunction"), None);
        assert_eq!(col.attr("totalsRowLabel"), None);
        assert!(col.child("totalsRowFormula").is_none());
    }
}

#[test]
fn table_shown_totals_are_written_as_before() {
    let table = saved_table(&package(r#" totalsRowCount="1""#, LABEL_AND_SUM, true));
    assert_eq!(table.attr("totalsRowCount"), Some("1"));
    assert_eq!(table.attr("ref"), Some("A1:B4"));
    let cols = saved_columns(&table);
    assert_eq!(cols[0].attr("totalsRowLabel"), Some("Total"));
    assert_eq!(cols[1].attr("totalsRowFunction"), Some("sum"));
}
