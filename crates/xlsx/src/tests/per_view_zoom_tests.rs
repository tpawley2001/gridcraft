//! Each view's own zoom on `<sheetView>` (ECMA-376 Part 1 §18.3.1.87: `zoomScale` for the current
//! view, `zoomScaleNormal`, `zoomScaleSheetLayoutView` for Page Break Preview and
//! `zoomScalePageLayoutView`) survives open + save. Packages are built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

/// Open + save a one-sheet workbook whose `<sheetView>` carries `attrs`; the saved `<sheetView>`.
fn saved_view(attrs: &str) -> El {
    let body =
        format!(r#"<sheetViews><sheetView{attrs} workbookViewId="0"/></sheetViews><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#);
    let out = write_xlsx(&read_xlsx(&minimal(&body, &[], "", "")).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let sheet = xml::parse(s.as_bytes()).unwrap();
    sheet.child("sheetViews").expect("sheetViews").child("sheetView").expect("sheetView").clone()
}

/// (zoomScale, zoomScaleNormal, zoomScaleSheetLayoutView, zoomScalePageLayoutView); an absent
/// attribute (or 0) counts as 100.
fn zooms(v: &El) -> (u32, u32, u32, u32) {
    let z = |a: &str| match v.attr_u32(a) {
        None | Some(0) => 100,
        Some(n) => n,
    };
    (z("zoomScale"), z("zoomScaleNormal"), z("zoomScaleSheetLayoutView"), z("zoomScalePageLayoutView"))
}

#[test]
fn page_break_preview_keeps_the_normal_view_zoom() {
    let v = saved_view(r#" view="pageBreakPreview" zoomScale="60" zoomScaleNormal="100" zoomScaleSheetLayoutView="60""#);
    assert_eq!(v.attr("view"), Some("pageBreakPreview"));
    assert_eq!(zooms(&v), (60, 100, 60, 100));
}

#[test]
fn normal_view_keeps_the_page_layout_zoom() {
    let v = saved_view(r#" zoomScale="90" zoomScaleNormal="90" zoomScalePageLayoutView="150""#);
    assert_eq!(v.attr("view"), None);
    assert_eq!(zooms(&v), (90, 90, 100, 150));
}

#[test]
fn normal_view_at_100_keeps_the_page_break_preview_zoom() {
    let v = saved_view(r#" zoomScaleNormal="100" zoomScaleSheetLayoutView="75""#);
    assert_eq!(zooms(&v), (100, 100, 75, 100));
}

#[test]
fn page_layout_view_keeps_the_normal_view_zoom() {
    let v = saved_view(r#" view="pageLayout" zoomScale="120" zoomScaleNormal="80" zoomScalePageLayoutView="120""#);
    assert_eq!(v.attr("view"), Some("pageLayout"));
    assert_eq!(zooms(&v), (120, 80, 100, 120));
}

#[test]
fn a_preview_zoom_alone_is_not_copied_to_normal_view() {
    let v = saved_view(r#" view="pageBreakPreview" zoomScale="60""#);
    assert_eq!(zooms(&v), (60, 100, 60, 100));
}

#[test]
fn per_view_zooms_are_stable_over_a_second_save() {
    let first =
        saved_view(r#" view="pageBreakPreview" zoomScale="60" zoomScaleNormal="100" zoomScaleSheetLayoutView="60" zoomScalePageLayoutView="130""#);
    assert_eq!(zooms(&first), (60, 100, 60, 130));
    let mut attrs = String::new();
    for a in ["view", "zoomScale", "zoomScaleNormal", "zoomScaleSheetLayoutView", "zoomScalePageLayoutView"] {
        if let Some(val) = first.attr(a) {
            attrs.push_str(&format!(r#" {a}="{val}""#));
        }
    }
    assert_eq!(zooms(&saved_view(&attrs)), (60, 100, 60, 130));
}

#[test]
fn a_sheet_without_zoom_attributes_writes_none() {
    let v = saved_view("");
    for a in ["zoomScale", "zoomScaleNormal", "zoomScaleSheetLayoutView", "zoomScalePageLayoutView"] {
        assert_eq!(v.attr(a), None, "{a}");
    }
}

#[test]
fn out_of_range_per_view_zooms_count_as_not_stated() {
    let v = saved_view(r#" zoomScale="90" zoomScaleNormal="90" zoomScaleSheetLayoutView="0" zoomScalePageLayoutView="5000""#);
    assert_eq!(zooms(&v), (90, 90, 100, 100));
}
