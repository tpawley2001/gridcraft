//! Reapply the last UI paste using the existing paste-special commands.
use super::{Drag, FillTag, SheetApp, geo::Geo};
use egui::{Rect, Sense, Stroke, pos2, vec2};
use gridcraft_engine::{Clipboard, model::Sheet};
use serde_json::{Value as Json, json};

/// The clipboard snapshot and destination belong to this paste, even if Copy is used again.
#[derive(Clone, Debug)]
pub struct PasteTag {
    state: FillTag,
    clipboard: Clipboard,
    params: Json,
}

impl PasteTag {
    pub(crate) fn current(&self, app: &SheetApp) -> bool {
        app.session.active().is_some_and(|d| {
            d.uid == self.state.uid
                && d.wb.active_sheet() == self.state.sheet
                && d.revision == self.state.revision
                && d.undo.len() == self.state.undo_len
                && d.selection.ranges.as_slice() == [self.state.target]
        })
    }
}

pub(crate) fn record(app: &mut SheetApp, clipboard: Clipboard, params: Json) {
    app.grid.paste_tag = app.session.active().map(|d| PasteTag {
        state: FillTag {
            src: clipboard.range,
            target: d.selection.current(),
            uid: d.uid,
            sheet: d.wb.active_sheet(),
            revision: d.revision,
            undo_len: d.undo.len(),
        },
        clipboard,
        params,
    });
}

struct Choice {
    label: &'static str,
    key: egui::Key,
    what: &'static str,
    extra: Option<&'static str>,
    transpose: bool,
    link: bool,
}

impl Choice {
    const fn new(label: &'static str, key: egui::Key, what: &'static str) -> Self {
        Self { label, key, what, extra: None, transpose: false, link: false }
    }
}

const CHOICES: [Choice; 12] = [
    Choice::new("Paste (P)", egui::Key::P, "all"),
    Choice::new("Formulas (F)", egui::Key::F, "formulas"),
    Choice::new("Formulas & Number Formatting (O)", egui::Key::O, "formulasAndNumberFormats"),
    Choice::new("Keep Source Formatting (K)", egui::Key::K, "all"),
    Choice::new("No Borders (B)", egui::Key::B, "allExceptBorders"),
    Choice { extra: Some("columnWidths"), ..Choice::new("Keep Source Column Widths (W)", egui::Key::W, "all") },
    Choice { transpose: true, ..Choice::new("Transpose (T)", egui::Key::T, "all") },
    Choice::new("Values (V)", egui::Key::V, "values"),
    Choice::new("Values & Number Formatting (A)", egui::Key::A, "valuesAndNumberFormats"),
    Choice { extra: Some("formats"), ..Choice::new("Values & Source Formatting (E)", egui::Key::E, "values") },
    Choice::new("Formatting (R)", egui::Key::R, "formats"),
    Choice { link: true, ..Choice::new("Paste Link (N)", egui::Key::N, "all") },
];

pub(super) fn show(app: &mut SheetApp, ui: &mut egui::Ui, geo: &Geo, sh: &Sheet) {
    let Some(tag) = app.grid.paste_tag.as_ref() else { return };
    if !tag.current(app) {
        app.grid.paste_tag = None;
        return;
    }
    if app.editor.is_some() || app.dialog.is_some() || app.grid.drag != Drag::None {
        return;
    }
    let corner = geo.range_rect(sh, tag.state.target).right_bottom();
    let button = geo.mirror_rect(Rect::from_min_size(corner + vec2(2.0, 2.0), vec2(22.0, 20.0)));
    if !geo.mirror_rect(geo.cells).contains_rect(button) {
        return;
    }
    let resp = ui.interact(button, egui::Id::unique("gridcraft.paste_options"), Sense::click()).on_hover_text("Paste Options");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Paste Options"));
    let t = crate::theme::Tokens::get(ui.ctx());
    let painter = ui.painter();
    painter.rect_filled(button, 3.0, if resp.hovered() { t.hover } else { t.window });
    painter.rect_stroke(button, 3.0, Stroke::new(1.0, t.separator), egui::StrokeKind::Inside);
    let c = button.center();
    // An original clipboard outline and a small menu arrow, drawn without image assets.
    painter.rect_stroke(Rect::from_min_size(pos2(c.x - 7.0, c.y - 6.0), vec2(9.0, 12.0)), 1.0, Stroke::new(1.0, t.text), egui::StrokeKind::Inside);
    painter.line_segment([pos2(c.x - 5.0, c.y - 7.0), pos2(c.x, c.y - 7.0)], Stroke::new(2.0, t.text));
    painter.add(egui::Shape::convex_polygon(vec![pos2(c.x + 4.0, c.y), pos2(c.x + 9.0, c.y), pos2(c.x + 6.5, c.y + 3.0)], t.text, Stroke::NONE));
    let mut chosen = None;
    egui::Popup::menu(&resp).show(|ui| {
        for choice in &CHOICES {
            match choice.key {
                egui::Key::P => {
                    ui.label("Paste");
                }
                egui::Key::V => {
                    ui.separator();
                    ui.label("Paste Values");
                }
                egui::Key::R => {
                    ui.separator();
                    ui.label("Other Paste Options");
                }
                _ => {}
            }
            let key = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, choice.key));
            if ui.add(egui::Button::new(choice.label).frame(false)).clicked() || key {
                chosen = Some(choice);
                ui.close();
            }
        }
    });
    if let Some(choice) = chosen
        && let Some(tag) = app.grid.paste_tag.take()
    {
        reapply(app, tag, choice);
    }
}

fn reapply(app: &mut SheetApp, tag: PasteTag, choice: &Choice) {
    app.grid.paste_tag = None;
    if let Err(e) = app.run("edit.undo", json!({})) {
        app.message = Some((crate::brand::APP_NAME.into(), crate::clean_error(&e)));
        return;
    }
    let kept_clipboard = app.session.clipboard.replace(tag.clipboard.clone());
    let result = app.session.grouped("Paste", |session| {
        let mut params = json!({"what": choice.what, "transpose": choice.transpose, "link": choice.link});
        if let Some(at) = tag.params.get("at") {
            params["at"] = at.clone();
        }
        session.execute("edit.pasteSpecial", params.clone())?;
        if let Some(what) = choice.extra {
            params["what"] = json!(what);
            session.execute("edit.pasteSpecial", params)?;
        }
        Ok(())
    });
    app.session.clipboard = kept_clipboard;
    if let Err(e) = result {
        // A failed replacement leaves the original paste available, rather than losing it.
        if app.run("edit.redo", json!({})).is_ok() {
            record(app, tag.clipboard, tag.params);
        }
        app.message = Some((crate::brand::APP_NAME.into(), crate::clean_error(&e.to_string())));
    } else {
        app.after_engine();
        record(app, tag.clipboard, tag.params);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::{Session, core::CellRef};

    fn app(at: &str) -> SheetApp {
        let mut s = Session::new();
        s.new_workbook();
        for (id, params) in [
            ("cell.set", json!({"cell": "A1", "input": "2"})),
            ("cell.set", json!({"cell": "A2", "input": "=A1*3"})),
            ("home.bold", json!({"range": "A1:A2", "on": true})),
            ("edit.copy", json!({"range": "A1:A2"})),
            ("selection.set", json!({"range": at})),
        ] {
            s.execute(id, params).unwrap();
        }
        let mut app = SheetApp::new(s, Default::default());
        app.paste_from_clipboard();
        app
    }

    #[test]
    fn combined_values_and_formats_tiles_and_undo_restores_every_cell() {
        let mut app = app("C1:D4");
        let tag = app.grid.paste_tag.clone().unwrap();
        let before = tag.state.undo_len;
        reapply(&mut app, tag, CHOICES.iter().find(|c| c.key == egui::Key::E).unwrap());
        let d = app.session.active().unwrap();
        assert_eq!(d.undo.len(), before);
        let sh = d.wb.active().unwrap();
        for cell in ["C2", "D2", "C4", "D4"] {
            let c = CellRef::parse(cell).unwrap();
            assert_eq!(sh.input_text(c), "6");
            assert!(d.wb.styles.get(sh.style_id(c)).font.bold);
        }
        app.run("edit.undo", json!({})).unwrap();
        let d = app.session.active().unwrap();
        for cell in ["C1", "C2", "D3", "D4"] {
            assert_eq!(d.wb.active().unwrap().input_text(CellRef::parse(cell).unwrap()), "");
        }
    }

    #[test]
    fn transpose_uses_existing_paste_special_formula_adjustment() {
        let mut app = app("C1");
        let tag = app.grid.paste_tag.clone().unwrap();
        let mut expected = Session::new();
        expected.new_workbook();
        expected.clipboard = Some(tag.clipboard.clone());
        expected.execute("selection.set", json!({"range": "C1"})).unwrap();
        expected.execute("edit.pasteSpecial", json!({"what": "all", "transpose": true})).unwrap();
        reapply(&mut app, tag, CHOICES.iter().find(|c| c.transpose).unwrap());
        let formula = CellRef::parse("D1").unwrap();
        let actual = app.session.active().unwrap().wb.active().unwrap().input_text(formula);
        let want = expected.active().unwrap().wb.active().unwrap().input_text(formula);
        assert_eq!(actual, want);
    }

    #[test]
    fn refused_replacement_restores_original_paste() {
        let mut app = app("XFD1");
        let tag = app.grid.paste_tag.clone().unwrap();
        let before = tag.state.undo_len;
        reapply(&mut app, tag, CHOICES.iter().find(|c| c.transpose).unwrap());
        let d = app.session.active().unwrap();
        assert_eq!(d.undo.len(), before);
        assert_eq!(d.wb.active().unwrap().input_text(CellRef::parse("XFD1").unwrap()), "2");
        assert!(app.message.is_some());
        assert!(app.grid.paste_tag.is_some());
    }

    #[test]
    fn matching_system_clipboard_text_records_an_internal_paste() {
        let mut app = app("C1");
        let text = app.session.clipboard.as_ref().unwrap().text.clone();
        app.run("edit.undo", json!({})).unwrap();
        assert!(app.grid.paste_tag.is_none());
        app.paste_external(text);
        assert!(app.grid.paste_tag.is_some());
    }

    #[test]
    fn external_paste_and_sheet_switch_dismiss_the_tag() {
        let mut app = app("C1");
        app.run_or_alert("edit.paste", json!({"text": "external"}));
        assert!(app.grid.paste_tag.is_none());
        app.paste_from_clipboard();
        assert!(app.grid.paste_tag.is_some());
        app.run("home.insertSheet", json!({})).unwrap();
        assert!(app.grid.paste_tag.is_none());
    }
}
