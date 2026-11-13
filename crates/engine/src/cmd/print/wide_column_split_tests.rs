//! A column wider than the printable page is cut to the page width; only text longer than the cut
//! column continues on following pages (measured from Microsoft 365 Excel's own PDF print of a
//! probe workbook, 2026-11-12: Letter portrait, 0.5 in side margins = 540 pt printable, column A
//! 108.57 or 250 characters wide. A number alone in that column prints one page with the number
//! against the page's right edge; a 30-word text prints two pages, the second showing the same
//! line shifted left by the printable width).
use serde_json::json;

use super::*;

/// Letter portrait with the default 0.7 in side margins: the printable area is x = 50.4..561.6.
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

/// Letter portrait with 0.5 in side margins, as in the measured workbook: 612 - 2 * 36.
const PROBE_PRINTABLE_WIDTH: f64 = 540.0;
const PROBE_RIGHT_EDGE: f32 = 576.0;

/// A new workbook with 0.5 in side margins and column `col` 108.57 characters wide.
fn probe_sheet(col: &str, chars: f64) -> Session {
    let mut s = s();
    s.execute("pageLayout.pageSetup", json!({"left": 0.5, "right": 0.5})).unwrap();
    s.execute("home.columnWidth", json!({"cols": format!("{col}:{col}"), "chars": chars})).unwrap();
    s
}

/// `w001 w002 ...`: one text of `n` four-letter words, far longer than a page when `n` is 30.
fn words(n: usize) -> String {
    (1..=n).map(|i| format!("w{i:03}")).collect::<Vec<_>>().join(" ")
}

/// B1:L1 = 2..12.
fn numbers_b_to_l(s: &mut Session) {
    let row: Vec<Json> = (2..=12).map(|i| json!(i)).collect();
    s.execute("range.setValues", json!({"range": "B1", "values": [row]})).unwrap();
}

#[test]
fn wide_column_split_prints_one_page_when_nothing_is_longer_than_the_page() {
    let mut s = wide_column();
    s.execute("range.setValues", json!({"range": "A1", "values": [["HEAD"], ["TAIL"]]})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1:A2"], "Excel prints the cut column on one page");
    assert_eq!(list[0]["columns"], "A:A");
    assert_eq!(offsets(&list), [None]);
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(r["pages"], 1);
}

#[test]
fn wide_column_split_draws_right_aligned_text_at_the_page_edge() {
    let mut s = wide_column();
    s.execute("range.setValues", json!({"range": "A1", "values": [["HEAD"], ["TAIL"]]})).unwrap();
    s.execute("home.alignRight", json!({"range": "A2"})).unwrap();
    let pages = pdf_pages(&mut s);
    assert_eq!(pages.len(), 1);
    let head = text_x(&pages[0], "HEAD").expect("HEAD is drawn on page 1");
    assert!((LEFT_MARGIN..LEFT_MARGIN + 10.0).contains(&head), "HEAD at the left margin of page 1: {head}");
    // The column is laid out one printable width wide, so right-aligned text ends at the page's
    // right edge (4 letters of an 11 pt font start within 45 pt of it).
    let tail = text_x(&pages[0], "TAIL").expect("TAIL is drawn on page 1");
    assert!((RIGHT_EDGE - 45.0..RIGHT_EDGE).contains(&tail), "TAIL against the right edge of page 1: {tail}");
}

#[test]
fn wide_column_split_draws_a_number_at_the_page_edge() {
    // Excel: the number 1 is drawn at x = 569.0..574.6 of a page whose printable area ends at 576.
    for chars in [108.57, 250.0] {
        let mut s = probe_sheet("A", chars);
        s.execute("cell.set", json!({"cell": "A1", "input": "1"})).unwrap();
        let pages = pdf_pages(&mut s);
        assert_eq!(pages.len(), 1, "one page for a {chars}-character column");
        let one = text_x(&pages[0], "1").expect("the number is drawn on page 1");
        assert!((PROBE_RIGHT_EDGE - 12.0..PROBE_RIGHT_EDGE).contains(&one), "{chars} characters: the number at {one}");
    }
}

#[test]
fn wide_column_split_prints_one_page_for_a_filled_cell() {
    let mut s = probe_sheet("A", 108.57);
    s.execute("cell.set", json!({"cell": "A1", "input": "1"})).unwrap();
    s.execute("home.fillColor", json!({"range": "A1", "color": "#FFFF00"})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1"], "the fill is cut to the page too: no second page");
    assert_eq!(offsets(&list), [None]);
}

#[test]
fn wide_column_split_prints_one_page_however_wide_the_column_is() {
    let mut s = s();
    // 250 characters = 1755 px = 1316.25 pt, more than two printable widths.
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 250})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "x"})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1"]);
    assert_eq!(offsets(&list), [None]);
}

#[test]
fn wide_column_split_pairs_the_cut_column_with_every_row_band() {
    let mut s = wide_column();
    let rows: Vec<Vec<Json>> = (1..=60).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    // 45 rows of 15 pt per page; numbers never continue past the cut column.
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1:A45", "A46:A60"]);
    assert_eq!(offsets(&list), [None, None]);
}

#[test]
fn wide_column_split_continues_long_text_on_a_second_page() {
    // Excel: 2 pages; page 2 shows the same line shifted left by the printable width.
    let mut s = probe_sheet("A", 108.57);
    s.execute("cell.set", json!({"cell": "A1", "input": words(30)})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1", "A1"]);
    let offsets = offsets(&list);
    assert_eq!(offsets[0], None);
    assert!(near(offsets[1], PROBE_PRINTABLE_WIDTH), "{:?}", offsets[1]);
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(r["pages"], 2);
}

#[test]
fn wide_column_split_stops_long_text_at_a_filled_neighbour() {
    // Excel: 2 pages, A (the text clipped at the page edge) then B.
    let mut s = probe_sheet("A", 108.57);
    s.execute("cell.set", json!({"cell": "A1", "input": words(30)})).unwrap();
    s.execute("cell.set", json!({"cell": "B1", "input": "2"})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1", "B1"]);
    assert_eq!(offsets(&list), [None, None]);
}

#[test]
fn wide_column_split_starts_the_next_columns_on_a_new_page() {
    // Excel: A alone on page 1, B onwards from page 2, with a short number or a long text in A1.
    for input in ["1".to_string(), words(30)] {
        let mut s = probe_sheet("A", 108.57);
        s.execute("cell.set", json!({"cell": "A1", "input": input})).unwrap();
        numbers_b_to_l(&mut s);
        let list = preview(&mut s);
        let ranges = ranges(&list);
        assert_eq!(ranges[0], "A1", "{ranges:?}");
        assert!(ranges[1].starts_with("B1"), "B starts page 2: {ranges:?}");
        assert!(ranges.iter().skip(1).all(|r| !r.starts_with("A1")), "A is on page 1 only: {ranges:?}");
        assert!(offsets(&list).iter().all(Option::is_none), "no continuation pages: {:?}", offsets(&list));
    }
}

#[test]
fn wide_column_split_continues_long_text_after_a_narrow_column() {
    // Excel: 3 pages, A | B | the rest of B1's text.
    let mut s = probe_sheet("B", 108.57);
    s.execute("cell.set", json!({"cell": "A1", "input": "1"})).unwrap();
    s.execute("cell.set", json!({"cell": "B1", "input": words(30)})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1", "B1", "B1"]);
    let offsets = offsets(&list);
    assert_eq!(&offsets[..2], [None, None]);
    assert!(near(offsets[2], PROBE_PRINTABLE_WIDTH), "{:?}", offsets[2]);
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

/// Column A 250 characters wide, 0.5 in side margins, A1 = 40 words centred: Excel's probe sheet
/// `fit200center` (Microsoft 365 Excel's own PDF print, 2026-11-13).
fn centred_words(n: usize) -> Session {
    let mut s = probe_sheet("A", 250.0);
    s.execute("cell.set", json!({"cell": "A1", "input": words(n)})).unwrap();
    s.execute("home.alignCenter", json!({"range": "A1"})).unwrap();
    s
}

#[test]
fn wide_column_split_continues_centred_text_past_the_cut() {
    // Excel: 2 pages. The line is centred on the cut column (the printable width), so it runs on
    // past the page's right edge by about (1090 - 540) / 2 pt and continues on one more page.
    let mut s = centred_words(40);
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1", "A1"], "Excel prints the centred line on 2 pages");
    let offsets = offsets(&list);
    assert_eq!(offsets[0], None);
    assert!(near(offsets[1], PROBE_PRINTABLE_WIDTH), "{:?}", offsets[1]);
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(r["pages"], 2);
}

#[test]
fn wide_column_split_draws_centred_text_centred_on_the_cut_and_shifted_on_the_next_page() {
    // Excel page 1: the line starts at x = -239.0 (its first nine words are clipped off the left
    // of the page; it is centred on x = 306, the middle of the printable area, not of the
    // 250-character column). Page 2: the same line shifted left by exactly the printable width.
    let text = words(40);
    let mut s = centred_words(40);
    let pages = pdf_pages(&mut s);
    assert_eq!(pages.len(), 2);
    let first = text_x(&pages[0], &text).expect("the line is drawn on page 1");
    // Centred on the 250-character column instead, the line would start right of the margin.
    // The range allows for small text-width differences (GridCraft measured 2026-11-13: -279.4).
    assert!((-330.0..-200.0).contains(&first), "centred on the cut column (Excel: -239.0): {first}");
    let second = text_x(&pages[1], &text).expect("the line is drawn again on page 2");
    assert!(
        (second - (first - PROBE_PRINTABLE_WIDTH as f32)).abs() < 0.5,
        "page 2 shows the line shifted left by the printable width: page 1 {first}, page 2 {second}"
    );
}

#[test]
fn wide_column_split_keeps_centred_text_within_the_cut_on_one_page() {
    // 15 words = 74 characters, about 400 pt: centred inside the 540 pt cut, nothing runs on.
    let mut s = centred_words(15);
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1"]);
    assert_eq!(offsets(&list), [None]);
}

#[test]
fn wide_column_split_centred_text_stops_at_a_filled_neighbour() {
    // B1 holds a value: the centred line cannot run on over it, so no continuation page (as for
    // left-aligned text); B starts the next page.
    let mut s = centred_words(40);
    s.execute("cell.set", json!({"cell": "B1", "input": "2"})).unwrap();
    let list = preview(&mut s);
    assert_eq!(ranges(&list), ["A1", "B1"]);
    assert_eq!(offsets(&list), [None, None]);
}
