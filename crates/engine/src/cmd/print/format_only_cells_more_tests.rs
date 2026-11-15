//! More cases of the rule in `format_only_cells_tests`: formatting-only cells over a wide range,
//! a later band of columns, and a print area. Letter portrait, default margins, column widths and
//! row heights.
use serde_json::json;

use super::*;

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

/// Bold over A1:Z900: only A1 prints. A border on D40 ends the range there; a fill on D400 ends
/// it at D400.
#[test]
fn a_bold_block_prints_a1_only_until_a_border_or_fill_marks_the_paper() {
    let mut s = s();
    s.execute("home.bold", json!({"range": "A1:Z900", "on": true})).unwrap();
    let p = page_list(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0], ("A:A".to_string(), "1:1".to_string()), "{p:?}");

    s.execute("home.borders", json!({"range": "D40", "preset": "bottom"})).unwrap();
    let p = page_list(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].0.ends_with(":D") && p[0].1 == "1:40", "{p:?}");

    s.execute("home.fillColor", json!({"range": "D400", "color": "#FFFF00"})).unwrap();
    let p = page_list(&mut s);
    assert!(p.len() > 1, "{p:?}");
    assert!(p.iter().all(|q| q.0.ends_with(":D")), "{p:?}");
    assert!(p.last().unwrap().1.ends_with(":400"), "{p:?}");
}

#[test]
fn a_number_format_or_wrap_over_a_column_prints_one_page() {
    for (cmd, params) in
        [("home.numberFormat", json!({"range": "A1:A900", "code": "0.00"})), ("home.wrapText", json!({"range": "A1:A900", "on": true}))]
    {
        let mut s = s();
        s.execute(cmd, params).unwrap();
        let p = page_list(&mut s);
        assert_eq!(p.len(), 1, "{cmd}: {p:?}");
    }
}

/// A1:A200 values, a value in P1 and font-only cells P2:P900: five pages of the first band and
/// one of P's band.
#[test]
fn font_only_cells_below_a_later_bands_value_add_no_pages() {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Vec<Json>> = (1..=200).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("range.setValues", json!({"range": "P1", "values": [["x"]]})).unwrap();
    s.execute("home.bold", json!({"range": "P2:P900", "on": true})).unwrap();
    let p = page_list(&mut s);
    let first = &p[0].0;
    let first_band = p.iter().filter(|q| &q.0 == first).count();
    assert_eq!(first_band, 5, "{p:?}");
    assert!(p[4].1.ends_with(":200"), "{p:?}");
    assert_eq!(p.len(), 6, "{p:?}");
    assert!(p[5].0.ends_with(":P") && p[5].1.starts_with("1:"), "{p:?}");
}

/// A long text over font-only cells, with nothing else: every band it reaches prints one page,
/// past the formatted cells to where the text ends.
#[test]
fn every_band_an_overflowing_text_reaches_prints_one_page() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [["x".repeat(400)]]})).unwrap();
    s.execute("home.bold", json!({"range": "B1:P1", "on": true})).unwrap();
    let p = page_list(&mut s);
    assert!(p.len() >= 2, "{p:?}");
    assert!(p.iter().all(|q| q.1 == "1:1"), "{p:?}");
    // 400 x's (about 2,100 pt) end in column AT.
    assert!(p.last().unwrap().0.ends_with(":AT"), "{p:?}");
}

/// A print area prints whole, formatting-only cells or not.
#[test]
fn a_print_area_over_font_only_cells_prints_whole() {
    let mut s = s();
    s.execute("home.bold", json!({"range": "A1:Z900", "on": true})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"printArea": "A1:Z900"})).unwrap();
    let p = page_list(&mut s);
    assert!(p.len() > 1, "{p:?}");
    assert!(p.last().unwrap().0.ends_with(":Z") && p.last().unwrap().1.ends_with(":900"), "{p:?}");
}

/// A value in a hidden row does not count, in a later band either.
#[test]
fn a_value_in_a_hidden_row_adds_no_band() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "AD5", "values": [[2]]})).unwrap();
    s.execute("home.hideRows", json!({"rows": "5:5"})).unwrap();
    let p = page_list(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
}
