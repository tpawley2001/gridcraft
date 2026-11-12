//! Frozen panes that were a split before freezing (`<pane state="frozenSplit">`; ECMA-376 Part 1
//! §18.3.1.66, ST_PaneState §18.18.53) keep that state through open + save, so unfreezing in
//! Excel restores the split. Packages are built in code.

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

/// The `<sheetView>` of the first sheet in the package `bytes`.
fn view_of(bytes: &[u8]) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let sheet = xml::parse(s.as_bytes()).unwrap();
    sheet.child("sheetViews").expect("sheetViews").child("sheetView").expect("sheetView").clone()
}

/// Open + save `bytes`.
fn resave(bytes: &[u8]) -> Vec<u8> {
    write_xlsx(&read_xlsx(bytes).unwrap().0).unwrap()
}

/// Open + save `inner`; the saved `<sheetView>`.
fn saved_view(inner: &str) -> El {
    view_of(&resave(&book(inner)))
}

/// A count attribute as written (`6` and `6.0` are the same count); absent counts as 0.
fn count(e: &El, name: &str) -> f64 {
    e.attr(name).map_or(0.0, |v| v.parse::<f64>().unwrap_or_else(|_| panic!("{name}={v:?}")))
}

/// What Microsoft 365 Excel writes after View > Split, then Freeze Panes at G4.
const EXCEL_FROZEN_SPLIT: &str = r#"<pane xSplit="6" ySplit="3" topLeftCell="G4" activePane="bottomRight" state="frozenSplit"/><selection pane="topRight" activeCell="G1" sqref="G1"/><selection pane="bottomLeft" activeCell="A4" sqref="A4"/><selection pane="bottomRight"/>"#;

#[test]
fn frozen_split_rows_and_columns_stay_frozen_split() {
    let v = saved_view(EXCEL_FROZEN_SPLIT);
    let pane = v.child("pane").expect("the pane is written");
    assert_eq!(pane.attr("state"), Some("frozenSplit"));
    assert_eq!(count(pane, "xSplit"), 6.0);
    assert_eq!(count(pane, "ySplit"), 3.0);
    assert_eq!(pane.attr("topLeftCell"), Some("G4"));
    assert_eq!(pane.attr("activePane"), Some("bottomRight"));
}

#[test]
fn frozen_split_rows_only_stay_frozen_split() {
    let v = saved_view(
        r#"<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozenSplit"/><selection pane="bottomLeft" activeCell="A2" sqref="A2"/>"#,
    );
    let pane = v.child("pane").expect("the pane is written");
    assert_eq!(pane.attr("state"), Some("frozenSplit"));
    assert_eq!(count(pane, "ySplit"), 1.0);
    assert_eq!(count(pane, "xSplit"), 0.0);
    assert_eq!(pane.attr("activePane"), Some("bottomLeft"));
}

#[test]
fn frozen_split_columns_only_stay_frozen_split() {
    let v = saved_view(
        r#"<pane xSplit="2" topLeftCell="C1" activePane="topRight" state="frozenSplit"/><selection pane="topRight" activeCell="C1" sqref="C1"/>"#,
    );
    let pane = v.child("pane").expect("the pane is written");
    assert_eq!(pane.attr("state"), Some("frozenSplit"));
    assert_eq!(count(pane, "xSplit"), 2.0);
    assert_eq!(count(pane, "ySplit"), 0.0);
    assert_eq!(pane.attr("activePane"), Some("topRight"));
}

#[test]
fn plain_frozen_panes_stay_frozen() {
    let v = saved_view(
        r#"<pane xSplit="6" ySplit="3" topLeftCell="G4" activePane="bottomRight" state="frozen"/><selection pane="bottomRight" activeCell="G4" sqref="G4"/>"#,
    );
    let pane = v.child("pane").expect("the pane is written");
    assert_eq!(pane.attr("state"), Some("frozen"));
    assert_eq!(count(pane, "xSplit"), 6.0);
    assert_eq!(count(pane, "ySplit"), 3.0);
}

#[test]
fn frozen_split_that_freezes_nothing_writes_no_frozen_pane() {
    let v = saved_view(r#"<pane xSplit="0" ySplit="0" topLeftCell="A1" state="frozenSplit"/><selection activeCell="A1" sqref="A1"/>"#);
    let frozen = v.kids("pane").any(|p| matches!(p.attr("state"), Some("frozen") | Some("frozenSplit")));
    assert!(!frozen, "a pane that freezes no row or column is not written as frozen");
}

#[test]
fn frozen_split_survives_a_second_save() {
    let first = resave(&book(EXCEL_FROZEN_SPLIT));
    let second = resave(&first);
    let (a, b) = (view_of(&first), view_of(&second));
    let (pa, pb) = (a.child("pane").expect("first save"), b.child("pane").expect("second save"));
    assert_eq!(pb.attr("state"), Some("frozenSplit"));
    for name in ["xSplit", "ySplit", "topLeftCell", "activePane", "state"] {
        assert_eq!(pa.attr(name), pb.attr(name), "{name}");
    }
}
