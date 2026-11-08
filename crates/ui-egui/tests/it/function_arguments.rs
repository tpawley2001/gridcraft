//! Function Arguments: picking a function in Insert Function opens a box per argument with the
//! formula's live result; OK enters the formula, Cancel leaves the cell as it was. Ctrl+A after a
//! function name (or inside a call) opens it on that call with its arguments filled in.

use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A1:A3 = 1, 2, 3 and A4 = "x"; the active cell is B1.
fn harness() -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1], [2], [3], ["x"]]})).unwrap();
    s.execute("selection.set", json!({"range": "B1"})).unwrap();
    let app = crate::common::app(s);
    crate::common::harness_with(crate::common::builder().with_os(OperatingSystem::Nix).with_size(vec2(1280.0, 800.0)), app, 4)
}

fn dialog_name(h: &Harness) -> Option<String> {
    h.state().dialog.as_ref().map(|d| d.name.clone())
}

fn value_of(h: &Harness, key: &str) -> Option<Json> {
    h.state().dialog.as_ref().and_then(|d| d.values.get(key).cloned())
}

fn editor_text(h: &Harness) -> Option<String> {
    h.state().editor.as_ref().map(|e| e.text.clone())
}

fn input(h: &Harness, cell: &str) -> String {
    let d = h.state().session.active().unwrap();
    d.wb.active().unwrap().input_text(CellRef::parse(cell).unwrap())
}

fn cell_value(h: &mut Harness, a1: &str) -> Json {
    h.state_mut().session.execute("cell.get", json!({"cell": a1})).unwrap()["value"].clone()
}

fn set_arg(h: &mut Harness, i: usize, text: &str) {
    h.state_mut().dialog.as_mut().expect("Function Arguments is open").values.insert(format!("arg{i}"), json!(text));
    h.run_steps(3);
}

/// Clicks a button by its label, ignoring the padding dialog buttons put around "OK" and "Cancel".
fn click(h: &mut Harness, label: &str) {
    h.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label().is_some_and(|l| l.trim() == label)).click();
    h.run_steps(4);
}

/// Insert Function, searched for `name` (a search that lists `name` first), then OK.
fn insert_function(h: &mut Harness, name: &str) {
    h.state_mut().open_dialog("insertFunction", json!({}));
    h.run_steps(3);
    h.state_mut().dialog.as_mut().expect("Insert Function is open").search = name.into();
    h.run_steps(3);
    click(h, "OK");
}

fn ctrl_a(h: &mut Harness) {
    let m = Modifiers::CTRL | Modifiers::COMMAND;
    h.input_mut().events.extend([
        Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: m },
        Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: m },
    ]);
    h.run_steps(4);
}

fn edit(h: &mut Harness, text: &str) {
    h.state_mut().begin_edit(Some(text.into()), false);
    h.run_steps(4);
}

#[test]
fn insert_function_ok_opens_function_arguments_while_the_cell_edits_the_call() {
    let mut h = harness();
    insert_function(&mut h, "VLOOKUP");
    assert_eq!(dialog_name(&h).as_deref(), Some("functionArguments"));
    assert_eq!(value_of(&h, "function"), Some(json!("VLOOKUP")));
    assert_eq!(editor_text(&h).as_deref(), Some("=VLOOKUP("), "the cell still edits the call, as before");
    for arg in ["lookup_value", "table_array", "col_index_num", "range_lookup"] {
        h.get_by_label(arg);
    }
}

#[test]
fn ok_enters_the_formula_and_commits_the_cell() {
    let mut h = harness();
    insert_function(&mut h, "SUMPRODUCT");
    set_arg(&mut h, 0, "A1:A3");
    set_arg(&mut h, 1, "A1:A3");
    click(&mut h, "OK");
    assert_eq!(dialog_name(&h), None, "OK closes Function Arguments");
    assert_eq!(editor_text(&h), None, "the cell is committed");
    assert_eq!(input(&h, "B1"), "=SUMPRODUCT(A1:A3,A1:A3)");
    assert_eq!(cell_value(&mut h, "B1"), json!(14.0));
}

#[test]
fn a_variable_argument_function_offers_one_more_box_than_it_has_filled() {
    let mut h = harness();
    insert_function(&mut h, "SUMPRODUCT");
    set_arg(&mut h, 0, "A1:A3");
    set_arg(&mut h, 1, "A1:A3");
    h.get_by_label("array3");
    assert!(h.query_by_label("array4").is_none(), "only one empty box after the filled ones");
}

#[test]
fn the_dialog_shows_each_argument_value_and_the_formula_result() {
    let mut h = harness();
    edit(&mut h, "=ROUND(");
    ctrl_a(&mut h);
    set_arg(&mut h, 0, "A3/7");
    set_arg(&mut h, 1, "2");
    // 3/7 = 0.428571..., ROUND(...,2) = 0.43.
    h.get_by_label("= 2");
    h.get_by_label("Formula result = 0.43");
}

#[test]
fn trailing_empty_arguments_are_dropped_and_middle_ones_kept() {
    let mut h = harness();
    insert_function(&mut h, "VLOOKUP");
    set_arg(&mut h, 0, "2");
    set_arg(&mut h, 1, "A1:A3");
    set_arg(&mut h, 2, "1");
    set_arg(&mut h, 3, "");
    click(&mut h, "OK");
    assert_eq!(input(&h, "B1"), "=VLOOKUP(2,A1:A3,1)");
    assert_eq!(cell_value(&mut h, "B1"), json!(2.0));

    h.state_mut().session.execute("selection.set", json!({"range": "C1"})).unwrap();
    h.run_steps(2);
    edit(&mut h, "=IF(");
    ctrl_a(&mut h);
    assert_eq!(value_of(&h, "function"), Some(json!("IF")));
    set_arg(&mut h, 0, "A1>1");
    set_arg(&mut h, 1, "");
    set_arg(&mut h, 2, "\"n\"");
    click(&mut h, "OK");
    assert_eq!(input(&h, "C1"), "=IF(A1>1,,\"n\")");
    assert_eq!(cell_value(&mut h, "C1"), json!("n"));
}

#[test]
fn cancel_leaves_the_cell_as_it_was() {
    let mut h = harness();
    h.state_mut().session.execute("cell.set", json!({"cell": "B1", "input": "7"})).unwrap();
    h.run_steps(2);
    insert_function(&mut h, "SUMPRODUCT");
    set_arg(&mut h, 0, "A1:A3");
    click(&mut h, "Cancel");
    assert_eq!(dialog_name(&h), None);
    assert_eq!(editor_text(&h), None, "Cancel ends the cell edit");
    assert_eq!(input(&h, "B1"), "7");
}

#[test]
fn a_function_without_arguments_says_so_and_ok_closes_the_call() {
    let mut h = harness();
    edit(&mut h, "=TODAY(");
    ctrl_a(&mut h);
    assert_eq!(value_of(&h, "function"), Some(json!("TODAY")));
    h.get_by_label("This function takes no arguments.");
    click(&mut h, "OK");
    assert_eq!(input(&h, "B1"), "=TODAY()");
}

#[test]
fn ctrl_a_after_a_function_name_opens_its_arguments() {
    let mut h = harness();
    edit(&mut h, "=VLOOKUP");
    ctrl_a(&mut h);
    assert_eq!(dialog_name(&h).as_deref(), Some("functionArguments"));
    assert_eq!(value_of(&h, "function"), Some(json!("VLOOKUP")));
}

#[test]
fn ctrl_a_inside_a_call_fills_in_its_arguments_and_ok_replaces_only_that_call() {
    let mut h = harness();
    edit(&mut h, "=1+ROUND(A3/7,2");
    ctrl_a(&mut h);
    assert_eq!(value_of(&h, "function"), Some(json!("ROUND")));
    assert_eq!(value_of(&h, "arg0"), Some(json!("A3/7")));
    assert_eq!(value_of(&h, "arg1"), Some(json!("2")));
    set_arg(&mut h, 1, "1");
    click(&mut h, "OK");
    assert_eq!(input(&h, "B1"), "=1+ROUND(A3/7,1)");
    assert_eq!(cell_value(&mut h, "B1"), json!(1.4));
}

#[test]
fn nested_calls_and_quoted_commas_are_not_split() {
    let mut h = harness();
    edit(&mut h, "=IF(A4=\"a,b\",MAX(A1,A2),");
    ctrl_a(&mut h);
    assert_eq!(value_of(&h, "function"), Some(json!("IF")));
    assert_eq!(value_of(&h, "arg0"), Some(json!("A4=\"a,b\"")));
    assert_eq!(value_of(&h, "arg1"), Some(json!("MAX(A1,A2)")));
}

#[test]
fn ctrl_a_outside_a_call_keeps_its_meaning() {
    let mut h = harness();
    edit(&mut h, "=A1+A2");
    ctrl_a(&mut h);
    assert_eq!(dialog_name(&h), None, "no function at the caret: no dialog");
    assert_eq!(editor_text(&h).as_deref(), Some("=A1+A2"));
}
