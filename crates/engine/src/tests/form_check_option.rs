//! Form check boxes and option buttons on the sheet: insert, click (`control.press`),
//! `control.setValue`, the linked cell in both directions, and one selected option per sheet group.

use gridcraft_core::{CellError, Value};
use gridcraft_model::{ControlKind, FormControl};
use serde_json::{Value as Json, json};

use super::{s, v};
use crate::Session;

fn id(r: &Json) -> u64 {
    r["control"].as_u64().unwrap()
}

fn control(s: &Session, id: u64) -> FormControl {
    s.doc().unwrap().wb.active().unwrap().controls.iter().find(|c| c.id as u64 == id).cloned().unwrap()
}

fn undo_len(s: &Session) -> usize {
    s.doc().unwrap().undo.len()
}

fn values(s: &Session, ids: &[u64]) -> Vec<u32> {
    ids.iter().map(|i| control(s, *i).value).collect()
}

#[test]
fn a_check_box_is_inserted_with_its_caption_and_link() {
    let mut s = s();
    let c = id(&s.execute("insert.formCheckBox", json!({"at": "E3", "text": "Paid", "link": "B1"})).unwrap());
    let k = control(&s, c);
    assert_eq!((k.kind, k.text.as_str(), k.link.as_deref(), k.value), (ControlKind::CheckBox, "Paid", Some("$B$1"), 0));
    let list = s.execute("control.list", json!({})).unwrap();
    assert!(list.to_string().contains("\"checkBox\""), "{list}");
    // Without a caption it gets Excel's default.
    let d = id(&s.execute("insert.formCheckBox", json!({"at": "E6"})).unwrap());
    assert_eq!(control(&s, d).text, format!("Check Box {d}"));
}

#[test]
fn clicking_a_check_box_toggles_it_and_writes_true_or_false_as_one_undo_step() {
    let mut s = s();
    s.execute("cell.set", json!({"cell": "C1", "input": "=IF(B1,\"yes\",\"no\")"})).unwrap();
    let c = id(&s.execute("insert.formCheckBox", json!({"at": "E3", "link": "B1"})).unwrap());
    let before = undo_len(&s);
    s.execute("control.press", json!({"id": c})).unwrap();
    assert_eq!(control(&s, c).value, 1);
    assert_eq!(v(&s, "B1"), Value::Bool(true));
    assert_eq!(v(&s, "C1"), Value::Text("yes".into()), "dependents recalculate");
    s.execute("control.press", json!({"id": c})).unwrap();
    assert_eq!(control(&s, c).value, 0);
    assert_eq!(v(&s, "B1"), Value::Bool(false));
    assert_eq!(undo_len(&s), before + 2, "one undo step per click");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(control(&s, c).value, 1);
    assert_eq!(v(&s, "B1"), Value::Bool(true));
}

#[test]
fn set_value_on_a_check_box_writes_true_false_or_na() {
    let mut s = s();
    let c = id(&s.execute("insert.formCheckBox", json!({"link": "B1"})).unwrap());
    for (given, cell) in [(1, Value::Bool(true)), (2, Value::Error(CellError::NA)), (0, Value::Bool(false))] {
        s.execute("control.setValue", json!({"id": c, "value": given})).unwrap();
        assert_eq!(control(&s, c).value, given as u32, "{given}");
        assert_eq!(v(&s, "B1"), cell, "{given}");
    }
    assert!(s.execute("control.setValue", json!({"id": c, "value": 3})).is_err());
}

#[test]
fn the_linked_cell_drives_the_check_box() {
    let mut s = s();
    let c = id(&s.execute("insert.formCheckBox", json!({"link": "B1"})).unwrap());
    s.execute("cell.set", json!({"cell": "B1", "input": "TRUE"})).unwrap();
    assert_eq!(control(&s, c).value, 1);
    s.execute("cell.set", json!({"cell": "B1", "input": "FALSE"})).unwrap();
    assert_eq!(control(&s, c).value, 0);
    s.execute("cell.set", json!({"cell": "B1", "input": "=1=1"})).unwrap();
    assert_eq!(control(&s, c).value, 1, "a formula giving TRUE");
    s.execute("cell.set", json!({"cell": "B1", "input": "=NA()"})).unwrap();
    assert_eq!(control(&s, c).value, 2, "#N/A = mixed");
}

#[test]
fn option_buttons_share_one_link_and_one_selection() {
    let mut s = s();
    let a = id(&s.execute("insert.optionButton", json!({"at": "E2", "text": "Small", "link": "C1"})).unwrap());
    let b = id(&s.execute("insert.optionButton", json!({"at": "E4", "text": "Medium"})).unwrap());
    let c = id(&s.execute("insert.optionButton", json!({"at": "E6", "text": "Large"})).unwrap());
    assert_eq!(control(&s, b).kind, ControlKind::OptionButton);
    assert_eq!(values(&s, &[a, b, c]), [0, 0, 0], "new option buttons start unselected");
    for i in [a, b, c] {
        assert_eq!(control(&s, i).link.as_deref(), Some("$C$1"), "the group shares the link");
    }
    s.execute("control.press", json!({"id": b})).unwrap();
    assert_eq!(values(&s, &[a, b, c]), [0, 1, 0]);
    assert_eq!(v(&s, "C1"), Value::Number(2.0), "the 1-based position of the selected button");
    s.execute("control.press", json!({"id": c})).unwrap();
    assert_eq!(values(&s, &[a, b, c]), [0, 0, 1]);
    assert_eq!(v(&s, "C1"), Value::Number(3.0));
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(values(&s, &[a, b, c]), [0, 1, 0], "undo restores the whole group");
    assert_eq!(v(&s, "C1"), Value::Number(2.0));
}

#[test]
fn a_new_link_on_one_option_button_moves_the_whole_group() {
    let mut s = s();
    let a = id(&s.execute("insert.optionButton", json!({"at": "E2"})).unwrap());
    let b = id(&s.execute("insert.optionButton", json!({"at": "E4"})).unwrap());
    s.execute("control.format", json!({"id": b, "link": "D9"})).unwrap();
    assert_eq!(control(&s, a).link.as_deref(), Some("$D$9"));
    assert_eq!(control(&s, b).link.as_deref(), Some("$D$9"));
    assert_eq!(control(&s, a).text, format!("Option Button {a}"));
}

#[test]
fn the_linked_cell_selects_the_option_by_number() {
    let mut s = s();
    let a = id(&s.execute("insert.optionButton", json!({"link": "C1"})).unwrap());
    let b = id(&s.execute("insert.optionButton", json!({})).unwrap());
    let c = id(&s.execute("insert.optionButton", json!({})).unwrap());
    s.execute("cell.set", json!({"cell": "C1", "input": "3"})).unwrap();
    assert_eq!(values(&s, &[a, b, c]), [0, 0, 1]);
    s.execute("cell.set", json!({"cell": "C1", "input": "1"})).unwrap();
    assert_eq!(values(&s, &[a, b, c]), [1, 0, 0]);
    for none in ["0", "9", "text"] {
        s.execute("cell.set", json!({"cell": "C1", "input": none})).unwrap();
        assert_eq!(values(&s, &[a, b, c]), [0, 0, 0], "{none}");
    }
    // setValue 1 selects it like a click.
    s.execute("control.setValue", json!({"id": b, "value": 1})).unwrap();
    assert_eq!(values(&s, &[a, b, c]), [0, 1, 0]);
    assert_eq!(v(&s, "C1"), Value::Number(2.0));
}
