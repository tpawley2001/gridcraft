//! XLSB AutoFilter colour (`BrtColorFilter`) and icon (`BrtIconFilter`) criteria, on a sheet
//! AutoFilter and inside a table part, matched against the .xlsx reader by round trip.

use gridcraft_model::{Color, FilterCriterion, Workbook};

use super::make_zip;
use super::xlsb_rules_tests::{fill_dxf, read, rgb, styles};
use super::xlsb_tables_tests::{autofilter, book, filter_column, list_parts, record, sales_cells, sales_table, table_rels, u32s, worksheet};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

const MALFORMED: &str = "Malformed XLSB AutoFilter criteria were skipped.";
const UNSUPPORTED: &str = "XLSB colour and icon filters in AutoFilters are not imported.";

fn color_filter(dxf: u32, cell: u32) -> Vec<u8> {
    record(168, &u32s(&[dxf, cell]))
}
fn icon_filter(set: u32, icon: u32) -> Vec<u8> {
    record(169, &u32s(&[set, icon]))
}
/// A BrtDXF stating only a font colour `c`.
fn font_dxf(c: u32) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0];
    out.extend(1u16.to_le_bytes());
    out.extend([5, 0, 12, 0]); // font colour
    out.extend(rgb(c));
    out
}
/// A sheet AutoFilter over A1:C4 whose first column has `body`.
fn sheet_filter(body: &[u8], dxfs: &[Vec<u8>]) -> (Workbook, ReadReport) {
    read(&autofilter([0, 3, 0, 2], &[filter_column(0, body)]), dxfs)
}
fn criteria(wb: &Workbook) -> Vec<(u32, FilterCriterion)> {
    wb.sheets[0].autofilter.as_ref().map(|af| af.criteria.clone()).unwrap_or_default()
}
fn count(report: &ReadReport, text: &str) -> usize {
    report.warnings.iter().filter(|w| w.contains(text)).count()
}

#[test]
fn xlsb_cell_colour_filter_reads_the_dxf_fill() {
    let (wb, report) = sheet_filter(&color_filter(0, 1), &[fill_dxf(0xFF0000)]);
    assert_eq!(criteria(&wb), vec![(0, FilterCriterion::FillColor(Color::Rgb(0xFF0000)))], "{:?}", report.warnings);
    assert_eq!(count(&report, UNSUPPORTED), 0);
}

#[test]
fn xlsb_font_colour_filter_reads_the_dxf_font_colour() {
    let (wb, report) = sheet_filter(&color_filter(0, 0), &[font_dxf(0x0000FF)]);
    assert_eq!(criteria(&wb), vec![(0, FilterCriterion::FontColor(Color::Rgb(0x0000FF)))], "{:?}", report.warnings);
}

#[test]
fn xlsb_colour_filter_with_a_missing_dxf_is_dropped() {
    let (wb, _) = sheet_filter(&color_filter(7, 1), &[fill_dxf(0xFF0000)]);
    assert!(wb.sheets[0].autofilter.is_some());
    assert!(criteria(&wb).is_empty());
}

#[test]
fn xlsb_icon_filter_names_the_icon_set() {
    let (wb, report) = sheet_filter(&icon_filter(3, 2), &[]);
    assert_eq!(criteria(&wb), vec![(0, FilterCriterion::Icon { set: "3TrafficLights1".into(), id: Some(2) })], "{:?}", report.warnings);
    let (wb, _) = sheet_filter(&icon_filter(0, u32::MAX), &[]);
    assert_eq!(criteria(&wb), vec![(0, FilterCriterion::Icon { set: "3Arrows".into(), id: None })]);
}

#[test]
fn xlsb_icon_filter_with_an_unknown_set_is_dropped_with_one_warning() {
    let (wb, report) = sheet_filter(&icon_filter(99, 0), &[]);
    assert!(criteria(&wb).is_empty());
    assert_eq!(count(&report, UNSUPPORTED), 1, "{:?}", report.warnings);
}

#[test]
fn xlsb_short_colour_filter_is_malformed() {
    let (wb, report) = sheet_filter(&record(168, &u32s(&[0])), &[fill_dxf(0xFF0000)]);
    assert!(criteria(&wb).is_empty());
    assert_eq!(count(&report, MALFORMED), 1, "{:?}", report.warnings);
}

#[test]
fn xlsb_table_autofilter_keeps_a_colour_filter() {
    let af = autofilter([0, 3, 0, 2], &[filter_column(1, &color_filter(0, 1))]);
    let table = sales_table("Sales", [0, 3, 0, 2], false, &af, None);
    let sheet = worksheet(&sales_cells(), &list_parts(&["rIdT1"]));
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/styles.bin" ContentType="application/vnd.ms-excel.styles"/><Override PartName="/xl/tables/table1.bin" ContentType="application/vnd.ms-excel.table"/></Types>"#;
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.bin"/></Relationships>"#;
    let bytes = make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", &book()),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.bin", &sheet),
        ("xl/worksheets/_rels/sheet1.bin.rels", table_rels(1).as_bytes()),
        ("xl/tables/table1.bin", &table),
        ("xl/styles.bin", &styles(&[fill_dxf(0xFF0000)])),
    ]);
    let (wb, report) = read_xlsb(&bytes).unwrap();
    let tables = &wb.sheets[0].tables;
    assert_eq!(tables.len(), 1, "{:?}", report.warnings);
    assert_eq!(tables[0].filter, vec![(1, FilterCriterion::FillColor(Color::Rgb(0xFF0000)))]);
}

#[test]
fn xlsb_colour_and_icon_filters_survive_an_xlsx_round_trip() {
    let body = autofilter([0, 3, 0, 2], &[filter_column(0, &color_filter(0, 1)), filter_column(2, &icon_filter(3, 2))]);
    let (wb, report) = read(&body, &[fill_dxf(0xFF0000)]);
    let expected =
        vec![(0, FilterCriterion::FillColor(Color::Rgb(0xFF0000))), (2, FilterCriterion::Icon { set: "3TrafficLights1".into(), id: Some(2) })];
    assert_eq!(criteria(&wb), expected, "{:?}", report.warnings);
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(criteria(&back), expected);
}
