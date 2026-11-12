//! More cases of leaving out the empty pages below and to the right of everything on a sheet:
//! drawing objects and merged ranges count as something on a page, Fit To and both page orders
//! follow the same rule, and an explicit print area prints its full grid of pages.
use serde_json::json;

use super::*;

/// A1:A200 filled: five pages down in the first band of columns (45 rows a page).
fn two_hundred_rows() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (1..=200).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

/// (columns, rows) of every printed page, in print order.
fn pages(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    for (i, p) in list.iter().enumerate() {
        assert_eq!(p["page"], i + 1);
    }
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

fn rows_of(pages: &[(String, String)]) -> Vec<&str> {
    pages.iter().map(|p| p.1.as_str()).collect()
}

#[test]
fn a_shape_alone_on_a_later_bands_page_prints_it() {
    let mut s = two_hundred_rows();
    // A small rectangle at P60 (the second page down of P's band).
    s.execute("insert.shape", json!({"kind": "rectangle", "at": "P60", "width": 20, "height": 10})).unwrap();
    let p = pages(&mut s);
    assert_eq!(rows_of(&p), ["1:45", "46:90", "91:135", "136:180", "181:200", "1:45", "46:90"], "{p:?}");
    assert!(p[5..].iter().all(|x| x.0 != p[0].0), "{p:?}");
}

#[test]
fn a_merged_range_reaching_into_a_page_prints_it() {
    let mut s = two_hundred_rows();
    s.execute("selection.set", json!({"range": "P40:P100"})).unwrap();
    s.execute("home.mergeCenter", json!({})).unwrap();
    s.execute("cell.set", json!({"cell": "P40", "input": "merged"})).unwrap();
    let p = pages(&mut s);
    assert_eq!(rows_of(&p), ["1:45", "46:90", "91:135", "136:180", "181:200", "1:45", "46:90", "91:135"], "{p:?}");
}

#[test]
fn fit_to_pages_tall_keeps_the_rule() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "Z1", "values": [["x"]]})).unwrap();
    // Three pages tall, automatic width: the band holding Z still prints only its first page.
    s.execute("pageLayout.pageSetup", json!({"fitWidth": 0, "fitHeight": 3})).unwrap();
    let p = pages(&mut s);
    let firsts = p.iter().filter(|x| x.0 == p[0].0).count();
    assert_eq!(firsts, 3, "{p:?}");
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(p[3].0 != p[0].0 && p[3].0.ends_with(":Z") && p[3].1 == p[0].1, "{p:?}");
}

#[test]
fn over_then_down_prints_the_same_pages() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "P100", "values": [["x"]]})).unwrap();
    let mut down = pages(&mut s);
    s.execute("pageLayout.pageSetup", json!({"pageOrder": "overThenDown"})).unwrap();
    let mut over = pages(&mut s);
    assert_eq!(over.len(), 8, "{over:?}");
    // Over then down: rows 1:45 of both bands, then 46:90 of both, 91:135 of both, then the rest.
    assert_eq!(rows_of(&over), ["1:45", "1:45", "46:90", "46:90", "91:135", "91:135", "136:180", "181:200"], "{over:?}");
    down.sort();
    over.sort();
    assert_eq!(down, over);
}

#[test]
fn a_print_area_prints_its_full_grid() {
    let mut s = two_hundred_rows();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"printArea": "A1:P200"})).unwrap();
    let p = pages(&mut s);
    assert_eq!(p.len(), 10, "{p:?}");
}

#[test]
fn a_wide_column_counts_on_every_piece() {
    let mut s = two_hundred_rows();
    // Column B wider than the page: it prints in pieces, each its own band of columns.
    s.execute("home.columnWidth", json!({"cols": "B:B", "chars": 250})).unwrap();
    s.execute("range.setValues", json!({"range": "B100", "values": [["x"]]})).unwrap();
    let p = pages(&mut s);
    let bands: Vec<&str> = p.iter().map(|x| x.0.as_str()).collect();
    // Every band of columns prints at least down to row 100 (third page down).
    let mut seen = bands.clone();
    seen.dedup();
    for b in seen {
        let n = bands.iter().filter(|x| **x == b).count();
        assert!(n >= 3, "{b}: {p:?}");
    }
}
