//! Pages left out of the page grid, continued: a page that holds only text overflowing from the
//! columns before it, or part of a table, has something on it and is printed, as Excel prints
//! (measured from Excel's PDFs: a workbook whose second band of columns carries only the text
//! running over from the first band prints that band down to the last such row).
//! Letter portrait with default margins, column widths and row heights: 47 rows per page, and
//! column P is in a later band of columns than column A.
use serde_json::json;

use super::*;

/// A1:A200 filled (five pages down in the first band of columns) and P1, which makes a later
/// band of columns whose own last cell is on its first page.
fn sheet() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (1..=200).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    s
}

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

/// The rows of the pages of the band of columns that holds column P.
fn rows_of_p_band(pages: &[(String, String)]) -> Vec<&str> {
    pages.iter().filter(|p| p.0.ends_with(":P")).map(|p| p.1.as_str()).collect()
}

fn first_band_rows(pages: &[(String, String)]) -> Vec<&str> {
    pages.iter().filter(|p| p.0 == pages[0].0).map(|p| p.1.as_str()).collect()
}

const FIVE: [&str; 5] = ["1:47", "48:94", "95:141", "142:188", "189:200"];

#[test]
fn without_overflowing_text_the_later_band_prints_one_page() {
    let mut s = sheet();
    let pages = page_list(&mut s);
    assert_eq!(first_band_rows(&pages), FIVE, "{pages:?}");
    assert_eq!(rows_of_p_band(&pages), ["1:47"], "{pages:?}");
}

/// A long text in A150 runs over the empty cells to its right, into the later bands of columns
/// (past P, which widens the printed range to where the text ends): the last band's pages down
/// to rows 142:188 hold that text and are printed, and every band before it prints whole.
#[test]
fn text_overflowing_into_a_later_band_prints_that_bands_page() {
    let mut s = sheet();
    s.execute("cell.set", json!({"cell": "A150", "input": "overflowing words ".repeat(24)})).unwrap();
    let pages = page_list(&mut s);
    assert_eq!(first_band_rows(&pages), FIVE, "{pages:?}");
    let mut later: Vec<&str> = pages.iter().map(|p| p.0.as_str()).filter(|c| *c != pages[0].0).collect();
    later.dedup();
    assert!(later.len() >= 2, "the text reaches past P's band: {pages:?}");
    let last = later.len() - 1;
    for (i, band) in later.into_iter().enumerate() {
        let rows: Vec<&str> = pages.iter().filter(|p| p.0 == band).map(|p| p.1.as_str()).collect();
        let expected = if i == last { &FIVE[..4] } else { &FIVE[..] };
        assert_eq!(rows, expected, "{band}: {pages:?}");
    }
}

/// A value in B150 cuts the text off inside the first band: nothing reaches the later band.
#[test]
fn text_cut_off_by_a_neighbour_does_not_count() {
    let mut s = sheet();
    s.execute("cell.set", json!({"cell": "A150", "input": "overflowing words ".repeat(24)})).unwrap();
    s.execute("cell.set", json!({"cell": "B150", "input": "stop"})).unwrap();
    let pages = page_list(&mut s);
    assert_eq!(rows_of_p_band(&pages), ["1:47"], "{pages:?}");
}

/// Wrapped text stays inside its own column.
#[test]
fn wrapped_text_does_not_count() {
    let mut s = sheet();
    s.execute("cell.set", json!({"cell": "A150", "input": "overflowing words ".repeat(24)})).unwrap();
    s.execute("home.wrapText", json!({"range": "A150"})).unwrap();
    let pages = page_list(&mut s);
    // The wrapped row is taller, so the first band's page breaks may move: only the later band is checked.
    assert_eq!(pages.iter().filter(|p| p.0.ends_with(":P")).count(), 1, "{pages:?}");
}

/// Numbers never overflow into the next cell.
#[test]
fn a_long_number_does_not_count() {
    let mut s = sheet();
    s.execute("cell.set", json!({"cell": "A150", "input": "123456789012345678"})).unwrap();
    let pages = page_list(&mut s);
    assert_eq!(rows_of_p_band(&pages), ["1:47"], "{pages:?}");
}

/// A table over N150:P160 with empty body rows: its rows lie on the fourth page down of P's
/// band, which is printed.
#[test]
fn a_tables_range_counts_as_something_on_the_page() {
    let mut s = sheet();
    s.execute("range.setValues", json!({"range": "N150", "values": [["a", "b", "c"]]})).unwrap();
    s.execute("insert.table", json!({"range": "N150:P160", "header": true})).unwrap();
    let pages = page_list(&mut s);
    assert_eq!(first_band_rows(&pages), FIVE, "{pages:?}");
    assert_eq!(rows_of_p_band(&pages), ["1:47", "48:94", "95:141", "142:188"], "{pages:?}");
}
