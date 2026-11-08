//! View › Window › View Side by Side, Synchronous Scrolling and Reset Window Position, as in Excel:
//! two windows are compared one above the other, scrolling together until that is turned off.

use gridcraft_engine::{Session, SideBySide};
use serde_json::{Value as Json, json};

/// A session with one workbook and `n` windows on it (View › New Window); the last one is active.
fn session_with_windows(n: usize) -> (Session, Vec<u64>) {
    let mut s = Session::new();
    s.new_workbook();
    let mut ids = vec![s.active_window().unwrap()];
    for _ in 1..n {
        let r = s.execute("view.newWindow", json!({})).unwrap();
        ids.push(r["window"].as_u64().unwrap());
    }
    assert_eq!(s.active_window(), ids.last().copied());
    (s, ids)
}

fn err(s: &mut Session, id: &str, p: Json) -> String {
    s.execute(id, p).expect_err(id).to_string()
}

#[test]
fn one_window_cannot_be_compared() {
    let (mut s, _) = session_with_windows(1);
    err(&mut s, "view.sideBySide", json!({}));
    assert_eq!(s.side_by_side(), None);
}

#[test]
fn two_windows_go_side_by_side_scrolling_together() {
    let (mut s, w) = session_with_windows(2);
    let r = s.execute("view.sideBySide", json!({})).unwrap();
    assert_eq!(r, json!({"on": true, "windows": [w[1], w[0]], "synchronousScrolling": true, "arrange": "horizontal"}));
    assert_eq!(s.side_by_side(), Some(SideBySide { windows: [w[1], w[0]], synchronous: true }));
    assert_eq!(s.scroll_partner(w[0]), Some(w[1]));
    assert_eq!(s.scroll_partner(w[1]), Some(w[0]));
}

#[test]
fn the_button_toggles_and_on_sets() {
    let (mut s, _) = session_with_windows(2);
    s.execute("view.sideBySide", json!({})).unwrap();
    assert_eq!(s.execute("view.sideBySide", json!({})).unwrap(), json!({"on": false}));
    assert_eq!(s.side_by_side(), None);
    s.execute("view.sideBySide", json!({"on": true})).unwrap();
    s.execute("view.sideBySide", json!({"on": true})).unwrap();
    assert!(s.side_by_side().is_some(), "on: true keeps it on");
    s.execute("view.sideBySide", json!({"on": false})).unwrap();
    assert_eq!(s.side_by_side(), None);
}

#[test]
fn three_windows_need_the_compare_side_by_side_choice() {
    let (mut s, w) = session_with_windows(3);
    let e = err(&mut s, "view.sideBySide", json!({}));
    assert!(e.contains("Compare Side by Side"), "{e}");
    assert_eq!(s.side_by_side(), None);
    err(&mut s, "view.sideBySide", json!({"with": w[2]})); // the active window itself
    err(&mut s, "view.sideBySide", json!({"with": 9999}));
    let r = s.execute("view.sideBySide", json!({"with": w[0]})).unwrap();
    assert_eq!(r["windows"], json!([w[2], w[0]]));
    assert_eq!(s.scroll_partner(w[1]), None, "the third window is not in the pair");
}

#[test]
fn hidden_windows_do_not_count() {
    let (mut s, w) = session_with_windows(3);
    s.hide_window(w[0]);
    let r = s.execute("view.sideBySide", json!({})).unwrap();
    assert_eq!(r["windows"], json!([w[2], w[1]]));
    s.execute("view.sideBySide", json!({"on": false})).unwrap();
    err(&mut s, "view.sideBySide", json!({"with": w[0]}));
}

#[test]
fn synchronous_scrolling_toggles_only_while_side_by_side() {
    let (mut s, w) = session_with_windows(2);
    err(&mut s, "view.synchronousScrolling", json!({}));
    s.execute("view.sideBySide", json!({})).unwrap();
    assert_eq!(s.execute("view.synchronousScrolling", json!({})).unwrap(), json!({"on": false}));
    assert_eq!(s.side_by_side().map(|p| p.synchronous), Some(false));
    assert_eq!(s.scroll_partner(w[0]), None);
    assert_eq!(s.execute("view.synchronousScrolling", json!({"on": true})).unwrap(), json!({"on": true}));
    assert_eq!(s.scroll_partner(w[0]), Some(w[1]));
    // Turning side by side off and on again starts with synchronous scrolling on.
    s.execute("view.synchronousScrolling", json!({"on": false})).unwrap();
    s.execute("view.sideBySide", json!({})).unwrap();
    s.execute("view.sideBySide", json!({})).unwrap();
    assert_eq!(s.side_by_side().map(|p| p.synchronous), Some(true));
}

#[test]
fn reset_window_position_lays_the_pair_out_again() {
    let (mut s, w) = session_with_windows(2);
    err(&mut s, "view.resetWindowPosition", json!({}));
    s.execute("view.sideBySide", json!({})).unwrap();
    assert_eq!(s.execute("view.resetWindowPosition", json!({})).unwrap(), json!({"arrange": "horizontal", "windows": [w[1], w[0]]}));
    s.activate_window(w[0]);
    assert_eq!(s.execute("view.resetWindowPosition", json!({})).unwrap(), json!({"arrange": "horizontal", "windows": [w[0], w[1]]}));
}

#[test]
fn closing_or_hiding_a_compared_window_ends_side_by_side() {
    let (mut s, w) = session_with_windows(3);
    s.execute("view.sideBySide", json!({"with": w[1]})).unwrap();
    s.close_window(w[1]);
    assert_eq!(s.side_by_side(), None);
    s.execute("view.sideBySide", json!({})).unwrap();
    s.hide_window(w[0]);
    assert_eq!(s.side_by_side(), None);
}

#[test]
fn other_work_keeps_side_by_side_on() {
    let (mut s, w) = session_with_windows(2);
    s.execute("view.sideBySide", json!({})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "5"})).unwrap();
    s.execute("home.insertSheet", json!({})).unwrap();
    s.activate_window(w[0]);
    s.execute("view.arrangeAll", json!({})).unwrap();
    assert!(s.side_by_side().is_some());
}
