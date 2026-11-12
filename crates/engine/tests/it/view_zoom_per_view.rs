//! View › Workbook Views: each view of a sheet (Normal, Page Break Preview, Page Layout) has its
//! own zoom, as in Excel. Switching view stores the zoom of the view being left and shows the new
//! view at its stored zoom, or at its default (Normal 100, Page Break Preview 60, Page Layout 100).

use gridcraft_engine::Session;
use serde_json::json;

/// (zoom, zoom_normal, zoom_page_break, zoom_page_layout) of the active sheet.
type Zooms = (u16, Option<u16>, Option<u16>, Option<u16>);

fn zooms(s: &Session) -> Zooms {
    let sh = s.doc().unwrap().wb.active().unwrap();
    (sh.zoom, sh.zoom_normal, sh.zoom_page_break, sh.zoom_page_layout)
}

fn new_session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, id: &str) {
    s.execute(id, json!({})).unwrap();
}

fn zoom(s: &mut Session, percent: u16) {
    s.execute("view.zoom", json!({"percent": percent})).unwrap();
}

#[test]
fn page_break_preview_opens_at_60_and_normal_comes_back_at_its_own_zoom() {
    let mut s = new_session();
    assert_eq!(zooms(&s), (100, None, None, None));
    run(&mut s, "view.pageBreakPreview");
    assert_eq!(zooms(&s), (60, Some(100), None, None));
    run(&mut s, "view.normal");
    assert_eq!(zooms(&s), (100, Some(100), Some(60), None));
}

#[test]
fn each_view_keeps_the_zoom_it_was_left_at() {
    let mut s = new_session();
    zoom(&mut s, 150);
    assert_eq!(zooms(&s), (150, None, None, None));
    run(&mut s, "view.pageBreakPreview");
    assert_eq!(zooms(&s), (60, Some(150), None, None));
    zoom(&mut s, 85);
    assert_eq!(zooms(&s), (85, Some(150), None, None));
    run(&mut s, "view.pageLayout");
    assert_eq!(zooms(&s), (100, Some(150), Some(85), None));
    run(&mut s, "view.pageBreakPreview");
    assert_eq!(zooms(&s), (85, Some(150), Some(85), Some(100)));
    run(&mut s, "view.normal");
    assert_eq!(zooms(&s), (150, Some(150), Some(85), Some(100)));
}

#[test]
fn page_layout_view_opens_at_100_whatever_normal_was_at() {
    let mut s = new_session();
    zoom(&mut s, 75);
    run(&mut s, "view.pageLayout");
    assert_eq!(zooms(&s), (100, Some(75), None, None));
    zoom(&mut s, 120);
    run(&mut s, "view.normal");
    assert_eq!(zooms(&s), (75, Some(75), None, Some(120)));
    run(&mut s, "view.pageLayout");
    assert_eq!(zooms(&s), (120, Some(75), None, Some(120)));
}

#[test]
fn choosing_the_view_a_sheet_is_already_in_changes_nothing() {
    let mut s = new_session();
    zoom(&mut s, 130);
    run(&mut s, "view.normal");
    assert_eq!(zooms(&s), (130, None, None, None));
    run(&mut s, "view.pageBreakPreview");
    zoom(&mut s, 40);
    run(&mut s, "view.pageBreakPreview");
    assert_eq!(zooms(&s), (40, Some(130), None, None));
}

#[test]
fn switching_view_does_not_mark_an_unmodified_workbook_as_modified() {
    let mut s = new_session();
    let was_dirty = s.doc().unwrap().is_dirty();
    run(&mut s, "view.pageBreakPreview");
    run(&mut s, "view.normal");
    assert_eq!(s.doc().unwrap().is_dirty(), was_dirty);
    assert_eq!(zooms(&s), (100, Some(100), Some(60), None));
}
