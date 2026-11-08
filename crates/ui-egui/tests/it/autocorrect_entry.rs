//! AutoCorrect applies to typed entries before validation, never to programmatic edits.

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
fn typed_text_is_corrected_and_the_preference_can_disable_it() {
    let mut h = harness();
    enter(&mut h, "COlumn");
    assert_eq!(value(h.state(), "A1"), Value::text("Column"));
    h.state_mut().run("options.autoCorrect", json!({"twoInitialCaps": false})).unwrap();
    enter(&mut h, "COlumn");
    assert_eq!(value(h.state(), "A2"), Value::text("COlumn"));
}

#[test]
fn formulas_and_agent_commands_are_preserved() {
    let mut h = harness();
    enter(&mut h, "=teh");
    let cell = h.state().session.active().unwrap().wb.active().unwrap().cell(CellRef::new(0, 0)).unwrap();
    assert_eq!(cell.formula.as_ref().map(|f| f.text.as_str()), Some("teh"));
    h.state_mut().run("cell.set", json!({"cell": "B1", "input": "COlumn"})).unwrap();
    assert_eq!(value(h.state(), "B1"), Value::text("COlumn"));
}

#[test]
fn options_persist_and_older_settings_enable_every_rule() {
    let mut h = harness();
    let result = h.state_mut().run("options.autoCorrect", json!({"twoInitialCaps": false, "replaceText": false})).unwrap();
    assert_eq!(result, json!({"twoInitialCaps": false, "dayNames": true, "capsLock": true, "replaceText": false}));
    let saved = serde_json::to_string(&h.state().ui).unwrap();
    let restored: UiState = serde_json::from_str(&saved).unwrap();
    assert!(!restored.auto_correct.two_initial_caps && !restored.auto_correct.replace_text);
    assert!(restored.auto_correct.day_names && restored.auto_correct.caps_lock);
    let old: UiState = serde_json::from_str("{}").unwrap();
    assert!(old.auto_correct.two_initial_caps && old.auto_correct.day_names && old.auto_correct.caps_lock && old.auto_correct.replace_text);
}

#[test]
fn validation_checks_the_corrected_entry() {
    let mut h = harness();
    h.state_mut().session.execute("data.validation", json!({"range": "A1", "type": "list", "formula1": "\"Column,Other\""})).unwrap();
    enter(&mut h, "COlumn");
    assert_eq!(value(h.state(), "A1"), Value::text("Column"));
    assert!(h.state().message.is_none());
}
