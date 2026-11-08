//! Replacement ordering, persistence and the shared dialog command path.
mod common;

use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_ui_egui::{AutoCorrectOptions, UiState, control};
use serde_json::{Value, json};

fn set_field(h: &mut common::Harness, field: &str, value: &str) {
    let ctx = h.ctx.clone();
    control::handle(h.state_mut(), &ctx, "ui.dialog.set", &json!({"field": field, "value": value}));
    h.run_steps(3);
}

#[test]
fn dialog_add_replace_delete_and_errors_use_the_commands() {
    let mut session = Session::new();
    session.new_workbook();
    let mut h = common::os_harness(session, egui::os::OperatingSystem::Nix);
    h.state_mut().run("ui.dialog", json!({"name": "autoCorrect"})).unwrap();
    h.run_steps(3);
    h.get_by_label("Add").click();
    h.run_steps(3);
    assert!(h.state().dialog.as_ref().unwrap().result.as_ref().unwrap().is_string());
    assert!(h.state().message.is_none());

    set_field(&mut h, "replace", "brb");
    set_field(&mut h, "with", "be right back");
    h.get_by_label("Add").click();
    h.run_steps(3);
    assert!(h.state().ui.auto_correct.entries.contains(&("brb".into(), "be right back".into())));
    set_field(&mut h, "with", "back soon");
    h.get_by_label("Replace").click();
    h.run_steps(3);
    assert!(h.state().ui.auto_correct.entries.contains(&("brb".into(), "back soon".into())));
    h.get_by_role_and_label(egui::accesskit::Role::Window, "AutoCorrect").get_by_label("Delete").click();
    h.run_steps(3);
    assert!(!h.state().ui.auto_correct.entries.iter().any(|(key, _)| key == "brb"));
    h.get_by_label("Replace text as you type").click();
    h.run_steps(3);
    assert!(!h.state().ui.auto_correct.replace_text);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
}

#[test]
fn loaded_entries_sort_case_insensitively_with_exact_ties_and_keep_empty_lists() {
    let ui: UiState = serde_json::from_value(json!({"autoCorrect": {
        "entries": [["z", "last"], ["brb", " lower "], ["BRB", "upper"], ["a", "first"]]
    }}))
    .unwrap();
    assert_eq!(
        serde_json::to_value(ui.auto_correct).unwrap()["entries"],
        json!([["a", "first"], ["BRB", "upper"], ["brb", " lower "], ["z", "last"]])
    );
    let empty: AutoCorrectOptions = serde_json::from_value(json!({"entries": []})).unwrap();
    assert!(empty.entries.is_empty());
    let roundtrip: AutoCorrectOptions = serde_json::from_value(serde_json::to_value(empty).unwrap()).unwrap();
    assert!(roundtrip.entries.is_empty());
    let old: AutoCorrectOptions = serde_json::from_value(json!({})).unwrap();
    assert_eq!(old.entries.len(), 8);
    assert_eq!(Value::from(old.entries.first().unwrap().0.clone()), json!("(c)"));
}
