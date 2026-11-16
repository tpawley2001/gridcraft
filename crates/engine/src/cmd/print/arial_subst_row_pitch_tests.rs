//! Normal fonts Excel does not have and prints in Arial (Clear Sans, Liberation Sans, generic "Sans", Helvetica...,
//! Geneva) print their rows at Arial's printed height for that size, as their column widths already use Arial's
//! digit width. Measured on Excel 365's PDF of a workbook whose Normal font is Clear Sans 8: every row prints at
//! 85 dots (Arial 8's pair) and a Letter-portrait page holds 66 rows. Names outside that group, and sizes Arial is
//! not measured at, keep Calibri 11's pair as before.
use serde_json::json;

use super::metrics::{cell_font_px, printed_row_dots};
use super::*;

#[test]
fn substituted_names_print_rows_at_arials_dots() {
    assert_eq!(printed_row_dots(11.25, "Clear Sans", 8.0), 85);
    assert_eq!(printed_row_dots(15.0, "clear sans", 8.0), 113);
    assert_eq!(printed_row_dots(12.75, "Liberation Sans", 10.0), 103);
    assert_eq!(printed_row_dots(12.75, "'Liberation Sans'", 10.0), 103);
    assert_eq!(printed_row_dots(24.0, "Helvetica Neue", 10.0), 194);
    assert_eq!(printed_row_dots(15.0, "Geneva", 12.0), 123);
}

#[test]
fn other_names_and_unmeasured_sizes_keep_the_calibri_pair() {
    // Arial 7 is not measured: the substituted name falls back exactly like an unknown font.
    assert_eq!(printed_row_dots(15.0, "Clear Sans", 7.0), 121);
    assert_eq!(printed_row_dots(15.0, "No Such Font", 11.0), 121);
    assert_eq!(printed_row_dots(11.25, "No Such Font", 8.0), 90);
}

#[test]
fn substituted_cell_fonts_take_arials_default_row() {
    assert_eq!(cell_font_px("Clear Sans", 8.0), Some(15));
    assert_eq!(cell_font_px("Sans", 10.0), Some(17));
    assert_eq!(cell_font_px("No Such Font", 8.0), None);
}

/// Normal font Clear Sans 8, 132 rows of 11.25 pt, Letter portrait, 0.75 in margins: 66 rows a page, like Arial 8.
#[test]
fn a_clear_sans_sheet_prints_66_rows_a_page() {
    let mut s = Session::new();
    s.new_workbook();
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut normal = Style::default();
    normal.font.name = "Clear Sans".to_string();
    normal.font.size = 8.0;
    wb.styles = gridcraft_model::StyleTable::with_default(normal);
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "margins": {"top": 0.75, "bottom": 0.75}})).unwrap();
    let rows: Vec<Vec<Json>> = (1..=132).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("home.rowHeight", json!({"rows": "1:132", "points": 11.25})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list: Vec<String> = pv["pageList"].as_array().unwrap().iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect();
    assert_eq!(list, vec!["1:66".to_string(), "67:132".to_string()], "pages {list:?}");
}
