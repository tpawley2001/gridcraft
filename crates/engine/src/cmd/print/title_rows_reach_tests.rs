//! Repeated title rows: when a sheet has print title rows, Excel prints EVERY row band of every band
//! of columns, whatever the title rows hold (even nothing) and wherever the far cell is. Measured
//! from Excel's PDFs (Microsoft 365, Letter portrait, default widths and heights, A1:A200 filled,
//! column P in the second band of columns, 5 row bands): no titles + P100 = 5 + 3 pages; titles 1:1 +
//! P100, titles 2:2 + P1, titles 1:1 + P1, titles 1:1 + P5, empty title row 1 (A2:A200 filled) + P100
//! all = 10 pages (5 + 5); titles 1:1 + Z100 (third band) = 15. Without title rows Excel still skips
//! the blank pages of a later band (P5 alone = 5 + 1).
use serde_json::json;

use super::*;

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

fn two_hundred_rows() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (1..=200).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

/// Pages of the first band of columns, then pages of the band holding column P.
fn split(pages: &[(String, String)]) -> (Vec<&str>, Vec<&str>) {
    let first = &pages[0].0;
    let a = pages.iter().filter(|p| &p.0 == first).map(|p| p.1.as_str()).collect();
    let b = pages.iter().filter(|p| p.0.ends_with(":P")).map(|p| p.1.as_str()).collect();
    (a, b)
}

#[test]
fn title_rows_reaching_the_second_band_print_every_page_of_it() {
    let mut s = two_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"titleRows": "1:1"})).unwrap();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    assert!(a.len() > 1, "{pages:?}");
    assert_eq!(a, b, "the second band prints the same row bands as the first: {pages:?}");
    assert_eq!(pages.len(), a.len() * 2, "{pages:?}");
}

#[test]
fn without_title_rows_only_the_first_page_of_the_second_band_prints() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    assert_eq!(b, [a[0]], "{pages:?}");
    assert_eq!(pages.len(), a.len() + 1, "{pages:?}");
}

#[test]
fn title_rows_print_every_band_even_when_the_far_cell_is_outside_them() {
    let mut s = two_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"titleRows": "2:2"})).unwrap();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    assert!(a.len() > 1, "{pages:?}");
    assert_eq!(a, b, "{pages:?}");
}

#[test]
fn title_rows_print_every_band_when_the_far_cell_is_below_them() {
    for far in ["P5", "P100"] {
        let mut s = two_hundred_rows();
        s.execute("pageLayout.pageSetup", json!({"titleRows": "1:1"})).unwrap();
        s.execute("range.setValues", json!({"range": far, "values": [["x"]]})).unwrap();
        let pages = page_list(&mut s);
        let (a, b) = split(&pages);
        assert!(a.len() > 1, "{far}: {pages:?}");
        assert_eq!(a, b, "{far}: {pages:?}");
        assert_eq!(pages.len(), a.len() * 2, "{far}: {pages:?}");
    }
}

#[test]
fn an_empty_title_row_still_prints_every_band() {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (2..=200).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A2", "values": rows})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"titleRows": "1:1"})).unwrap();
    s.execute("range.setValues", json!({"range": "P100", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    assert!(a.len() > 1, "{pages:?}");
    assert_eq!(a, b, "{pages:?}");
}

#[test]
fn without_title_rows_a_far_cell_near_the_top_prints_one_page_of_its_band() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "P5", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    assert_eq!(b, [a[0]], "{pages:?}");
}
