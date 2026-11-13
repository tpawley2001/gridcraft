//! Pages of the page grid with nothing on them, below them or to their right are not printed, as
//! Excel prints (measured from Excel's PDFs of workbooks whose second band of columns ends far
//! above the first one's last row: Excel prints that band down to its last cell only).
//! Letter portrait with default margins, column widths and row heights: 47 rows per page, and
//! column P is in the second band of columns across.
use serde_json::json;

use super::*;

fn s() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    for (i, p) in list.iter().enumerate() {
        assert_eq!(p["page"], i + 1, "pages are numbered without gaps");
    }
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

/// A1:A200 filled: five pages down in the first band of columns.
fn two_hundred_rows() -> Session {
    let mut s = s();
    let rows: Vec<Vec<Json>> = (1..=200).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

fn rows_of(pages: &[(String, String)]) -> Vec<&str> {
    pages.iter().map(|p| p.1.as_str()).collect()
}

#[test]
fn one_band_of_columns_is_unchanged() {
    let mut s = two_hundred_rows();
    let pages = page_list(&mut s);
    assert_eq!(rows_of(&pages), ["1:47", "48:94", "95:141", "142:188", "189:200"], "{pages:?}");
    assert!(pages.iter().all(|p| p.0 == "A:A"), "{pages:?}");
}

/// Excel: 36 pages of the first band, then ONE page of a second band that holds text in its
/// first rows only. Here: five pages of A:I, then the first page of the band holding P1.
#[test]
fn the_empty_pages_below_a_later_bands_last_cell_are_left_out() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    assert_eq!(rows_of(&pages), ["1:47", "48:94", "95:141", "142:188", "189:200", "1:47"], "{pages:?}");
    assert!(pages[..5].iter().all(|p| p.0 == pages[0].0 && p.0.starts_with("A:")), "{pages:?}");
    assert!(pages[5].0 != pages[0].0 && pages[5].0.ends_with(":P"), "{pages:?}");
}

/// Excel prints the empty pages ABOVE a band's last cell (a second band whose lowest cell is on
/// its 13th page prints 13 pages, most of them blank), and nothing below it.
#[test]
fn the_empty_pages_above_a_later_bands_last_cell_still_print() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "P100", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    assert_eq!(rows_of(&pages), ["1:47", "48:94", "95:141", "142:188", "189:200", "1:47", "48:94", "95:141"], "{pages:?}");
    assert!(pages[..5].iter().all(|p| p.0 == pages[0].0 && p.0.starts_with("A:")), "{pages:?}");
    assert!(pages[5..].iter().all(|p| p.0 != pages[0].0 && p.0.ends_with(":P")), "{pages:?}");
}
