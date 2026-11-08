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
    let mut selected_replace = d.values.get("selectedReplace").and_then(Json::as_str).unwrap_or_default().to_string();
    let mut changed = false;
    egui::Grid::new("auto_correct_fields").num_columns(2).min_col_width(210.0).show(ui, |ui| {
        let replace_label = ui.label(lang.tr("Replace:"));
        let with_label = ui.label(lang.tr("With:"));
        ui.end_row();
        changed = ui.add(egui::TextEdit::singleline(&mut replace).desired_width(210.0)).labelled_by(replace_label.id).changed();
        ui.add(egui::TextEdit::singleline(&mut with).desired_width(210.0)).labelled_by(with_label.id);
        ui.end_row();
    });
    let prefix = replace.to_lowercase();
    let entries = &app.ui.auto_correct.entries;
    let first = entries.iter().position(|(key, _)| !prefix.is_empty() && key.to_lowercase().starts_with(&prefix));
    // Agent edits through ui.dialog.set also move the list to the new prefix.
    let scroll = changed || d.search != replace;
    if scroll {
        selected_replace.clear();
    }
    let selected_index = entries.iter().position(|(key, _)| key == &selected_replace).or(first);
    egui::ScrollArea::vertical().id_salt("auto_correct_entries").max_height(220.0).show(ui, |ui| {
        egui::Grid::new("auto_correct_list").num_columns(2).min_col_width(210.0).striped(true).show(ui, |ui| {
            for (index, (key, value)) in entries.iter().enumerate() {
                let selected = Some(index) == selected_index;
                let key_response = ui.selectable_label(selected, key);
                let value_response = ui.selectable_label(selected, value);
                if selected && scroll {
                    key_response.scroll_to_me(Some(egui::Align::Center));
                }
                if key_response.clicked() || value_response.clicked() {
                    selected_replace.clone_from(key);
                    replace.clone_from(key);
                    with.clone_from(value);
                }
                ui.end_row();
            }
        });
    });
    let exact = app.ui.auto_correct.entries.iter().any(|(key, _)| key == &replace);
    ui.horizontal(|ui| {
        if ui.button(lang.tr(if exact { "Replace" } else { "Add" })).clicked() {
            run(app, d, "options.autoCorrectEntry.set", json!({"replace": replace, "with": with}));
        }
        if ui.add_enabled(exact, egui::Button::new(lang.tr("Delete"))).clicked() {
            run(app, d, "options.autoCorrectEntry.delete", json!({"replace": replace}));
        }
        if ui.button(lang.tr("Exceptions...")).clicked() {
            run(app, d, "ui.dialog", json!({"name": "autoCorrectExceptions"}));
        }
        if ui.button(lang.tr("OK")).clicked() {
            *open = false;
        }
    });
    if let Some(error) = d.result.as_ref().and_then(Json::as_str) {
        ui.label(error);
    }
    d.search.clone_from(&replace);
    d.values.insert("selectedReplace".into(), json!(selected_replace));
    d.values.insert("replace".into(), json!(replace));
    d.values.insert("with".into(), json!(with));
}

/// Exceptions > INitial CAps: words the TWo INitial CApitals rule leaves as typed.
pub(super) fn exceptions(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let lang = app.ui.language;
    ui.strong(lang.tr("INitial CAps"));
    let mut word = d.values.get("word").and_then(Json::as_str).unwrap_or_default().to_string();
    let label = ui.label(lang.tr("Don't correct:"));
    ui.add(egui::TextEdit::singleline(&mut word).desired_width(240.0)).labelled_by(label.id);
    egui::ScrollArea::vertical().id_salt("auto_correct_exceptions").max_height(160.0).show(ui, |ui| {
        for listed in &app.ui.auto_correct.two_caps_exceptions {
            if ui.selectable_label(listed == &word, listed).clicked() {
                word.clone_from(listed);
            }
        }
    });
    let exact = app.ui.auto_correct.two_caps_exceptions.iter().any(|listed| listed == &word);
    ui.horizontal(|ui| {
        if ui.add_enabled(!exact, egui::Button::new(lang.tr("Add"))).clicked() {
            run(app, d, "options.autoCorrectException.add", json!({"word": word}));
        }
        if ui.add_enabled(exact, egui::Button::new(lang.tr("Delete"))).clicked() {
            run(app, d, "options.autoCorrectException.delete", json!({"word": word}));
        }
        let ok = ui.button(lang.tr("OK")).clicked();
        if ui.button(lang.tr("Close")).clicked() || ok {
            *open = false;
        }
    });
    if let Some(error) = d.result.as_ref().and_then(Json::as_str) {
        ui.label(error);
    }
    d.values.insert("word".into(), json!(word));
}
