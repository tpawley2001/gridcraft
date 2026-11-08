//! Function Arguments: each argument box is a reference box (Excel's RefEdit). Collapsing it and
//! dragging a range on the grid fills the box with a relative reference, which OK enters.

use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::{SheetApp, grid::Geo};
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A1:A3 = 1, 2, 3; the active cell is C1.
fn harness() -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1], [2], [3]]})).unwrap();
    s.execute("selection.set", json!({"range": "C1"})).unwrap();
    let app = crate::common::app(s);
    crate::common::harness_with(crate::common::builder().with_os(OperatingSystem::Nix).with_size(vec2(1280.0, 800.0)), app, 4)
}

fn dialog_name(h: &Harness) -> Option<String> {
    h.state().dialog.as_ref().map(|d| d.name.clone())
}

fn value_of(h: &Harness, key: &str) -> Option<Json> {
    h.state().dialog.as_ref().and_then(|d| d.values.get(key).cloned())
}

fn collapsed(h: &Harness) -> Option<String> {
    h.state().dialog.as_ref().and_then(|d| d.collapsed().map(str::to_string))
}

fn input(h: &Harness, cell: &str) -> String {
    let d = h.state().session.active().unwrap();
    d.wb.active().unwrap().input_text(CellRef::parse(cell).unwrap())
}

fn click(h: &mut Harness, label: &str) {
    h.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label().is_some_and(|l| l.trim() == label)).click();
    h.run_steps(4);
}

fn insert_function(h: &mut Harness, name: &str) {
    h.state_mut().open_dialog("insertFunction", json!({}));
    h.run_steps(3);
    h.state_mut().dialog.as_mut().expect("Insert Function is open").search = name.into();
    h.run_steps(3);
    click(h, "OK");
    assert_eq!(dialog_name(h).as_deref(), Some("functionArguments"));
}

fn cell_pos(h: &Harness, cell: &str) -> egui::Pos2 {
    let app = h.state();
    let sheet = app.session.active().unwrap().wb.active().unwrap();
    let geo = Geo::new(sheet, app.grid.rect.unwrap(), app.view().scroll);
    geo.cell_rect(sheet, CellRef::parse(cell).unwrap()).center()
}

fn press(h: &mut Harness, pos: egui::Pos2, pressed: bool) {
    h.input_mut().events.push(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
}

fn click_at(h: &mut Harness, pos: egui::Pos2) {
    h.input_mut().events.push(Event::PointerMoved(pos));
    press(h, pos, true);
    press(h, pos, false);
    h.run_steps(3);
}

fn drag(h: &mut Harness, from: &str, to: &str) {
    let (a, b) = (cell_pos(h, from), cell_pos(h, to));
    h.input_mut().events.push(Event::PointerMoved(a));
    press(h, a, true);
    h.run_steps(2);
    h.input_mut().events.push(Event::PointerMoved((a + b.to_vec2()) / 2.0));
    h.run_steps(2);
    h.input_mut().events.push(Event::PointerMoved(b));
    h.run_steps(2);
    press(h, b, false);
    h.run_steps(3);
}

/// Clicks the `n`th button labelled `label` (top to bottom).
fn click_nth(h: &mut Harness, label: &str, n: usize) {
    let rect = h.get_all_by_label(label).nth(n).map(|b| b.rect()).expect("button present");
    click_at(h, rect.center());
}

fn key(h: &mut Harness, k: Key) {
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

#[test]
fn every_argument_box_has_a_collapse_button() {
    let mut h = harness();
    insert_function(&mut h, "ROUND");
    assert_eq!(h.get_all_by_label("Collapse dialog").count(), 2, "ROUND has Number and Num_digits boxes");
}

#[test]
fn collapsing_an_argument_box_names_that_box() {
    let mut h = harness();
    insert_function(&mut h, "ROUND");
    click_nth(&mut h, "Collapse dialog", 0);
    assert_eq!(collapsed(&h).as_deref(), Some("arg0"));
    assert_eq!(h.get_all_by_label("Expand dialog").count(), 1);
    click_nth(&mut h, "Expand dialog", 0);
    assert_eq!(collapsed(&h), None);
    assert_eq!(dialog_name(&h).as_deref(), Some("functionArguments"));
}

#[test]
fn a_dragged_range_fills_the_box_as_a_relative_reference() {
    let mut h = harness();
    insert_function(&mut h, "SUM");
    click_nth(&mut h, "Collapse dialog", 0);
    drag(&mut h, "A1", "A3");
    assert_eq!(value_of(&h, "arg0"), Some(json!("A1:A3")));
}

#[test]
fn esc_while_collapsed_expands_and_keeps_the_dialog() {
    let mut h = harness();
    insert_function(&mut h, "SUM");
    click_nth(&mut h, "Collapse dialog", 0);
    key(&mut h, Key::Escape);
    assert_eq!(collapsed(&h), None);
    assert_eq!(dialog_name(&h).as_deref(), Some("functionArguments"));
}

#[test]
fn ok_after_a_pick_enters_the_formula_in_the_cell_being_edited() {
    let mut h = harness();
    insert_function(&mut h, "SUM");
    click_nth(&mut h, "Collapse dialog", 0);
    drag(&mut h, "A1", "A3");
    click_nth(&mut h, "Expand dialog", 0);
    click(&mut h, "OK");
    assert_eq!(dialog_name(&h), None);
    assert_eq!(input(&h, "C1"), "=SUM(A1:A3)");
    assert_eq!(input(&h, "A1"), "1", "the picked cells are untouched");
}

#[test]
fn goal_seek_still_picks_absolute_references() {
    let mut h = harness();
    h.state_mut().session.execute("cell.set", json!({"cell": "B1", "input": "=A1*2"})).unwrap();
    h.state_mut().open_dialog("goalSeek", json!({}));
    h.run_steps(3);
    click_nth(&mut h, "Collapse dialog", 1);
    drag(&mut h, "A2", "A2");
    assert_eq!(value_of(&h, "changing"), Some(json!("$A$2")), "By changing cell stays absolute");
}
