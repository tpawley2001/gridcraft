//! The AutoCorrect replacement list is editable (Excel's AutoCorrect dialog: Replace / With, Add,
//! Delete), kept sorted, persisted with the UI settings, and used for typed entries.

use egui::{Event, Key};
use gridcraft_engine::{
    Session,
    core::{CellRef, Value},
};
use gridcraft_ui_egui::{SheetApp, UiState};
use serde_json::{Value as Json, json};

fn harness() -> crate::common::Harness {
    let mut session = Session::new();
    session.new_workbook();
    crate::common::harness_with(crate::common::builder().with_size(egui::vec2(1200.0, 800.0)), crate::common::app(session), 4)
}

fn enter(h: &mut crate::common::Harness, text: &str) {
    h.input_mut().events.push(Event::Text(text.into()));
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().editor.is_none());
}

fn value(app: &SheetApp, address: &str) -> Value {
    app.session.active().unwrap().wb.active().unwrap().cell(CellRef::parse(address).unwrap()).unwrap().value.clone()
}

fn pairs(list: &[(&str, &str)]) -> Json {
    Json::Array(list.iter().map(|(k, v)| json!([k, v])).collect())
}

const BUILT_IN: [(&str, &str); 8] = [
    ("(c)", "©"),
    ("(e)", "€"),
    ("(r)", "®"),
    ("(tm)", "™"),
    ("...", "…"),
    ("adn", "and"),
    ("recieve", "receive"),
    ("teh", "the"),
];

#[test]
fn the_list_starts_with_the_built_ins_sorted() {
    let mut h = harness();
    assert_eq!(h.state_mut().run("options.autoCorrectEntries", json!({})).unwrap(), pairs(&BUILT_IN));
}

#[test]
fn an_added_entry_is_sorted_in_and_replaces_typed_text() {
    let mut h = harness();
    let list = h.state_mut().run("options.autoCorrectEntry.set", json!({"replace": "brb", "with": "be right back"})).unwrap();
    let mut want = BUILT_IN.to_vec();
    want.insert(6, ("brb", "be right back"));
    assert_eq!(list, pairs(&want));
    enter(&mut h, "brb");
    assert_eq!(value(h.state(), "A1"), Value::text("be right back"));
}

#[test]
fn setting_an_existing_entry_changes_its_replacement() {
    let mut h = harness();
    h.state_mut().run("options.autoCorrectEntry.set", json!({"replace": "teh", "with": "THE"})).unwrap();
    let list = h.state_mut().run("options.autoCorrectEntries", json!({})).unwrap();
    let mut want = BUILT_IN.to_vec();
    want[7] = ("teh", "THE");
    assert_eq!(list, pairs(&want));
    enter(&mut h, "teh");
    assert_eq!(value(h.state(), "A1"), Value::text("THE"));
}

#[test]
fn a_deleted_entry_no_longer_replaces_and_reset_restores_it() {
    let mut h = harness();
    let list = h.state_mut().run("options.autoCorrectEntry.delete", json!({"replace": "teh"})).unwrap();
    assert_eq!(list, pairs(&BUILT_IN[..7]));
    enter(&mut h, "teh");
    assert_eq!(value(h.state(), "A1"), Value::text("teh"));
    assert!(h.state_mut().run("options.autoCorrectEntry.delete", json!({"replace": "teh"})).is_err(), "deleting a missing entry");
    h.state_mut().run("options.autoCorrectEntry.set", json!({"replace": "brb", "with": "be right back"})).unwrap();
    assert_eq!(h.state_mut().run("options.autoCorrectEntries.reset", json!({})).unwrap(), pairs(&BUILT_IN));
    enter(&mut h, "teh");
    assert_eq!(value(h.state(), "A2"), Value::text("the"));
}

#[test]
fn invalid_entries_are_refused_and_leave_the_list_unchanged() {
    let mut h = harness();
    for params in [
        json!({"replace": "", "with": "x"}),
        json!({"replace": "two words", "with": "x"}),
        json!({"replace": "tab\there", "with": "x"}),
        json!({"replace": "brb", "with": ""}),
        json!({"with": "x"}),
    ] {
        assert!(h.state_mut().run("options.autoCorrectEntry.set", params.clone()).is_err(), "{params} was accepted");
    }
    assert_eq!(h.state_mut().run("options.autoCorrectEntries", json!({})).unwrap(), pairs(&BUILT_IN));
}

#[test]
fn the_list_persists_and_older_settings_get_the_built_ins() {
    let mut h = harness();
    h.state_mut().run("options.autoCorrectEntry.set", json!({"replace": "brb", "with": "be right back"})).unwrap();
    h.state_mut().run("options.autoCorrectEntry.delete", json!({"replace": "adn"})).unwrap();
    let edited = h.state_mut().run("options.autoCorrectEntries", json!({})).unwrap();
    let saved = serde_json::to_string(&h.state().ui).unwrap();

    let mut fresh = harness();
    fresh.state_mut().ui = serde_json::from_str::<UiState>(&saved).unwrap();
    assert_eq!(fresh.state_mut().run("options.autoCorrectEntries", json!({})).unwrap(), edited);
    enter(&mut fresh, "brb");
    assert_eq!(value(fresh.state(), "A1"), Value::text("be right back"));

    for old in ["{}", r#"{"autoCorrect": {"twoInitialCaps": false}}"#] {
        let mut h = harness();
        h.state_mut().ui = serde_json::from_str::<UiState>(old).unwrap();
        assert_eq!(h.state_mut().run("options.autoCorrectEntries", json!({})).unwrap(), pairs(&BUILT_IN), "settings {old}");
    }
}

#[test]
fn replace_text_off_still_disables_the_list() {
    let mut h = harness();
    h.state_mut().run("options.autoCorrectEntry.set", json!({"replace": "brb", "with": "be right back"})).unwrap();
    h.state_mut().run("options.autoCorrect", json!({"replaceText": false})).unwrap();
    enter(&mut h, "brb");
    assert_eq!(value(h.state(), "A1"), Value::text("brb"));
}
