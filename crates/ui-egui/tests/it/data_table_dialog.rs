//! Data › What-If Analysis › Data Table…: the dialog takes a row and/or column input cell, builds the
//! table over the selection, and keeps itself open with Excel's message when the input is not valid.

use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// Rate B1 (5%), amount B2 (1000); a column table D2:E5 (amounts down D3:D5, formula in E2) and a
/// two-variable table G2:I4 (formula in G2, rates across H2:I2, amounts down G3:G4).
fn harness(selection: &str) -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "B1", "values": [[0.05], [1000]]})).unwrap();
    s.execute("range.setValues", json!({"range": "D3", "values": [[100], [200], [300]]})).unwrap();
    s.execute("cell.set", json!({"cell": "E2", "input": "=B2*(1+B1)"})).unwrap();
    s.execute("cell.set", json!({"cell": "G2", "input": "=B2*(1+B1)"})).unwrap();
    s.execute("range.setValues", json!({"range": "H2", "values": [[0.1, 0.2]]})).unwrap();
    s.execute("range.setValues", json!({"range": "G3", "values": [[100], [200]]})).unwrap();
    s.execute("selection.set", json!({"range": selection})).unwrap();
    let mut h = crate::common::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        crate::common::app(s),
    );
    h.state_mut().open_dialog("dataTable", json!({}));
    h.run_steps(4);
    h
}

fn set_field(h: &mut Harness, key: &str, text: &str) {
    h.state_mut().dialog.as_mut().expect("the Data Table dialog is open").values.insert(key.into(), json!(text));
    h.run_steps(2);
}

fn click(h: &mut Harness, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(4);
}

fn value(h: &mut Harness, a1: &str) -> Json {
    h.state_mut().session.execute("cell.get", json!({"cell": a1})).unwrap()["value"].clone()
}

fn near(h: &mut Harness, a1: &str, want: f64) {
    let got = value(h, a1).as_f64().unwrap_or(f64::NAN);
    assert!((got - want).abs() < 1e-9, "{a1}: got {got}, want {want}");
}

fn message(h: &Harness) -> Option<(String, String)> {
    h.state().message.clone()
}

#[test]
fn the_dialog_opens_titled_data_table_with_both_input_fields_empty() {
    let h = harness("D2:E5");
    let d = h.state().dialog.as_ref().expect("dialog open");
    assert_eq!(d.title, "Data Table");
    for key in ["rowInput", "columnInput"] {
        assert_eq!(d.values.get(key).and_then(Json::as_str).unwrap_or(""), "", "{key} starts empty");
    }
    h.get_by_label("Row input cell:");
    h.get_by_label("Column input cell:");
}

#[test]
fn a_column_input_cell_fills_a_one_variable_table_over_the_selection() {
    let mut h = harness("D2:E5");
    set_field(&mut h, "columnInput", "B2");
    click(&mut h, "OK");
    assert!(h.state().dialog.is_none(), "OK closes the dialog");
    assert_eq!(message(&h), None);
    near(&mut h, "E3", 105.0);
    near(&mut h, "E4", 210.0);
    near(&mut h, "E5", 315.0);
}

#[test]
fn row_and_column_input_cells_fill_a_two_variable_table() {
    let mut h = harness("G2:I4");
    set_field(&mut h, "rowInput", "B1");
    set_field(&mut h, "columnInput", "B2");
    click(&mut h, "OK");
    assert!(h.state().dialog.is_none());
    near(&mut h, "H3", 110.0);
    near(&mut h, "I3", 120.0);
    near(&mut h, "H4", 220.0);
    near(&mut h, "I4", 240.0);
}

#[test]
fn both_fields_empty_keeps_the_dialog_open_with_excels_message() {
    let mut h = harness("D2:E5");
    click(&mut h, "OK");
    assert!(h.state().dialog.is_some(), "the dialog stays open");
    let (_, text) = message(&h).expect("a message is shown");
    assert_eq!(text, "Input cell reference is not valid.");
    assert_eq!(value(&mut h, "E3"), Json::Null, "nothing is written");
}

#[test]
fn an_input_cell_inside_the_table_is_refused_and_the_dialog_stays_open() {
    let mut h = harness("D2:E5");
    set_field(&mut h, "columnInput", "D3");
    click(&mut h, "OK");
    assert!(h.state().dialog.is_some(), "the dialog stays open");
    let (title, text) = message(&h).expect("a message is shown");
    assert_eq!(title, "Data Table");
    assert!(text.contains("Input cell reference is not valid."), "{text}");
    assert_eq!(value(&mut h, "E3"), Json::Null, "nothing is written");
}

#[test]
fn cancel_closes_without_building_the_table() {
    let mut h = harness("D2:E5");
    set_field(&mut h, "columnInput", "B2");
    click(&mut h, "Cancel");
    assert!(h.state().dialog.is_none());
    assert_eq!(value(&mut h, "E3"), Json::Null);
}
