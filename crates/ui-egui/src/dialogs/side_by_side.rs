use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json};

/// View › View Side by Side with more than two visible windows: the windows other than the active
/// one, as `[{window, title}]` for the Compare Side by Side list.
pub(super) fn compare_choices(app: &SheetApp) -> Vec<Json> {
    let active = app.session.active_window();
    app.session
        .windows()
        .iter()
        .filter(|w| !w.hidden && Some(w.id) != active)
        .map(|w| json!({"window": w.id, "title": app.session.window_title(w.id).unwrap_or_default()}))
        .collect()
}

/// Compare Side by Side: pick the window to compare the active one with; OK runs
/// `view.sideBySide` with it as `with`.
pub(super) fn compare_side_by_side(ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    ui.label(lang.tr("Compare Side by Side with:"));
    let choices = d.result.as_ref().and_then(Json::as_array).cloned().unwrap_or_default();
    let chosen = d.values.get("with").and_then(Json::as_u64);
    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        for c in &choices {
            let Some(id) = c.get("window").and_then(Json::as_u64) else { continue };
            let title = c.get("title").and_then(Json::as_str).unwrap_or_default();
            let r = ui.add(egui::Button::selectable(chosen == Some(id), title).min_size(egui::vec2(320.0, 22.0)));
            if r.clicked() {
                d.values.insert("with".into(), json!(id));
            }
            if r.double_clicked() {
                *confirm = true;
            }
        }
    });
    ok_cancel(ui, confirm, open);
}
