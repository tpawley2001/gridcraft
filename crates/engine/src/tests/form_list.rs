//! Form list boxes and drop-downs on the sheet: choosing an item (`control.setValue`) writes its
//! 1-based position to the linked cell as one undo step, the linked cell moves the selection, and
//! structural edits rewrite the item range like the link.

use gridcraft_core::Value;
use gridcraft_model::{ControlKind, FormControl, ListSelection};
use serde_json::json;

use super::{s, v};
use crate::Session;

/// Puts a control of `kind` with items `$E$1:$E$4` and linked cell `$B$2` on the active sheet,
/// with the items typed in E1:E4. Returns its id.
fn with_list(s: &mut Session, kind: ControlKind) -> u32 {
    for (i, t) in ["Small", "Medium", "Large", "Huge"].iter().enumerate() {
        s.execute("cell.set", json!({"cell": format!("E{}", i + 1), "input": t})).unwrap();
    }
    let id = 900;
    let c = FormControl {
        id,
        kind,
        link: Some("$B$2".into()),
        input_range: Some("$E$1:$E$4".into()),
        anchor: gridcraft_model::Anchor { cell: gridcraft_core::CellRef::parse("G2").unwrap(), width: 120.0, height: 80.0, ..Default::default() },
        ..FormControl::default()
    };
    std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().wb).sheet_mut(0).unwrap().controls.push(c);
    id
}

fn control(s: &Session, id: u32) -> FormControl {
    s.doc().unwrap().wb.active().unwrap().controls.iter().find(|c| c.id == id).cloned().unwrap()
}

#[test]
fn choosing_a_drop_down_item_writes_the_linked_cell_as_one_undo_step() {
    let mut s = s();
    let id = with_list(&mut s, ControlKind::DropDown);
    let before = s.doc().unwrap().undo.len();
    assert_eq!(s.execute("control.setValue", json!({"id": id, "value": 3})).unwrap()["value"], 3);
    assert_eq!(control(&s, id).value, 3);
    assert_eq!(v(&s, "B2"), Value::number(3.0));
    assert_eq!(s.doc().unwrap().undo.len(), before + 1, "one undo step");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(control(&s, id).value, 0);
    assert_eq!(v(&s, "B2"), Value::Empty);
    // Past the last item is refused and changes nothing.
    assert!(s.execute("control.setValue", json!({"id": id, "value": 5})).is_err());
    assert_eq!(control(&s, id).value, 0);
}

#[test]
fn a_single_selection_list_box_shows_its_items_and_follows_its_linked_cell() {
    let mut s = s();
    let id = with_list(&mut s, ControlKind::ListBox);
    let c = control(&s, id);
    assert_eq!(c.selection, ListSelection::Single);
    let items = crate::cmd::controls::list_items(&s.doc().unwrap().wb, 0, &c);
    assert_eq!(items, ["Small", "Medium", "Large", "Huge"]);
    s.execute("control.setValue", json!({"id": id, "value": 2})).unwrap();
    assert_eq!(v(&s, "B2"), Value::number(2.0));
    s.execute("cell.set", json!({"cell": "B2", "input": "4"})).unwrap();
    assert_eq!(control(&s, id).value, 4, "editing the linked cell moves the selection");
    let list = s.execute("control.list", json!({})).unwrap();
    assert!(list.to_string().contains("\"listBox\"") && list.to_string().contains("$E$1:$E$4"), "{list}");
}

#[test]
fn a_row_inserted_above_the_items_rewrites_the_input_range() {
    let mut s = s();
    let id = with_list(&mut s, ControlKind::DropDown);
    s.execute("home.insertRows", json!({"rows": "1:1"})).unwrap();
    let c = control(&s, id);
    assert_eq!(c.input_range.as_deref(), Some("$E$2:$E$5"));
    assert_eq!(c.link.as_deref(), Some("$B$3"));
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(control(&s, id).input_range.as_deref(), Some("$E$1:$E$4"));
}

#[test]
fn group_boxes_and_labels_take_no_value() {
    let mut s = s();
    for kind in [ControlKind::GroupBox, ControlKind::Label] {
        let id = with_list(&mut s, kind);
        assert!(s.execute("control.setValue", json!({"id": id, "value": 1})).is_err());
        assert!(s.execute("control.press", json!({"id": id})).is_err());
        std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().wb).sheet_mut(0).unwrap().controls.clear();
    }
}
