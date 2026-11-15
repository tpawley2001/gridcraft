//! Sparse interval guards supplement the inherited-row-fill round-trip regressions.
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_engine::cmd::print::print_range;
use gridcraft_model::{Cell, Color, Fill, LineInfo, Style, Workbook};

fn fixture() -> Workbook {
    let mut wb = Workbook::new();
    let fill = wb.styles.intern(Style { fill: Fill::solid(Color::rgb(20, 120, 200)), ..Style::default() });
    let mut font = Style::default();
    font.font.bold = true;
    let font = wb.styles.intern(font);
    let sh = wb.sheet_mut(0).unwrap();
    sh.set_cell(CellRef::new(2, 2), Cell { style: fill, ..Cell::default() });
    sh.set_cell(CellRef::new(100, 8), Cell { style: font, ..Cell::default() });
    sh.rows.insert(90, LineInfo { style: Some(fill), ..LineInfo::default() });
    wb
}

fn parsed(wb: &Workbook) -> Workbook {
    let (wb, report) = gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(wb).unwrap()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    wb
}

#[test]
fn only_hidden_missing_positions_do_not_supply_row_fill_ink() {
    let mut wb = fixture();
    let sh = wb.sheet_mut(0).unwrap();
    let font = sh.cells.get(CellRef::new(100, 8)).unwrap().style;
    for col in 2..8 {
        sh.set_cell(CellRef::new(90, col), Cell { style: font, ..Cell::default() });
    }
    sh.cols.insert(8, LineInfo { hidden: true, ..LineInfo::default() });
    let wb = parsed(&wb);
    let sh = wb.sheet(0).unwrap();
    assert!(sh.cells.get(CellRef::new(100, 8)).is_some());
    assert!(sh.cells.get(CellRef::new(90, 2)).is_some());
    assert!(sh.cells.get(CellRef::new(90, 8)).is_none());
    assert_eq!(print_range(&wb, sh), Some(RangeRef::new(CellRef::new(0, 0), CellRef::new(2, 2))));
}

#[test]
fn row_fill_overrides_unfilled_column_style_but_stops_at_explicit_cells() {
    let mut wb = fixture();
    let sh = wb.sheet_mut(0).unwrap();
    sh.cols.insert(7, LineInfo { style: Some(Default::default()), ..LineInfo::default() });
    let font = sh.cells.get(CellRef::new(100, 8)).unwrap().style;
    sh.set_cell(CellRef::new(90, 8), Cell { style: font, ..Cell::default() });
    let wb = parsed(&wb);
    let sh = wb.sheet(0).unwrap();
    assert!(sh.cells.get(CellRef::new(100, 8)).is_some());
    assert!(sh.cells.get(CellRef::new(90, 8)).is_some());
    assert!(sh.cells.get(CellRef::new(90, 7)).is_none());
    assert_eq!(sh.style_id(CellRef::new(90, 7)), sh.rows.get(&90).unwrap().style.unwrap());
    assert_eq!(print_range(&wb, sh), Some(RangeRef::new(CellRef::new(0, 0), CellRef::new(90, 7))));
}

#[test]
fn row_fills_outside_the_stored_vertical_footprint_do_not_print() {
    let mut wb = fixture();
    let sh = wb.sheet_mut(0).unwrap();
    let info = sh.rows.remove(&90).unwrap();
    sh.rows.insert(1, info);
    sh.rows.insert(101, info);
    let wb = parsed(&wb);
    assert_eq!(print_range(&wb, wb.sheet(0).unwrap()), Some(RangeRef::new(CellRef::new(0, 0), CellRef::new(2, 2))));
}

#[test]
fn inherited_fill_keeps_later_bands_even_when_stored_cells_already_bound_the_range() {
    use gridcraft_engine::cmd::print::{PrintOptions, layout_sheet};
    use gridcraft_model::PageOrder;

    let mut wb = fixture();
    let fill = wb.sheet(0).unwrap().rows.get(&90).unwrap().style.unwrap();
    let sh = wb.sheet_mut(0).unwrap();
    for col in 0..26 {
        sh.set_cell(CellRef::new(0, col), Cell { style: fill, ..Cell::default() });
    }
    sh.set_cell(CellRef::new(90, 0), Cell { style: fill, ..Cell::default() });
    // The range already reaches the inherited row: only the page-content check differs.
    let mut control = wb.clone();
    for col in 1..26 {
        control.sheet_mut(0).unwrap().set_cell(CellRef::new(90, col), Cell { style: fill, ..Cell::default() });
    }
    for order in [PageOrder::DownThenOver, PageOrder::OverThenDown] {
        wb.sheet_mut(0).unwrap().print.page_order = order;
        control.sheet_mut(0).unwrap().print.page_order = order;
        let implicit = parsed(&wb);
        let explicit = parsed(&control);
        let a = layout_sheet(&implicit, 0, &PrintOptions::default(), None).unwrap();
        let b = layout_sheet(&explicit, 0, &PrintOptions::default(), None).unwrap();
        assert_eq!(a.area, b.area);
        assert_eq!(a.row_bands, b.row_bands);
        assert_eq!(a.col_bands, b.col_bands);
        assert!(a.col_bands.len() > 1 && a.row_bands.len() > 1);
        assert_eq!(a.pages, b.pages);
    }
}
