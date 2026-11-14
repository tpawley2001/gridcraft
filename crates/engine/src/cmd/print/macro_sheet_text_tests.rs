//! A macro sheet prints its formula text when its view shows formulas (Excel sets that on macro
//! sheets), its saved values otherwise, and its own name in the header's `&A`. Built in code.
use std::sync::Arc;

use gridcraft_model::{Cell, Formula, MacroSheetPrint};

use super::*;

fn book(show_formulas: bool) -> Workbook {
    let mut wb = Workbook::new();
    if let Some(sh) = wb.sheet_mut(0) {
        sh.set_value(CellRef::new(0, 0), Value::Number(5.0));
    }
    let mut ms = Sheet::new("Macro1");
    ms.show_formulas = show_formulas;
    ms.print.header = "&LTab &A".into();
    let formula = Some(Arc::new(Formula::new("ALIGNMENT(2,FALSE,1,0,FALSE)")));
    ms.set_cell(CellRef::new(0, 0), Cell { value: Value::Bool(true), formula, ..Cell::default() });
    wb.macro_sheets = Arc::new(vec![MacroSheetPrint::new("Macro1".into(), Visibility::Visible, 1, 0, ms)]);
    wb
}

fn pdf(wb: &Workbook) -> (Vec<u8>, usize) {
    let opts = PrintOptions { all_sheets: true, ..PrintOptions::default() };
    render_pdf(wb, &layouts(wb, &opts), "book.xlsm", "")
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn a_macro_sheet_showing_formulas_prints_the_formula_text() {
    let (bytes, pages) = pdf(&book(true));
    assert_eq!(pages, 2);
    assert!(contains(&bytes, "=ALIGNMENT\\(2,FALSE,1,0,FALSE\\)"));
    assert!(contains(&bytes, "(Tab Macro1) Tj"));
}

#[test]
fn a_macro_sheet_not_showing_formulas_prints_its_saved_values() {
    let (bytes, pages) = pdf(&book(false));
    assert_eq!(pages, 2);
    assert!(!contains(&bytes, "ALIGNMENT"));
    assert!(contains(&bytes, "(TRUE) Tj"));
}

#[test]
fn the_macro_sheet_layout_names_its_own_sheet() {
    let wb = book(true);
    let opts = PrintOptions { all_sheets: true, ..PrintOptions::default() };
    let lays = layouts(&wb, &opts);
    let names: Vec<_> = lays.iter().map(|l| (l.macro_sheet, l.sheet_in(&wb).map(|s| s.name.clone()))).collect();
    assert_eq!(names, [(None, Some("Sheet1".to_string())), (Some(0), Some("Macro1".to_string()))]);
}
