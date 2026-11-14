//! Cells that hold no value and no formula, only formatting, add pages only when their
//! formatting shows on paper (a fill or a border), as Excel prints. Measured from Excel's PDFs
//! of probe workbooks: A1 = 1 and ONE formatted-but-empty thing at row 120 (third page down) or
//! column AD (a later band of columns across); Letter portrait, default margins, column widths and
//! row heights: 45 rows per page.
use serde_json::json;

use super::*;

/// A new workbook with A1 = 1.
fn s() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    s
}

/// (columns, rows) of every printed page, in print order.
fn page_list(s: &mut Session) -> Vec<(String, String)> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| (p["columns"].as_str().unwrap().to_string(), p["rows"].as_str().unwrap().to_string())).collect()
}

fn pages(s: &mut Session) -> usize {
    page_list(s).len()
}

/// Excel: 1 page (probe sheets font, fontsize, numfmt, align, wrap, unlocked).
#[test]
fn font_number_format_alignment_or_wrap_alone_add_no_pages() {
    for (cmd, params) in [
        ("home.bold", json!({"range": "A120", "on": true})),
        ("home.fontSize", json!({"range": "A120", "size": 20})),
        ("home.numberFormat", json!({"range": "A120", "code": "0.00"})),
        ("home.alignCenter", json!({"range": "A120"})),
        ("home.wrapText", json!({"range": "A120", "on": true})),
    ] {
        let mut s = s();
        s.execute(cmd, params).unwrap();
        assert_eq!(pages(&mut s), 1, "{cmd}: {:?}", page_list(&mut s));
    }
}

/// Excel: 3 pages (probe sheets fill, border, box): a fill or any border side prints.
#[test]
fn a_fill_or_a_border_on_an_empty_cell_still_prints() {
    for (cmd, params) in [
        ("home.fillColor", json!({"range": "A120", "color": "#FFFF00"})),
        ("home.borders", json!({"range": "A120", "preset": "bottom"})),
        ("home.borders", json!({"range": "A120", "preset": "all"})),
    ] {
        let mut s = s();
        s.execute(cmd, params).unwrap();
        let p = page_list(&mut s);
        assert_eq!(p.len(), 3, "{cmd}: {p:?}");
        assert!(p[2].1.ends_with(":120"), "{cmd}: {p:?}");
    }
}

/// Excel: 1 page (probe 1112a manyfont: bold / 0.00 / centred cells A2:C300; 1112d manyface,
/// farcolmany). Excel: 7 pages (manyfill: filled cells A2:A300).
#[test]
fn many_font_only_cells_add_no_pages_but_many_filled_ones_do() {
    let mut s = s();
    s.execute("home.bold", json!({"range": "A2:A300", "on": true})).unwrap();
    s.execute("home.numberFormat", json!({"range": "B2:B300", "code": "0.00"})).unwrap();
    s.execute("home.alignCenter", json!({"range": "C2:C300"})).unwrap();
    assert_eq!(pages(&mut s), 1, "{:?}", page_list(&mut s));
    s.execute("home.fillColor", json!({"range": "A2:A300", "color": "#FFFF00"})).unwrap();
    let p = page_list(&mut s);
    assert_eq!(p.len(), 7, "{p:?}");
    assert!(p[6].1.ends_with(":300"), "{p:?}");
}

/// Excel: 2 pages (probe fontthenvalue: a value in A60, a bold empty A120): the range ends at
/// the value.
#[test]
fn the_range_ends_at_the_last_value_not_the_last_font_only_cell() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A60", "values": [[2]]})).unwrap();
    s.execute("home.bold", json!({"range": "A120", "on": true})).unwrap();
    let p = page_list(&mut s);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(p[1].1.ends_with(":60"), "{p:?}");
}

/// Excel: 1 page across for a bold empty AD1 (probe farcolfont), a second band for a filled or
/// bordered one (farcolfill, farcolborder).
#[test]
fn far_columns_count_only_for_a_fill_or_border() {
    let mut s = s();
    s.execute("home.bold", json!({"range": "AD1", "on": true})).unwrap();
    assert_eq!(pages(&mut s), 1, "{:?}", page_list(&mut s));
    s.execute("home.fillColor", json!({"range": "AD1", "color": "#FFFF00"})).unwrap();
    let p = page_list(&mut s);
    assert!(p.len() >= 2, "{p:?}");
    assert!(p.last().unwrap().0.ends_with(":AD"), "{p:?}");
}

/// Excel: 3 pages (probe merge: A120:B121 merged, no value, no style). A merged range prints
/// even when none of its cells is stored.
#[test]
fn an_empty_merged_range_still_prints() {
    let mut s = s();
    s.execute("home.mergeCells", json!({"range": "A120:B121"})).unwrap();
    let p = page_list(&mut s);
    assert_eq!(p.len(), 3, "{p:?}");
    assert!(p[2].1.ends_with(":121"), "{p:?}");
}

/// Excel: 1 page (probe rowhidden: the value 5 in A120, row 120 hidden): cells in hidden rows do
/// not extend the printed range.
#[test]
fn a_value_in_a_hidden_row_does_not_extend_the_range() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A120", "values": [[5]]})).unwrap();
    s.execute("home.hideRows", json!({"rows": "120:120"})).unwrap();
    assert_eq!(pages(&mut s), 1, "{:?}", page_list(&mut s));
}

/// Excel: 3 pages (probe formulaempty: A120 = ""): a formula counts whatever it shows.
#[test]
fn a_formula_showing_nothing_still_counts() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A120", "values": [["=\"\""]]})).unwrap();
    assert_eq!(pages(&mut s), 3, "{:?}", page_list(&mut s));
}

/// Overflow text keeps printing as far as it did before this rule, never past the last stored
/// column: a long text in A1 over bold empty cells B1:Z1 prints its later band(s) up to Z.
#[test]
fn text_overflowing_over_font_only_cells_keeps_its_pages() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [["x".repeat(400)]]})).unwrap();
    s.execute("home.bold", json!({"range": "B1:Z1", "on": true})).unwrap();
    let p = page_list(&mut s);
    assert!(p.len() >= 2, "{p:?}");
    assert!(p.last().unwrap().0.ends_with(":Z"), "{p:?}");
}
