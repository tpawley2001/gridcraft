//! AutoCorrect Exceptions > INitial CAps: words the TWo INitial CApitals rule leaves as typed,
//! edited with commands and persisted with the UI settings.

use egui::{Event, Key};
use gridcraft_engine::{
    Session,
    core::{CellRef, Value},
};
use gridcraft_ui_egui::{SheetApp, UiState};
use serde_json::json;

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

#[test]
fn the_list_starts_empty_and_the_rule_applies() {
    let mut h = harness();
    assert_eq!(h.state_mut().run("options.autoCorrectExceptions", json!({})).unwrap(), json!([]));
    enter(&mut h, "IDs");
    assert_eq!(value(h.state(), "A1"), Value::text("Ids"));
}

#[test]
fn an_added_word_is_sorted_in_and_kept_as_typed() {
    let mut h = harness();
    h.state_mut().run("options.autoCorrectException.add", json!({"word": "PCs"})).unwrap();
    let list = h.state_mut().run("options.autoCorrectException.add", json!({"word": "IDs"})).unwrap();
    assert_eq!(list, json!(["IDs", "PCs"]));
    assert_eq!(h.state_mut().run("options.autoCorrectException.add", json!({"word": "IDs"})).unwrap(), json!(["IDs", "PCs"]), "no duplicate");
    enter(&mut h, "my IDs and CDs");
    assert_eq!(value(h.state(), "A1"), Value::text("my IDs and Cds"));
}

#[test]
fn a_deleted_word_is_corrected_again() {
    let mut h = harness();
    h.state_mut().run("options.autoCorrectException.add", json!({"word": "IDs"})).unwrap();
    assert_eq!(h.state_mut().run("options.autoCorrectException.delete", json!({"word": "IDs"})).unwrap(), json!([]));
    assert!(h.state_mut().run("options.autoCorrectException.delete", json!({"word": "IDs"})).is_err(), "deleting a missing word");
    enter(&mut h, "IDs");
    assert_eq!(value(h.state(), "A1"), Value::text("Ids"));
}

#[test]
fn invalid_words_are_refused() {
    let mut h = harness();
    for params in [json!({"word": ""}), json!({"word": "two words"}), json!({})] {
        assert!(h.state_mut().run("options.autoCorrectException.add", params.clone()).is_err(), "{params} was accepted");
    }
    assert_eq!(h.state_mut().run("options.autoCorrectExceptions", json!({})).unwrap(), json!([]));
}

#[test]
fn the_list_persists_and_older_settings_load_empty() {
    let mut h = harness();
    h.state_mut().run("options.autoCorrectException.add", json!({"word": "IDs"})).unwrap();
    let saved = serde_json::to_string(&h.state().ui).unwrap();
    let mut fresh = harness();
    fresh.state_mut().ui = serde_json::from_str::<UiState>(&saved).unwrap();
    assert_eq!(fresh.state_mut().run("options.autoCorrectExceptions", json!({})).unwrap(), json!(["IDs"]));
    enter(&mut fresh, "IDs");
    assert_eq!(value(fresh.state(), "A1"), Value::text("IDs"));
    for old in ["{}", r#"{"autoCorrect": {"twoInitialCaps": true}}"#] {
        let mut h = harness();
        h.state_mut().ui = serde_json::from_str::<UiState>(old).unwrap();
        assert_eq!(h.state_mut().run("options.autoCorrectExceptions", json!({})).unwrap(), json!([]), "settings {old}");
    }
}
