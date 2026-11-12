//! A column wider than the printable page is split across pages, as Excel prints it (measured
//! from Excel's own PDFs of real workbooks: Letter portrait, 0.7 in side margins, one used column
//! 108.57 characters wide; Excel prints two pages, the second showing the same cells shifted left
//! by the printable width, 511.2 pt).
use serde_json::json;

use super::*;

/// Letter portrait with the default 0.7 in side margins: 612 - 2 * 50.4.
const PRINTABLE_WIDTH: f64 = 511.2;
const LEFT_MARGIN: f32 = 50.4;
const RIGHT_EDGE: f32 = 561.6;

fn s() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn preview(s: &mut Session) -> Vec<Json> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap().clone();
    assert_eq!(pv["pages"], list.len());
    list
}

fn ranges(list: &[Json]) -> Vec<String> {
    list.iter().map(|p| p["range"].as_str().unwrap().to_string()).collect()
}

/// The `columnOffset` of each page: `None` for a first piece or an ordinary page.
fn offsets(list: &[Json]) -> Vec<Option<f64>> {
    list.iter().map(|p| p.get("columnOffset").and_then(Json::as_f64)).collect()
}

fn near(got: Option<f64>, want: f64) -> bool {
    got.is_some_and(|g| (g - want).abs() < 0.5)
}

/// The content streams of an exported PDF's pages, in page order.
fn pdf_pages(s: &mut Session) -> Vec<String> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let pdf = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    let streams: Vec<String> = text.split("endstream").filter(|part| part.contains("stream\n")).map(str::to_string).collect();
    assert_eq!(streams.len() as u64, r["pages"].as_u64().unwrap());
    streams
}

/// The x position of the first `(text) Tj` in a page's content stream.
fn text_x(page: &str, text: &str) -> Option<f32> {
    let before = &page[..page.find(&format!("({text}) Tj"))?];
    let tm = before.rfind(" Tm")?;
    let mut nums = before[..tm].rsplit([' ', '\n']);
    let _y = nums.next()?;
    nums.next()?.parse().ok()
}

/// Column A as wide as in the measured workbooks: 108.57 characters = 765 px = 573.75 pt.
fn wide_column() -> Session {
    let mut s = s();
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 108.57})).unwrap();
    s
}

#[test]
fn wide_column_split_prints_two_pages_for_one_over_wide_column() {
    let mut s = wide_column();
    s.execute("range.setValues", json!({"range": "A1", "values": [["HEAD"], ["TAIL"]]})).unwrap();
    let list = preview(&mut s);
    assert_eq!(list.len(), 2, "Excel prints the column's rest on a second page");
    assert_eq!(list[0]["range"], list[1]["range"]);
    assert_eq!(list[0]["columns"], "A:A");
    assert_eq!(list[1]["columns"], "A:A");
    assert_eq!(list[0]["rows"], list[1]["rows"]);
    let offsets = offsets(&list);
    assert_eq!(offsets[0], None, "the first piece has no columnOffset");
    assert!(near(offsets[1], PRINTABLE_WIDTH), "the second piece starts one printable width into the column: {:?}", offsets[1]);
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(r["pages"], 2);
}

#[test]
fn wide_column_split_draws_the_rest_of_the_column_on_the_next_page() {
    let mut s = wide_column();
    s.execute("range.setValues", json!({"range": "A1", "values": [["HEAD"], ["TAIL"]]})).unwrap();
    // Right-aligned text sits wholly in the part of the column past the first page.
    s.execute("home.alignRight", json!({"range": "A2"})).unwrap();
    let pages = pdf_pages(&mut s);
    assert_eq!(pages.len(), 2);
    let head = text_x(&pages[0], "HEAD").expect("HEAD is drawn on page 1");
    assert!((LEFT_MARGIN..LEFT_MARGIN + 10.0).contains(&head), "HEAD at the left margin of page 1: {head}");
    let tail = text_x(&pages[1], "TAIL").expect("TAIL is drawn on page 2");
    assert!((LEFT_MARGIN..RIGHT_EDGE).contains(&tail), "TAIL inside page 2's printable area: {tail}");
    // The column's rest is narrower than half a page, so the text is in page 2's left half.
    assert!(tail < 306.0, "TAIL in the column's rest, shifted left by the printable width: {tail}");
}

#[test]
fn wide_column_split_makes_as_many_pieces_as_the_column_needs() {
    let mut s = s();
    // 250 characters = 1755 px = 1316.25 pt: three pieces of 511.2 pt.
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 250})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "x"})).unwrap();
    let list = preview(&mut s);
    assert_eq!(list.len(), 3);
    let offsets = offsets(&list);
    assert_eq!(offsets[0], None);
    assert!(near(offsets[1], PRINTABLE_WIDTH), "{:?}", offsets[1]);
    assert!(near(offsets[2], 2.0 * PRINTABLE_WIDTH), "{:?}", offsets[2]);
}

#[test]
fn wide_column_split_pairs_every_piece_with_every_row_band() {
    let mut s = wide_column();
    let rows: Vec<Vec<Json>> = (1..=60).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    // 45 rows of 15 pt per page; down, then over.
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1:A45", "A46:A60", "A1:A45", "A46:A60"]);
    let offsets = offsets(&list);
    assert_eq!(&offsets[..2], [None, None]);
    assert!(near(offsets[2], PRINTABLE_WIDTH) && near(offsets[3], PRINTABLE_WIDTH), "{offsets:?}");
}

#[test]
fn wide_column_split_leaves_a_column_that_fits_an_empty_page_whole() {
    let mut s = s();
    // 40 characters = 213.75 pt and 60 characters = 318.75 pt: together wider than the page,
    // each narrower. B moves to the next page whole, as before.
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 40})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "B:B", "chars": 60})).unwrap();
    s.execute("range.setValues", json!({"range": "A1", "values": [["a", "b"]]})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1", "B1"]);
    assert_eq!(offsets(&list), [None, None]);
}
