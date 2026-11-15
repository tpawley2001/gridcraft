//! Missing grid positions inherit a visible row fill. Excel's 0f6b5534e1 print reaches row
//! 140 in every column band (12 pages vs 3 without inherited fills). Tests synthesize the
//! same root cause and compare it with explicit fills; no corpus/Excel files are embedded.
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_engine::cmd::print::{PrintOptions, layout_sheet, print_range};
use gridcraft_model::{Cell, Color, Fill, LineInfo, PatternType, Style, Workbook};

fn at(row: u32, col: u32) -> CellRef {
    CellRef::new(row, col)
}

fn fixture() -> Workbook {
    let mut wb = Workbook::new();
    let fill = wb.styles.intern(Style { fill: Fill::solid(Color::rgb(255, 255, 0)), ..Style::default() });
    let mut font = Style::default();
    font.font.bold = true;
    let font = wb.styles.intern(font);
    let sh = wb.sheet_mut(0).unwrap();
    // A visibly filled first row establishes three horizontal bands. The last stored
    // cell has only a font; missing row 140 inherits a fill inside that footprint.
    for col in 0..26 {
        sh.set_cell(at(0, col), Cell { style: fill, ..Cell::default() });
    }
    sh.set_cell(at(140, 25), Cell { style: font, ..Cell::default() });
    sh.rows.insert(139, LineInfo { style: Some(fill), ..LineInfo::default() });
    wb
}

fn parsed(wb: &Workbook) -> Workbook {
    // Parse SpreadsheetML through the actual reader, rather than matching XML text.
    let bytes = gridcraft_xlsx::write_xlsx(wb).unwrap();
    let (wb, report) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    wb
}

fn range(wb: &Workbook) -> RangeRef {
    print_range(wb, wb.sheet(0).unwrap()).unwrap()
}

fn materialize(mut wb: Workbook) -> Workbook {
    let sh = wb.sheet_mut(0).unwrap();
    for col in 0..26 {
        let c = at(139, col);
        let style = sh.style_id(c);
        sh.set_cell(c, Cell { style, ..Cell::default() });
    }
    wb
}

#[test]
fn inherited_row_fill_extends_the_automatic_range_without_storing_cells() {
    let wb = parsed(&fixture());
    let sh = wb.sheet(0).unwrap();
    assert!(sh.cells.get(at(139, 0)).is_none());
    let row_style = sh.rows.get(&139).unwrap().style.unwrap();
    assert_eq!(sh.style_id(at(139, 0)), row_style);
    assert_eq!(wb.styles.get(row_style).fill.pattern, PatternType::Solid);
    let before = sh.cells.len();
    assert_eq!(range(&wb), RangeRef::new(at(0, 0), at(139, 25)));
    assert_eq!(wb.sheet(0).unwrap().cells.len(), before, "print does not materialize cells");
}

#[test]
fn inherited_fill_keeps_the_same_trailing_pages_in_every_band_as_explicit_fill() {
    let implicit = parsed(&fixture());
    let explicit = parsed(&materialize(fixture()));
    let options = PrintOptions::default();
    let a = layout_sheet(&implicit, 0, &options, None).unwrap();
    let b = layout_sheet(&explicit, 0, &options, None).unwrap();
    assert!(b.col_bands.len() > 1);
    assert!(b.row_bands.len() > 1);
    assert_eq!(a.area, b.area);
    assert_eq!(a.row_bands, b.row_bands);
    assert_eq!(a.col_bands, b.col_bands);
    assert_eq!(a.pages, b.pages, "all inherited-filled bands retain the final row's page");
}

#[test]
fn hidden_row_fill_does_not_extend_the_range() {
    let mut wb = fixture();
    wb.sheet_mut(0).unwrap().rows.get_mut(&139).unwrap().hidden = true;
    assert_eq!(range(&parsed(&wb)), RangeRef::new(at(0, 0), at(0, 25)));
}

#[test]
fn explicit_unfilled_cells_override_the_row_fill() {
    let mut wb = fixture();
    let font = wb.sheet(0).unwrap().cells.get(at(140, 25)).unwrap().style;
    for col in 0..26 {
        wb.sheet_mut(0).unwrap().set_cell(at(139, col), Cell { style: font, ..Cell::default() });
    }
    let wb = parsed(&wb);
    let sh = wb.sheet(0).unwrap();
    assert!(sh.cells.get(at(139, 0)).is_some());
    assert_eq!(wb.styles.get(sh.style_id(at(139, 0))).fill.pattern, PatternType::None);
    assert_eq!(range(&wb), RangeRef::new(at(0, 0), at(0, 25)));
}

#[test]
fn a_font_only_row_style_does_not_extend_the_range() {
    let mut wb = fixture();
    let font = wb.sheet(0).unwrap().cells.get(at(140, 25)).unwrap().style;
    wb.sheet_mut(0).unwrap().rows.get_mut(&139).unwrap().style = Some(font);
    assert_eq!(range(&parsed(&wb)), RangeRef::new(at(0, 0), at(0, 25)));
}

#[test]
fn row_metadata_alone_does_not_create_a_print_footprint() {
    let mut wb = Workbook::new();
    let fill = wb.styles.intern(Style { fill: Fill::solid(Color::rgb(255, 255, 0)), ..Style::default() });
    wb.sheet_mut(0).unwrap().rows.insert(139, LineInfo { style: Some(fill), ..LineInfo::default() });
    let wb = parsed(&wb);
    assert!(print_range(&wb, wb.sheet(0).unwrap()).is_none());
}

#[test]
fn explicit_print_area_keeps_its_precedence_over_inherited_row_fill() {
    let mut wb = fixture();
    wb.sheet_mut(0).unwrap().print.set_print_areas(vec![RangeRef::cell(at(0, 0))]);
    assert_eq!(range(&parsed(&wb)), RangeRef::cell(at(0, 0)));
}
