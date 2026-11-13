//! Empty pages, the full rule: Excel lays a sheet out as the full grid of pages, walks it in the
//! sheet's page order and stops after the last page that holds anything. Blank pages before that
//! page are printed; only the pages after it are left out (measured from Excel's PDFs: a workbook
//! with cells only top-left, top-right and bottom-left of its used range prints every band of
//! columns but the last at full height, then one page of the last band).
//! Letter portrait with default margins, column widths and row heights: 45 rows per page, and the
//! bands of columns are A:J, K:T and U:AD.
use serde_json::json;

use super::*;

/// A new workbook with `1` in every cell of `cells`.
fn sheet(cells: &[String]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for c in cells {
        s.execute("cell.set", json!({"cell": c, "input": "1"})).unwrap();
    }
    s
}

fn refs(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| c.to_string()).collect()
}

/// Row 1 filled from A to AD and column A filled down to row 120: an L shape.
fn l_shape() -> Vec<String> {
    const TOP: [&str; 30] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "AA", "AB",
        "AC", "AD",
    ];
    TOP.iter().map(|c| format!("{c}1")).chain((2..=120).map(|r| format!("A{r}"))).collect()
}

fn over_then_down(s: &mut Session) {
    s.execute("pageLayout.pageSetup", json!({"pageOrder": "overThenDown"})).unwrap();
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

fn expect(pages: &[(String, String)], want: &[(&str, &str)]) {
    let got: Vec<(&str, &str)> = pages.iter().map(|p| (p.0.as_str(), p.1.as_str())).collect();
    assert_eq!(got, want);
}

/// Seven pages, down then over: both full bands before the last, then the last band's one page.
const SEVEN_DOWN: [(&str, &str); 7] =
    [("A:J", "1:45"), ("A:J", "46:90"), ("A:J", "91:120"), ("K:T", "1:45"), ("K:T", "46:90"), ("K:T", "91:120"), ("U:AD", "1:45")];

/// Seven pages, over then down: both full rows of pages before the last, then the last row's one page.
const SEVEN_OVER: [(&str, &str); 7] =
    [("A:J", "1:45"), ("K:T", "1:45"), ("U:AD", "1:45"), ("A:J", "46:90"), ("K:T", "46:90"), ("U:AD", "46:90"), ("A:J", "91:120")];

const FULL_GRID: [(&str, &str); 9] = [
    ("A:J", "1:45"),
    ("A:J", "46:90"),
    ("A:J", "91:120"),
    ("K:T", "1:45"),
    ("K:T", "46:90"),
    ("K:T", "91:120"),
    ("U:AD", "1:45"),
    ("U:AD", "46:90"),
    ("U:AD", "91:120"),
];

/// Cells in three corners only: the middle band of columns holds nothing at all, and prints at
/// full height because the last band's first page comes after it.
#[test]
fn three_corners_print_the_blank_middle_band_in_full() {
    let mut s = sheet(&refs(&["A1", "AD1", "A120"]));
    expect(&page_list(&mut s), &SEVEN_DOWN);
}

#[test]
fn three_corners_over_then_down_print_two_full_rows_of_pages() {
    let mut s = sheet(&refs(&["A1", "AD1", "A120"]));
    over_then_down(&mut s);
    expect(&page_list(&mut s), &SEVEN_OVER);
}

#[test]
fn an_l_shape_prints_the_blank_pages_below_its_middle_band() {
    let mut s = sheet(&l_shape());
    expect(&page_list(&mut s), &SEVEN_DOWN);
}

#[test]
fn an_l_shape_over_then_down_prints_two_full_rows_of_pages() {
    let mut s = sheet(&l_shape());
    over_then_down(&mut s);
    expect(&page_list(&mut s), &SEVEN_OVER);
}

/// The first band's only cell is on its third page, the last band's on its first.
#[test]
fn a_blank_middle_band_between_two_cells_prints_in_full() {
    let mut s = sheet(&refs(&["A120", "AD1"]));
    expect(&page_list(&mut s), &SEVEN_DOWN);
}

/// The last band stops at its own last cell (row 60, its second page); the band before it is full.
#[test]
fn only_the_last_band_stops_at_its_own_last_cell() {
    let cells: Vec<String> = (1..=120).map(|r| format!("A{r}")).chain((1..=60).map(|r| format!("AD{r}"))).collect();
    let mut s = sheet(&cells);
    expect(&page_list(&mut s), &FULL_GRID[..8]);
}

#[test]
fn a_middle_band_that_reaches_the_bottom_is_unchanged() {
    let mut s = sheet(&refs(&["A1", "A120", "M120", "AD60"]));
    expect(&page_list(&mut s), &FULL_GRID[..8]);
}

#[test]
fn a_cell_on_the_last_page_prints_the_full_grid() {
    let mut s = sheet(&refs(&["A1", "AD120"]));
    expect(&page_list(&mut s), &FULL_GRID);
}

#[test]
fn one_row_of_pages_is_unchanged() {
    let mut s = sheet(&refs(&["A1", "AD1"]));
    expect(&page_list(&mut s), &[("A:J", "1:1"), ("K:T", "1:1"), ("U:AD", "1:1")]);
}

#[test]
fn one_band_of_columns_is_unchanged() {
    let cells: Vec<String> = (1..=120).map(|r| format!("A{r}")).collect();
    let mut s = sheet(&cells);
    expect(&page_list(&mut s), &[("A:A", "1:45"), ("A:A", "46:90"), ("A:A", "91:120")]);
}

/// A print area always prints its full grid of pages.
#[test]
fn a_print_area_still_prints_its_full_grid() {
    let mut s = sheet(&l_shape());
    s.execute("pageLayout.printArea", json!({"range": "A1:AD120"})).unwrap();
    expect(&page_list(&mut s), &FULL_GRID);
}
