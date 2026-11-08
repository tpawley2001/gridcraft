use super::{Dialog, Json, SheetApp, json};

/// Replacement edits use the same commands as the control channel; refusals stay in the dialog.
fn run(app: &mut SheetApp, d: &mut Dialog, command: &str, params: Json) {
    d.result = app.run(command, params).err().map(Json::String);
}

pub(super) fn auto_correct(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let lang = app.ui.language;
    let mut enabled = app.ui.auto_correct.replace_text;
    if ui.checkbox(&mut enabled, lang.tr("Replace text as you type")).changed() {
        run(app, d, "options.autoCorrect", json!({"replaceText": enabled}));
    }
    let mut replace = d.values.get("replace").and_then(Json::as_str).unwrap_or_default().to_string();
    let mut with = d.values.get("with").and_then(Json::as_str).unwrap_or_default().to_string();
    let mut changed = false;
    egui::Grid::new("auto_correct_fields").num_columns(2).show(ui, |ui| {
        let replace_label = ui.label(lang.tr("Replace:"));
        let with_label = ui.label(lang.tr("With:"));
        ui.end_row();
        changed = ui.text_edit_singleline(&mut replace).labelled_by(replace_label.id).changed();
        ui.text_edit_singleline(&mut with).labelled_by(with_label.id);
        ui.end_row();
    });
    let prefix = replace.to_lowercase();
    let entries = &app.ui.auto_correct.entries;
    let first = entries.iter().position(|(key, _)| !prefix.is_empty() && key.to_lowercase().starts_with(&prefix));
    let exact = entries.iter().any(|(key, _)| key == &replace);
    // Agent edits through ui.dialog.set also move the list to the new prefix.
    let scroll = changed || d.search != replace;
    egui::ScrollArea::vertical().id_salt("auto_correct_entries").max_height(220.0).show(ui, |ui| {
        egui::Grid::new("auto_correct_list").num_columns(2).striped(true).show(ui, |ui| {
            for (index, (key, value)) in entries.iter().enumerate() {
                let selected = Some(index) == first;
                let key_response = ui.selectable_label(selected, key);
                let value_response = ui.selectable_label(selected, value);
                if selected && scroll {
                    key_response.scroll_to_me(Some(egui::Align::Center));
                }
                if key_response.clicked() || value_response.clicked() {
                    replace.clone_from(key);
                    with.clone_from(value);
                }
                ui.end_row();
            }
        });
    });
    ui.horizontal(|ui| {
        if ui.button(lang.tr(if exact { "Replace" } else { "Add" })).clicked() {
            run(app, d, "options.autoCorrectEntry.set", json!({"replace": replace, "with": with}));
        }
        if ui.add_enabled(exact, egui::Button::new(lang.tr("Delete"))).clicked() {
            run(app, d, "options.autoCorrectEntry.delete", json!({"replace": replace}));
        }
        if ui.button(lang.tr("OK")).clicked() {
            *open = false;
        }
    });
    if let Some(error) = d.result.as_ref().and_then(Json::as_str) {
        ui.label(error);
    }
    d.search.clone_from(&replace);
    d.values.insert("replace".into(), json!(replace));
    d.values.insert("with".into(), json!(with));
}
