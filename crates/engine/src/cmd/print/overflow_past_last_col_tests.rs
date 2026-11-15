//! Text overflowing past the last used column widens the printed range to the column bands its
//! drawn text reaches, as Excel prints (measured in Excel 365). Every sheet: Letter portrait,
//! 0.5" margins, header "&A p&P", Calibri 11, default column widths and row heights; row 120
//! is on the third page down and the first band of columns is A:K, the second L:V, the third
//! W:AG. These are the cases of `tests/it/print_overflow_past_last_col.rs`, under the same names.
use serde_json::json;

use super::*;

/// A new workbook with Excel's probe page setup and the numbers in `cells`.
fn sheet(cells: &[&str]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "pageLayout.pageSetup",
        json!({"paper": "Letter", "orientation": "portrait", "margins": {"left": 0.5, "right": 0.5, "top": 0.5, "bottom": 0.5}, "header": "&A p&P"}),
    )
    .unwrap();
    for c in cells {
        s.execute("cell.set", json!({"cell": c, "input": "1"})).unwrap();
    }
    s
}

/// 1 in A1, L1 and A120.
fn base() -> Session {
    sheet(&["A1", "L1", "A120"])
}

fn set(s: &mut Session, cell: &str, text: &str) {
    s.execute("range.setValues", json!({"range": cell, "values": [[text]]})).unwrap();
}

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

/// The page count, after checking the layout's bands start where Excel's do: A:K first, then L
/// (a band is labelled up to the range's last column).
fn pages(s: &mut Session) -> usize {
    let list = page_list(s);
    assert_eq!(list.first().map(|p| p.0.as_str()), Some("A:K"), "{list:?}");
    if let Some(p) = list.iter().find(|p| p.0 != "A:K") {
        assert!(p.0.starts_with("L:"), "{list:?}");
    }
    list.len()
}

/// "abcdefghi " repeated: `n` characters.
fn words(n: usize) -> String {
    "abcdefghi ".repeat(n / 10 + 1).chars().take(n).collect()
}

#[test]
fn base_sheet_prints_four_pages() {
    assert_eq!(pages(&mut base()), 4);
}

#[test]
fn long_text_in_last_band_adds_its_bands() {
    let mut s = base();
    set(&mut s, "K120", &words(200));
    assert_eq!(pages(&mut s), 9, "{:?}", page_list(&mut s));
}

#[test]
fn short_overflow_adds_nothing() {
    let mut s = base();
    set(&mut s, "A120", &words(60));
    assert_eq!(pages(&mut s), 4);
}

#[test]
fn trailing_spaces_overflow() {
    let mut s = base();
    set(&mut s, "K120", &format!("x{}", " ".repeat(150)));
    assert_eq!(pages(&mut s), 6, "{:?}", page_list(&mut s));
}

#[test]
fn overflow_inside_used_bands() {
    let mut s = sheet(&["A1", "L1", "W1"]);
    set(&mut s, "K120", &words(40));
    assert_eq!(pages(&mut s), 7, "{:?}", page_list(&mut s));
}

#[test]
fn widened_range_prints_blank_pages() {
    let mut s = sheet(&["A1"]);
    set(&mut s, "A120", &words(400));
    let list = page_list(&mut s);
    assert_eq!(list.len(), 12, "{list:?}");
    // Four bands of columns, three pages down each (the blank ones included).
    let bands: Vec<&str> = list.iter().step_by(3).map(|p| p.0.as_str()).collect();
    assert_eq!(bands.len(), 4, "{list:?}");
    assert_eq!(bands[0], "A:K", "{list:?}");
    assert!(bands[1].starts_with("L:") && bands[3].starts_with("AH:"), "{list:?}");
}

#[test]
fn centred_overflow_spreads_both_ways() {
    let mut s = base();
    set(&mut s, "K120", &words(200));
    s.execute("home.alignCenter", json!({"range": "K120"})).unwrap();
    assert_eq!(pages(&mut s), 6, "{:?}", page_list(&mut s));
}

#[test]
fn hidden_text_does_not_widen() {
    // A hidden row.
    let mut s = base();
    set(&mut s, "K100", &words(400));
    s.execute("home.hideRows", json!({"rows": "100:100"})).unwrap();
    assert_eq!(pages(&mut s), 4, "{:?}", page_list(&mut s));
    // A zero-height row.
    let mut s = base();
    set(&mut s, "K100", &words(400));
    s.execute("home.rowHeight", json!({"rows": "100:100", "height": 0})).unwrap();
    assert_eq!(pages(&mut s), 4, "{:?}", page_list(&mut s));
    // A hidden column (the text's own column).
    let mut s = base();
    set(&mut s, "K100", &words(400));
    s.execute("home.hideColumns", json!({"cols": "K:K"})).unwrap();
    let list = page_list(&mut s);
    // K is gone from the first band, which now reaches L: nothing past L prints.
    assert!(list.iter().all(|p| p.0 == "A:L"), "{list:?}");
    // A zero-width column.
    let mut s = base();
    set(&mut s, "K100", &words(400));
    s.execute("home.columnWidth", json!({"cols": "K:K", "width": 0})).unwrap();
    let list = page_list(&mut s);
    // K is gone from the first band, which now reaches L: nothing past L prints.
    assert!(list.iter().all(|p| p.0 == "A:L"), "{list:?}");
}

#[test]
fn cjk_text_uses_drawn_width() {
    // 40 ideographs in SimSun 11 draw about 440 pt from K: they end inside L:V.
    let mut s = base();
    set(&mut s, "K120", &"漢字".repeat(20));
    s.execute("home.fontName", json!({"range": "K120", "name": "SimSun"})).unwrap();
    assert_eq!(pages(&mut s), 6, "{:?}", page_list(&mut s));
}

#[test]
fn one_cell_adds_only_reached_bands() {
    // H120 holds 2,000 characters: only the first 1,024 are drawn, and the range ends at the
    // column their drawn width reaches.
    let mut s = sheet(&["A1"]);
    set(&mut s, "H120", &words(2000));
    let wb = &s.doc().unwrap().wb;
    let sh = &wb.sheets[0];
    let range = layout::print_ranges(wb, sh);
    let end = range.first().unwrap().end;
    let lay = layout::layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap();
    let mut fonts = None;
    let c = CellRef::new(119, 7);
    let cell = sh.cells.get(c).unwrap();
    let last = text::right_overflow_end(wb, sh, (c, cell), &sh.merges, (lay.k(), lay.scale), &mut fonts).unwrap();
    assert_eq!(end.col, last);
    // The drawn width of the 1,024 displayed characters (about 4,750 pt, some 99 default
    // columns), not of the 2,000 stored ones (about twice that).
    assert!((80..120).contains(&(last - 7)), "{last}");
    // Exactly the bands it reaches, each three pages down.
    let bands = lay.col_bands.iter().filter(|b| b.0 <= last).count();
    assert_eq!(lay.col_bands.len(), bands);
    assert_eq!(lay.pages.len(), bands * 3);
    // More long cells in the other row bands reach no further.
    for r in ["H10", "H60"] {
        set(&mut s, r, &words(2000));
    }
    let wb = &s.doc().unwrap().wb;
    let again = layout::layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap();
    assert_eq!(again.col_bands, lay.col_bands);
}

#[test]
fn non_overflowing_cells_do_not_widen() {
    // Wrapped.
    let mut s = base();
    set(&mut s, "K120", &words(400));
    s.execute("home.wrapText", json!({"range": "K120", "on": true})).unwrap();
    // (Its taller row may move the page breaks; no band of columns is added.)
    let list = page_list(&mut s);
    assert!(list.iter().all(|p| p.0 == "A:K" || p.0 == "L:L"), "{list:?}");
    // A non-empty right neighbour.
    let mut s = base();
    set(&mut s, "K120", &words(400));
    s.execute("cell.set", json!({"cell": "L120", "input": "2"})).unwrap();
    assert_eq!(pages(&mut s), 6, "{:?}", page_list(&mut s));
    // Merged.
    let mut s = base();
    set(&mut s, "K120", &words(400));
    s.execute("home.mergeCells", json!({"range": "K120:L120"})).unwrap();
    assert_eq!(pages(&mut s), 6, "{:?}", page_list(&mut s));
}
