use super::ref_edit::{RefLabel, end_pick, ref_edit};
use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json};
use crate::editor::CallAtCaret;
use gridcraft_engine::core::Value;
use gridcraft_engine::formula::FormulaLocale;

/// The most argument boxes a variable-argument function grows to (Excel's limit for SUM).
const MAX_ARGUMENTS: usize = 255;

/// Opens Function Arguments on `call` (Ctrl+A or fx while editing), its arguments filled in.
pub(crate) fn open_on_call(app: &mut SheetApp, call: CallAtCaret) {
    app.open_dialog("functionArguments", json!({"function": call.name, "args": call.args, "span": [call.start, call.end]}));
}

/// Opens Function Arguments for the call Insert Function just started at the end of the edit
/// (`=1+SUM(`): OK replaces that call only.
pub(super) fn open_after_insert(app: &mut SheetApp, name: &str) {
    let Some(text) = app.editor.as_ref().map(|e| e.text.as_str()) else { return };
    let end = text.len();
    let head = text.strip_suffix('(').unwrap_or(text);
    let start = head.trim_end_matches(|c: char| c.is_alphanumeric() || c == '.' || c == '_').len();
    app.open_dialog("functionArguments", json!({"function": name, "span": [start, end]}));
}

/// The dialog's values from `open_dialog` params: `function`, `arg0`, `arg1`, ... and `span`.
pub(super) fn dialog(params: &Json) -> Dialog {
    let mut d = Dialog::custom("functionArguments", "Function Arguments", json!({"function": params["function"].as_str().unwrap_or("")}));
    if let Some(args) = params["args"].as_array() {
        for (i, a) in args.iter().take(MAX_ARGUMENTS).enumerate() {
            d.values.insert(format!("arg{i}"), json!(a.as_str().unwrap_or("")));
        }
    }
    if params["span"].is_array() {
        d.values.insert("span".into(), params["span"].clone());
    }
    d
}

/// The argument names of `name`'s signature in the interface language, brackets removed, and
/// the group of them that repeats (`[number2]`, or `[criteria_range2, criteria2]`) when the
/// function takes a variable number of arguments.
fn argument_names(name: &str, locale: FormulaLocale, language: crate::i18n::Language) -> (Vec<String>, usize) {
    let Some(signature) = crate::formula_locale::signature(name, locale, language) else { return (Vec::new(), 0) };
    let inner = signature.split_once('(').map_or("", |(_, rest)| rest.strip_suffix(')').unwrap_or(rest));
    if inner.trim().is_empty() {
        return (Vec::new(), 0);
    }
    let separator = format!("{} ", locale.list_separator());
    let items: Vec<&str> = inner.split(separator.as_str()).collect();
    let variadic = items.last().is_some_and(|a| a.trim() == "...");
    let fixed: Vec<&str> = items.iter().copied().filter(|a| a.trim() != "...").collect();
    // The repeating group starts at the last argument that opens a bracket.
    let group = if variadic { fixed.iter().rposition(|a| a.starts_with('[')).map_or(1, |i| fixed.len() - i) } else { 0 };
    let names = fixed.iter().map(|a| a.trim().trim_matches(['[', ']']).to_string()).collect();
    (names, group)
}

/// `number2` advanced by `by` repeats: `number3` for 1. A name without a trailing number stays.
fn advance(name: &str, by: usize) -> String {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    match name.get(stem.len()..).and_then(|digits| digits.parse::<usize>().ok()) {
        Some(n) => format!("{stem}{}", n.saturating_add(by)),
        None => name.to_string(),
    }
}

/// The value of `text` as a formula in the edited cell's sheet, as the dialog shows it; `None`
/// when it does not parse. Nothing is written to the workbook.
fn preview(app: &SheetApp, text: &str, locale: FormulaLocale) -> Option<String> {
    let doc = app.session.active()?;
    let (sheet, at) = app.editor.as_ref().map_or((doc.wb.active_sheet(), doc.selection.active), |e| (e.sheet, e.cell));
    let known = |name: &str| doc.wb.name(name, sheet).is_some() || doc.wb.table(name).is_some();
    let expr = gridcraft_engine::formula::parse_input(text.strip_prefix('=').unwrap_or(text), locale, known).ok()?;
    Some(show_value(&gridcraft_engine::calc::recalc::evaluate_expr(&doc.wb, sheet, at, &expr), locale))
}

/// A value as the dialog shows it: text quoted, an array as `{1;2;3}` (its first values only).
fn show_value(v: &Value, locale: FormulaLocale) -> String {
    match v {
        Value::Empty => "0".into(),
        Value::Text(t) => format!("\"{t}\""),
        Value::Array(a) => {
            const SHOWN: usize = 20;
            let mut out = String::from("{");
            for (i, x) in a.data.iter().take(SHOWN).enumerate() {
                if i > 0 {
                    out.push(if a.cols > 0 && i % a.cols == 0 { ';' } else { locale.array_column_separator() });
                }
                out.push_str(&show_value(x, locale));
            }
            if a.data.len() > SHOWN {
                out.push_str("...");
            }
            out.push('}');
            out
        }
        v => v.display(),
    }
}

/// The argument boxes' texts, `arg0` on.
fn arguments(d: &Dialog) -> Vec<String> {
    let count = (0..MAX_ARGUMENTS).rev().find(|i| d.values.contains_key(&format!("arg{i}"))).map_or(0, |i| i + 1);
    (0..count).map(|i| d.values.get(&format!("arg{i}")).and_then(Json::as_str).unwrap_or("").to_string()).collect()
}

/// `NAME(a1,a2,...)` in the formula language: trailing empty arguments dropped, empty ones in
/// the middle kept.
fn call_text(app: &SheetApp, name: &str, args: &[String], locale: FormulaLocale) -> String {
    let shown = app.session.active().map(|doc| crate::formula_locale::completion_name(name, locale, &doc.wb, doc.wb.active_sheet())).unwrap_or(name);
    let used = args.iter().rposition(|a| !a.trim().is_empty()).map_or(0, |i| i + 1);
    let separator = locale.list_separator().to_string();
    format!("{shown}({})", args.get(..used).unwrap_or(&[]).join(&separator))
}

/// Function Arguments: a box per argument with its value, the description and the formula's
/// result. OK puts the call into the edit (in place of the call it was opened on) and enters the
/// cell; Cancel ends the edit (see `show`).
pub(super) fn function_arguments(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let language = app.ui.language;
    let locale = app.editor.as_ref().map_or_else(|| language.formula_locale(), |e| e.locale);
    let name = d.values.get("function").and_then(Json::as_str).unwrap_or("").to_string();
    let (names, group) = argument_names(&name, locale, language);
    let mut args = arguments(d);
    let filled = args.iter().rposition(|a| !a.is_empty()).map_or(0, |i| i + 1);
    let mut boxes = if names.is_empty() { 0 } else { names.len().max(filled).min(MAX_ARGUMENTS) };
    if group > 0 {
        while boxes <= filled && boxes < MAX_ARGUMENTS {
            boxes = boxes.saturating_add(group).min(MAX_ARGUMENTS);
        }
    }
    args.resize(boxes, String::new());
    ui.label(egui::RichText::new(locale.function_name(&name)).strong());
    if names.is_empty() {
        ui.label(language.tr("This function takes no arguments."));
    } else {
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::Grid::new("functionArgumentsGrid").num_columns(3).show(ui, |ui| {
                for i in 0..boxes {
                    let label = match names.get(i) {
                        Some(n) => n.clone(),
                        None => {
                            let base = names.len().saturating_sub(group);
                            let extra = i.saturating_sub(base);
                            names.get(base + extra % group.max(1)).map_or_else(|| format!("{}", i + 1), |n| advance(n, extra / group.max(1)))
                        }
                    };
                    let l = ui.label(&label);
                    let key = format!("arg{i}");
                    let before = d.values.get(&key).cloned();
                    // A reference box: typed into, or filled by a range picked on the grid.
                    ref_edit(app, ui, d, &key, RefLabel::By(l.id), 250.0);
                    let Some(text) = args.get_mut(i) else { continue };
                    if let Some(now) = d.values.get(&key).and_then(Json::as_str) {
                        text.clear();
                        text.push_str(now);
                    }
                    if before.is_none() && text.is_empty() {
                        // A box shown but never filled is not an argument yet.
                        d.values.remove(&key);
                    }
                    let value = if text.trim().is_empty() { None } else { preview(app, text, locale) };
                    ui.label(value.map(|v| format!("= {v}")).unwrap_or_default());
                    ui.end_row();
                }
            });
        });
    }
    ui.separator();
    ui.label(crate::formula_locale::description(&name, locale, language).unwrap_or_default());
    let call = call_text(app, &name, &args, locale);
    if let Some(result) = preview(app, &call, locale) {
        ui.label(format!("{} = {result}", language.tr("Formula result")));
    }
    ok_cancel(ui, confirm, open);
    // Back to the edited cell's sheet and selection before the formula goes in.
    if *confirm && let Err(e) = end_pick(app, d) {
        app.message = Some((d.title.clone(), crate::clean_error(&e)));
        *confirm = false;
    }
    if *confirm {
        enter(app, d, &call);
    }
}

/// Cancel, Escape or closing Function Arguments cancels the cell edit, as in Excel.
pub(super) fn after(app: &mut SheetApp, d: &Dialog, open: bool, confirm: bool) {
    if !open && !confirm && d.name == "functionArguments" {
        app.cancel_edit();
    }
}

/// OK: the call replaces the one the dialog was opened on (`span`), or the whole edit, and the
/// cell is entered without moving the selection.
fn enter(app: &mut SheetApp, d: &Dialog, call: &str) {
    let span = d.values.get("span").and_then(Json::as_array).and_then(|s| {
        let at = |i: usize| s.get(i).and_then(Json::as_u64).and_then(|n| usize::try_from(n).ok());
        Some((at(0)?, at(1)?))
    });
    if app.editor.is_none() {
        app.begin_edit(Some(format!("={call}")), false);
    } else if let Some(ed) = app.editor.as_mut() {
        let text = match span {
            Some((a, b)) if a <= b && ed.text.get(..a).is_some() && ed.text.get(b..).is_some() => {
                format!("{}{call}{}", ed.text.get(..a).unwrap_or(""), ed.text.get(b..).unwrap_or(""))
            }
            _ => format!("={call}"),
        };
        ed.text = text;
    }
    app.commit_edit(0, 0, false, false);
}
