//! Excel prints frozen f944e4f0ee as one blank page despite its saved multi-page area.
//! These tests build an independent seven-sheet empty workbook and parse SpreadsheetML.
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_engine::{
    Session,
    cmd::print::{PrintOptions, layouts},
};
use gridcraft_model::{Cell, Color, Fill, Sheet, Style, Workbook};
use serde_json::json;
use std::sync::Arc;

fn area() -> RangeRef {
    RangeRef::new(CellRef::new(4, 3), CellRef::new(199, 19))
}

fn fixture() -> Workbook {
    let mut wb = Workbook::new();
    for i in 1..7 {
        wb.sheets.push(Arc::new(Sheet::new(format!("Blank{i}"))));
    }
    let sh = wb.sheet_mut(4).unwrap();
    sh.print.set_print_areas(vec![area()]);
    sh.print.fit_to = Some((1, 2));
    sh.print.scale = 51;
    wb
}

fn parsed(wb: &Workbook) -> Workbook {
    let bytes = gridcraft_xlsx::write_xlsx(wb).unwrap();
    let (wb, _) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    wb
}

fn settings_intact(wb: &Workbook) {
    let sh = wb.sheet(4).unwrap();
    assert_eq!(sh.print.print_areas(), vec![area()]);
    assert_eq!(sh.print.fit_to, Some((1, 2)));
    assert_eq!(sh.print.scale, 51);
}

fn count(wb: &Workbook) -> usize {
    layouts(wb, &PrintOptions { all_sheets: true, ..PrintOptions::default() }).iter().map(|l| l.pages.len()).sum()
}

#[test]
fn empty_saved_area_does_not_create_several_blank_preview_pages() {
    let wb = parsed(&fixture());
    assert!(wb.sheets.iter().all(|s| s.cells.is_empty()));
    settings_intact(&wb);
    assert_eq!(count(&wb), 1, "Excel's wholly empty workbook fallback is one blank page");
    settings_intact(&wb);
}

#[test]
fn fit_two_pages_tall_does_not_force_two_pages_in_an_empty_workbook_pdf() {
    let wb = parsed(&fixture());
    let bytes = gridcraft_xlsx::write_xlsx(&wb).unwrap();
    let mut s = Session::new();
    s.execute("file.open", json!({"name":"blank.xlsx", "base64":gridcraft_engine::io::base64_encode(&bytes)})).unwrap();
    let result = s.execute("file.exportPdf", json!({"sheets":"all"})).unwrap();
    assert_eq!(result["pages"], 1);
    settings_intact(&parsed(&wb));
}

#[test]
fn ordinary_empty_workbook_still_prints_one_blank_page() {
    assert_eq!(count(&parsed(&Workbook::new())), 1);
}

#[test]
fn a_value_inside_a_tall_area_prevents_the_empty_workbook_fallback() {
    let mut wb = fixture();
    wb.sheet_mut(4).unwrap().set_cell(CellRef::new(199, 3), Cell { value: Value::Number(7.0), ..Cell::default() });
    assert!(count(&parsed(&wb)) > 1);
}

#[test]
fn a_visible_fill_inside_the_area_prevents_the_empty_workbook_fallback() {
    let mut wb = fixture();
    let style = wb.styles.intern(Style { fill: Fill::solid(Color::rgb(255, 255, 0)), ..Style::default() });
    wb.sheet_mut(4).unwrap().set_cell(CellRef::new(199, 3), Cell { style, ..Cell::default() });
    assert!(count(&parsed(&wb)) > 1);
}
