//! AutoCorrect › Exceptions... edits the INitial CAps list through the shared commands.
mod common;

use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_ui_egui::control;
use serde_json::json;

#[test]
fn exceptions_dialog_adds_and_deletes_words_and_returns_to_auto_correct() {
    let mut session = Session::new();
    session.new_workbook();
    let mut h = common::os_harness(session, egui::os::OperatingSystem::Nix);
    h.state_mut().run("ui.dialog", json!({"name": "autoCorrect"})).unwrap();
    h.run_steps(3);
    h.get_by_label("Exceptions...").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().unwrap().name(), "autoCorrectExceptions");

    let ctx = h.ctx.clone();
    control::handle(h.state_mut(), &ctx, "ui.dialog.set", &json!({"field": "word", "value": "IDs"}));
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Window, "AutoCorrect Exceptions").get_by_label("Add").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.auto_correct.two_caps_exceptions, vec!["IDs".to_string()]);
    h.get_by_role_and_label(egui::accesskit::Role::Window, "AutoCorrect Exceptions").get_by_label("Delete").click();
    h.run_steps(3);
    assert!(h.state().ui.auto_correct.two_caps_exceptions.is_empty());

    h.get_by_label("Close").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().unwrap().name(), "autoCorrect");
}
