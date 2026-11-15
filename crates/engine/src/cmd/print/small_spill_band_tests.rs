//! Text that runs past its cell edge by only a few points adds no printed column: Excel's
//! printed range grows only for a spill of several points (measured against Excel 365 prints:
//! spills of 0.1 to 2.7 pt added no column, spills of 14.4 pt and more always did). Every sheet:
//! Letter portrait, 0.5" margins, numbers in A1, T1 and A120 (bands A:J and K:T), text in T120
//! with column T sized so the text runs past it by the given number of printed points.
use serde_json::json;

use super::*;

const T: u32 = 19;
const TEXT: &str = "abcdefghi abcdefghi abcdefghi";

fn base() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "pageLayout.pageSetup",
        json!({"paper": "Letter", "orientation": "portrait", "margins": {"left": 0.5, "right": 0.5, "top": 0.5, "bottom": 0.5}, "header": "&A p&P"}),
    )
    .unwrap();
    for c in ["A1", "T1", "A120"] {
        s.execute("cell.set", json!({"cell": c, "input": "1"})).unwrap();
    }
    s.execute("range.setValues", json!({"range": "T120", "values": [[TEXT]]})).unwrap();
    s
}

/// The page scale `(k, scale)` of the first sheet's layout.
fn scale(s: &Session) -> (f32, f32) {
    let wb = &s.doc().unwrap().wb;
    let lay = layout::layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap();
    (lay.k(), lay.scale)
}

/// The printed width of T120's text (points at page scale, padding included), measured by the
/// cut-column reach of left-aligned text (which is the full text width).
fn text_need(s: &Session) -> f32 {
    let (k, sc) = scale(s);
    let wb = &s.doc().unwrap().wb;
    let sh = &wb.sheets[0];
    let c = CellRef::new(119, T);
    let cell = sh.cells.get(c).unwrap();
    text::cut_text_reach(wb, sh, (c, cell), &sh.merges, (k, sc), 1.0, &mut None).expect("text wider than 1 pt")
}

/// Sizes column T so that a text needing `need` points runs `spill` points past T's printed width.
fn size_t(s: &mut Session, need: f32, spill: f32) {
    let (k, _) = scale(s);
    let cs = layout::print_col_scale(&s.doc().unwrap().wb);
    let px = (need - spill) / (k * cs);
    assert!(px > 1.0, "{px}");
    let wb = std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().wb);
    std::sync::Arc::make_mut(&mut wb.sheets[0]).cols.entry(T).or_default().size = Some(px);
}

fn overflow_end(s: &Session) -> Option<u32> {
    let (k, sc) = scale(s);
    let wb = &s.doc().unwrap().wb;
    let sh = &wb.sheets[0];
    let c = CellRef::new(119, T);
    let cell = sh.cells.get(c).unwrap();
    text::right_overflow_end(wb, sh, (c, cell), &sh.merges, (k, sc), &mut None)
}

/// Column T's text with the given spill (total, both sides for centred text) and alignment.
fn spill(total: f32, centred: bool) -> Option<u32> {
    let mut s = base();
    let need = text_need(&s);
    size_t(&mut s, need, total);
    if centred {
        s.execute("home.alignCenter", json!({"range": "T120"})).unwrap();
    }
    overflow_end(&s)
}

#[test]
fn spill_of_two_points_adds_no_column() {
    assert_eq!(spill(2.0, false), None);
}

#[test]
fn spill_of_four_points_adds_no_column() {
    assert_eq!(spill(4.0, false), None);
}

#[test]
fn spill_of_fifteen_points_adds_a_column() {
    assert_eq!(spill(15.0, false), Some(T + 1));
}

#[test]
fn spill_of_thirty_points_adds_a_column() {
    assert!(spill(30.0, false).is_some_and(|c| c > T));
}

#[test]
fn centred_spill_counts_one_side() {
    // 8 pt over in total: 4 pt each side.
    assert_eq!(spill(8.0, true), None);
    // 40 pt over in total: 20 pt each side.
    assert!(spill(40.0, true).is_some_and(|c| c > T));
}

#[test]
fn small_spill_in_last_column_keeps_the_bands() {
    let mut s = base();
    let need = text_need(&s);
    size_t(&mut s, need, 4.0);
    let wb = &s.doc().unwrap().wb;
    let lay = layout::layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap();
    assert_eq!(lay.col_bands.last().map(|b| b.1), Some(T), "{:?}", lay.col_bands);
    let mut s = base();
    let need = text_need(&s);
    size_t(&mut s, need, 30.0);
    let wb = &s.doc().unwrap().wb;
    let lay = layout::layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap();
    assert!(lay.col_bands.last().is_some_and(|b| b.1 > T), "{:?}", lay.col_bands);
}
