//! View › View Side by Side with Synchronous Scrolling: scrolling one compared window scrolls the
//! other by as many rows; with Synchronous Scrolling off it stays where it is.

use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// One workbook with two windows (View › New Window) compared side by side; the second is active.
fn harness() -> (Harness, u64, u64) {
    let mut session = Session::new();
    session.new_workbook();
    let first = session.active_window().unwrap();
    let mut h = crate::common::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        crate::common::app(session),
    );
    h.run_steps(2);
    let second = h.state_mut().run("view.newWindow", json!({})).unwrap()["window"].as_u64().unwrap();
    h.state_mut().run("view.sideBySide", json!({})).unwrap();
    h.run_steps(2);
    (h, first, second)
}

/// The first row window `window` shows (0-based).
fn top_row(h: &Harness, window: u64) -> u32 {
    let app = h.state();
    let sh = app.session.active().unwrap().wb.active().unwrap();
    let y = app.views.get(&(window, sh.uid.get())).map_or(0.0, |v| v.scroll.y);
    sh.row_at(sh.row_top(0) + f64::from(y) + 0.01)
}

fn scroll_down(h: &mut Harness, rows: u32) {
    let app = h.state_mut();
    let sh = app.session.active().unwrap().wb.active().unwrap().clone();
    let v = app.view_mut().unwrap();
    let top = sh.row_at(f64::from(v.scroll.y) + 0.01);
    v.scroll.y = sh.row_top(top + rows) as f32;
    h.run_steps(2);
}

#[test]
fn the_partner_window_scrolls_by_the_same_rows() {
    let (mut h, first, second) = harness();
    assert_eq!(h.state().session.active_window(), Some(second));
    assert_eq!((top_row(&h, first), top_row(&h, second)), (0, 0));
    scroll_down(&mut h, 10);
    assert_eq!(top_row(&h, second), 10, "setup: the active window scrolled");
    assert_eq!(top_row(&h, first), 10, "the compared window scrolled with it");
    scroll_down(&mut h, 5);
    assert_eq!(top_row(&h, first), 15);
}

#[test]
fn without_synchronous_scrolling_the_partner_stays() {
    let (mut h, first, second) = harness();
    h.state_mut().run("view.synchronousScrolling", json!({"on": false})).unwrap();
    h.run_steps(2);
    scroll_down(&mut h, 10);
    assert_eq!(top_row(&h, second), 10, "setup: the active window scrolled");
    assert_eq!(top_row(&h, first), 0);
}

#[test]
fn more_than_two_windows_ask_which_to_compare_with() {
    let (mut h, _, second) = harness();
    h.state_mut().run("view.sideBySide", json!({"on": false})).unwrap();
    h.state_mut().run("view.newWindow", json!({})).unwrap();
    h.state_mut().open_dialog("compareSideBySide", json!({}));
    h.run_steps(2);
    let d = h.state().dialog.as_ref().unwrap();
    assert_eq!(d.title, "Compare Side by Side");
    h.get_by_label("Compare Side by Side with:");
    h.get_by_label("Book1:2").click();
    h.run_steps(2);
    h.get_by(|n| n.label().is_some_and(|l| l.trim() == "OK")).click();
    h.run_steps(2);
    let pair = h.state().session.side_by_side().unwrap();
    assert_eq!(pair.windows[1], second);
}
