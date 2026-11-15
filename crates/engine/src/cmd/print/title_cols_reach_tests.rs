//! Repeated title columns print at the left of every page, and Excel then prints every row band of
//! the printed range in every band of columns, blank pages included (measured from Excel's PDF of
//! synthetic workbooks: rows 1..150 and Z1 = "z" on Letter portrait, default widths and heights,
//! title columns $A:$A). Without title columns the later bands stop after their last page with
//! something on it.
use serde_json::json;

use super::*;

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

/// The row bands printed in each band of columns, bands in print order.
fn bands(pages: &[(String, String)]) -> Vec<Vec<String>> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (c, r) in pages {
        match out.iter_mut().find(|b| &b.0 == c) {
            Some(b) => b.1.push(r.clone()),
            None => out.push((c.clone(), vec![r.clone()])),
        }
    }
    out.into_iter().map(|b| b.1).collect()
}

/// `a` holds the rows of column A that get a number, `b_all` fills B1:B150; Z1 = "z" always.
fn sheet(a: &[u32], b_all: bool, titles: bool) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for &r in a {
        s.execute("cell.set", json!({"cell": format!("A{r}"), "input": r.to_string()})).unwrap();
    }
    if b_all {
        let rows: Vec<Vec<Json>> = (1..=150).map(|i| vec![json!(i)]).collect();
        s.execute("range.setValues", json!({"range": "B1", "values": rows})).unwrap();
    }
    s.execute("cell.set", json!({"cell": "Z1", "input": "z"})).unwrap();
    if titles {
        s.execute("pageLayout.pageSetup", json!({"titleCols": "A:A"})).unwrap();
    }
    s
}

fn all_rows() -> Vec<u32> {
    (1..=150).collect()
}

/// Every band of columns prints the same row bands as the first; there are at least 3 bands.
fn assert_full_grid(pages: &[(String, String)]) {
    let b = bands(pages);
    assert!(b.len() >= 3, "{pages:?}");
    for band in &b[1..] {
        assert_eq!(band, &b[0], "{pages:?}");
    }
}

#[test]
fn without_title_columns_the_last_band_stops_after_its_first_page() {
    let mut s = sheet(&all_rows(), false, false);
    let pages = page_list(&mut s);
    let b = bands(&pages);
    assert!(b.len() >= 3 && b[0].len() >= 4, "{pages:?}");
    assert_eq!(b.last().unwrap(), &b[0][..1], "{pages:?}");
}

#[test]
fn title_columns_with_content_on_every_row_print_every_page() {
    let mut s = sheet(&all_rows(), false, true);
    let pages = page_list(&mut s);
    assert_full_grid(&pages);
    assert!(bands(&pages)[0].len() >= 4, "{pages:?}");
}

#[test]
fn title_columns_with_content_only_in_the_first_row_band_print_one_row_band() {
    let rows: Vec<u32> = (1..=30).collect();
    let mut s = sheet(&rows, false, true);
    let pages = page_list(&mut s);
    assert_full_grid(&pages);
    assert_eq!(bands(&pages)[0].len(), 1, "{pages:?}");
}

#[test]
fn title_columns_print_the_blank_row_band_before_the_last_one() {
    let mut s = sheet(&[1, 100], false, true);
    let pages = page_list(&mut s);
    assert_full_grid(&pages);
    let first = &bands(&pages)[0];
    let last = first.last().unwrap();
    let (lo, hi) = last.split_once(':').unwrap();
    assert!(lo.parse::<u32>().unwrap() <= 100 && 100 <= hi.parse::<u32>().unwrap(), "{pages:?}");
    assert!(first.len() >= 3, "{pages:?}");
}

#[test]
fn title_columns_empty_below_row_one_still_print_every_row_band() {
    let mut s = sheet(&[1], true, true);
    let pages = page_list(&mut s);
    assert_full_grid(&pages);
    assert!(bands(&pages)[0].len() >= 4, "{pages:?}");
}
