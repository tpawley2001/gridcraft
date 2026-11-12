use gridcraft_engine::Session;
use serde_json::json;

fn freeze(s: &Session) -> Option<(u32, u32)> {
    s.doc().unwrap().wb.active().unwrap().freeze
}

#[test]
fn showing_a_custom_view_restores_its_frozen_panes() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("view.freezePanes", json!({"cell": "B2"})).unwrap();
    s.execute("view.customViews", json!({"save": "Frozen"})).unwrap();
    s.execute("view.unfreezePanes", json!({})).unwrap();
    assert_eq!(freeze(&s), None);
    s.execute("view.customViews", json!({"show": "Frozen"})).unwrap();
    assert_eq!(freeze(&s), Some((1, 1)));
}

#[test]
fn showing_an_unfrozen_custom_view_clears_current_panes() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("view.customViews", json!({"save": "Plain"})).unwrap();
    s.execute("view.freezePanes", json!({"cell": "B2"})).unwrap();
    s.execute("view.customViews", json!({"show": "Plain"})).unwrap();
    assert_eq!(freeze(&s), None);
}

fn mark_was_split(s: &mut Session) {
    let sh = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().wb).sheet_mut(0).unwrap();
    sh.freeze = Some((3, 6));
    sh.freeze_was_split = true;
}

fn was_split(s: &Session) -> bool {
    s.doc().unwrap().wb.active().unwrap().freeze_was_split
}

#[test]
fn freezing_or_unfreezing_in_the_app_is_a_plain_freeze() {
    let mut s = Session::new();
    s.new_workbook();
    mark_was_split(&mut s);
    s.execute("view.freezePanes", json!({"cell": "B2"})).unwrap();
    assert!(!was_split(&s));
    mark_was_split(&mut s);
    s.execute("view.unfreezePanes", json!({})).unwrap();
    assert!(!was_split(&s));
    mark_was_split(&mut s);
    s.execute("view.freezeTopRow", json!({})).unwrap();
    assert!(!was_split(&s));
    s.execute("view.customViews", json!({"save": "Top"})).unwrap();
    mark_was_split(&mut s);
    s.execute("view.customViews", json!({"show": "Top"})).unwrap();
    assert!(!was_split(&s));
}
