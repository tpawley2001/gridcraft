//! Without Fit To, Excel prints every page of a saved print area even when the workbook holds no
//! cells: only a Fit To area with nothing on paper collapses to one blank page.
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_engine::cmd::print::{PrintOptions, layouts};
use gridcraft_model::{Sheet, Workbook};
use std::sync::Arc;

fn rr(r0: u32, c0: u32, r1: u32, c1: u32) -> RangeRef {
    RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1))
}

fn parsed(wb: &Workbook) -> Workbook {
    let bytes = gridcraft_xlsx::write_xlsx(wb).unwrap();
    gridcraft_xlsx::read_xlsx(&bytes).unwrap().0
}

fn count(wb: &Workbook) -> usize {
    layouts(wb, &PrintOptions { all_sheets: true, ..PrintOptions::default() }).iter().map(|l| l.pages.len()).sum()
}

#[test]
fn a_two_range_area_on_an_empty_sheet_prints_two_pages() {
    let mut wb = Workbook::new();
    let sh = wb.sheet_mut(0).unwrap();
    sh.print.set_print_areas(vec![rr(0, 0, 1, 1), rr(2, 3, 4, 3)]);
    sh.print.headings = true;
    sh.print.gridlines = true;
    let wb = parsed(&wb);
    assert!(wb.sheets.iter().all(|s| s.cells.is_empty()));
    assert_eq!(count(&wb), 2);
}

#[test]
fn manual_breaks_in_an_empty_area_print_every_page() {
    let mut wb = Workbook::new();
    wb.sheets.push(Arc::new(Sheet::new("Sheet2")));
    wb.sheet_mut(0).unwrap().print.set_print_areas(vec![rr(0, 0, 1, 1), rr(2, 3, 4, 3)]);
    let sh = wb.sheet_mut(1).unwrap();
    sh.print.set_print_areas(vec![rr(0, 0, 4, 3)]);
    sh.print.row_breaks = vec![2];
    sh.print.col_breaks = vec![2];
    let wb = parsed(&wb);
    assert!(wb.sheets.iter().all(|s| s.cells.is_empty()));
    assert_eq!(count(&wb), 6);
}
