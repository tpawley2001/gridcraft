//! Repeated title rows print at the top of every page, so a later band of columns that the title
//! rows reach is never blank: Excel prints every row band of it (measured from Excel's PDF of a
//! workbook with title rows 1:6 reaching the second band of columns and body cells only in the
//! first band: 12 pages of each band). Letter portrait, default widths and heights; A1:A200
//! filled; column P is in the second band of columns.
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
fn a_cell_outside_the_title_rows_does_not_repeat() {
    let mut s = two_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"titleRows": "2:2"})).unwrap();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    assert_eq!(b, [a[0]], "{pages:?}");
}

#[test]
fn title_rows_that_stay_in_the_first_band_change_nothing() {
    let mut s = two_hundred_rows();
    s.execute("pageLayout.pageSetup", json!({"titleRows": "1:1"})).unwrap();
    s.execute("range.setValues", json!({"range": "A1", "values": [["head"]]})).unwrap();
    s.execute("range.setValues", json!({"range": "P100", "values": [["x"]]})).unwrap();
    let pages = page_list(&mut s);
    let (a, b) = split(&pages);
    let at = a.iter().position(|r| {
        let (lo, hi) = r.split_once(':').unwrap();
        lo.parse::<u32>().unwrap() <= 100 && 100 <= hi.parse::<u32>().unwrap()
    });
    // Down to the band holding P100 and no further (body rows only; title rows add nothing in P).
    assert_eq!(b, a[..=at.unwrap()], "{pages:?}");
}
