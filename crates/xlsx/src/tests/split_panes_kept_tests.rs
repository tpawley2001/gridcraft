//! A split window (`<pane>` with `state="split"` or no `state`; ECMA-376 Part 1 §18.3.1.66)
//! survives open + save and is not turned into frozen panes. Packages are built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

/// A one-sheet workbook whose `<sheetView>` holds `inner` (a `<pane>` and `<selection>`s).
fn book(inner: &str) -> Vec<u8> {
    let body = format!(
        r#"<sheetViews><sheetView workbookViewId="0">{inner}</sheetView></sheetViews><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#
    );
    minimal(&body, &[], "", "")
}

/// Open + save `inner`; the saved `<sheetView>`.
fn saved_view(inner: &str) -> El {
    let out = write_xlsx(&read_xlsx(&book(inner)).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let sheet = xml::parse(s.as_bytes()).unwrap();
    sheet.child("sheetViews").expect("sheetViews").child("sheetView").expect("sheetView").clone()
}

/// A numeric attribute as written (`16230` and `16230.0` are the same split).
fn number(e: &El, name: &str) -> Option<f64> {
    e.attr(name).map(|v| v.parse::<f64>().unwrap_or_else(|_| panic!("{name}={v:?}")))
}

fn is_split(pane: &El) -> bool {
    matches!(pane.attr("state"), None | Some("split"))
}

#[test]
fn a_four_way_split_is_kept() {
    let v = saved_view(
        r#"<pane xSplit="16230" ySplit="3840" topLeftCell="AQ1422" activePane="bottomLeft"/><selection pane="bottomLeft" activeCell="A6" sqref="A6"/>"#,
    );
    let pane = v.child("pane").expect("the split pane is written");
    assert!(is_split(pane), "state={:?}", pane.attr("state"));
    assert_eq!(number(pane, "xSplit"), Some(16230.0));
    assert_eq!(number(pane, "ySplit"), Some(3840.0));
    assert_eq!(pane.attr("topLeftCell"), Some("AQ1422"));
    assert_eq!(pane.attr("activePane"), Some("bottomLeft"));
    let sel = v.kids("selection").find(|s| s.attr("pane") == Some("bottomLeft")).expect("the active pane's selection");
    assert_eq!(sel.attr("activeCell"), Some("A6"));
}

#[test]
fn a_vertical_split_alone_is_kept() {
    let v = saved_view(r#"<pane xSplit="12540" topLeftCell="K1"/><selection activeCell="B2" sqref="B2"/>"#);
    let pane = v.child("pane").expect("the split pane is written");
    assert!(is_split(pane), "state={:?}", pane.attr("state"));
    assert_eq!(number(pane, "xSplit"), Some(12540.0));
    assert!(number(pane, "ySplit").is_none_or(|y| y == 0.0), "ySplit={:?}", pane.attr("ySplit"));
    assert_eq!(pane.attr("topLeftCell"), Some("K1"));
    assert!(matches!(pane.attr("activePane"), None | Some("topLeft")), "activePane={:?}", pane.attr("activePane"));
    let sel = v.kids("selection").next().expect("a selection");
    assert_eq!(sel.attr("activeCell"), Some("B2"));
}

#[test]
fn an_explicit_split_state_is_kept_as_a_split() {
    let v = saved_view(
        r#"<pane ySplit="2400" topLeftCell="A9" activePane="bottomLeft" state="split"/><selection pane="bottomLeft" activeCell="A9" sqref="A9"/>"#,
    );
    let pane = v.child("pane").expect("the split pane is written");
    assert!(is_split(pane), "state={:?}", pane.attr("state"));
    assert_eq!(number(pane, "ySplit"), Some(2400.0));
    assert!(number(pane, "xSplit").is_none_or(|x| x == 0.0), "xSplit={:?}", pane.attr("xSplit"));
    assert_eq!(pane.attr("topLeftCell"), Some("A9"));
}

/// A split is not frozen panes: the model's freeze stays empty.
#[test]
fn a_split_is_not_read_as_frozen_panes() {
    let (wb, _) = read_xlsx(&book(r#"<pane xSplit="16230" ySplit="3840" topLeftCell="D5" activePane="bottomRight"/>"#)).unwrap();
    assert_eq!(wb.sheets[0].freeze, None);
    let v = saved_view(r#"<pane xSplit="16230" ySplit="3840" topLeftCell="D5" activePane="bottomRight"/>"#);
    assert!(v.child("pane").is_some_and(is_split), "a split pane is written, not a frozen one");
}

/// Frozen panes are written as before.
#[test]
fn frozen_panes_stay_frozen() {
    let v = saved_view(
        r#"<pane xSplit="1" ySplit="2" topLeftCell="B3" activePane="bottomRight" state="frozen"/><selection pane="bottomRight" activeCell="C4" sqref="C4"/>"#,
    );
    let pane = v.child("pane").expect("pane");
    assert_eq!(pane.attr("state"), Some("frozen"));
    assert_eq!(number(pane, "xSplit"), Some(1.0));
    assert_eq!(number(pane, "ySplit"), Some(2.0));
    assert_eq!(pane.attr("topLeftCell"), Some("B3"));
    assert_eq!(pane.attr("activePane"), Some("bottomRight"));
}

/// A sheet without a pane gains none.
#[test]
fn no_pane_stays_no_pane() {
    let v = saved_view(r#"<selection activeCell="B2" sqref="B2"/>"#);
    assert!(v.child("pane").is_none());
}

/// Split positions that are not usable numbers never fail the import and are never written.
#[test]
fn hostile_split_positions_are_dropped() {
    for bad in ["abc", "-5", "NaN", "inf", "1e308", ""] {
        let inner = format!(r#"<pane xSplit="{bad}" ySplit="{bad}" topLeftCell="B2" activePane="bottomRight"/>"#);
        let v = saved_view(&inner);
        assert!(v.child("pane").is_none(), "xSplit={bad:?} wrote {:?}", v.child("pane").map(|p| (p.attr("xSplit"), p.attr("ySplit"))));
    }
    // One usable axis is kept, the other dropped.
    let v = saved_view(r#"<pane xSplit="abc" ySplit="3000" topLeftCell="A7" activePane="bottomLeft"/>"#);
    let pane = v.child("pane").expect("the horizontal split is kept");
    assert_eq!(number(pane, "ySplit"), Some(3000.0));
    assert!(number(pane, "xSplit").is_none_or(|x| x == 0.0), "xSplit={:?}", pane.attr("xSplit"));
}
