//! Go To Special "Same" picks the rule that holds the active cell and selects that rule's ranges as
//! areas, whatever their size: data validation over whole columns, and conditional formats.

use gridcraft_engine::Session;
use serde_json::{Value, json};

fn book() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "1"})).unwrap();
    s
}

fn select(s: &mut Session, range: &str, active: &str) {
    s.execute("selection.set", json!({"range": range, "active": active})).unwrap();
}

/// Runs Go To Special and returns the reply and the selected areas as A1 strings.
fn special(s: &mut Session, p: Value) -> (Value, Vec<String>) {
    let reply = s.execute("edit.goToSpecial", p.clone()).unwrap_or_else(|e| panic!("{p}: {e}"));
    let areas = s.doc().unwrap().selection.ranges.iter().map(|r| r.a1()).collect();
    (reply, areas)
}

fn active(s: &Session) -> String {
    s.doc().unwrap().selection.active.a1()
}

#[test]
fn validation_same_over_whole_columns_selects_the_columns() {
    // Four whole columns hold more than 4,000,000 cells: still selected, as one area.
    let mut s = book();
    s.execute("data.validation", json!({"range": "B:E", "type": "list", "formula1": "a,b"})).unwrap();
    select(&mut s, "D7", "D7");
    let (reply, areas) = special(&mut s, json!({"kind": "dataValidation", "same": true}));
    assert_eq!(areas, vec!["B:E".to_string()], "{reply}");
    assert_eq!(reply["count"].as_f64(), Some(4.0 * 1_048_576.0), "{reply}");
    assert_eq!(active(&s), "B1");
}

#[test]
fn validation_same_keeps_each_range_of_the_rule_as_an_area() {
    let mut s = book();
    s.execute("data.validation", json!({"range": "B:C,F2:F4", "type": "whole", "operator": "between", "formula1": "1", "formula2": "9"})).unwrap();
    s.execute("data.validation", json!({"range": "H1:H3", "type": "list", "formula1": "x,y"})).unwrap();
    select(&mut s, "F3", "F3");
    let (reply, areas) = special(&mut s, json!({"kind": "dataValidation", "same": true}));
    assert_eq!(areas, vec!["B:C".to_string(), "F2:F4".to_string()], "{reply}");
    assert_eq!(reply["count"].as_f64(), Some(2.0 * 1_048_576.0 + 3.0), "{reply}");
    assert_eq!(active(&s), "B1");
}

fn formatted() -> Session {
    let mut s = book();
    let rule = |f: &str| json!({"type": "expression", "formula": f, "preset": "lightRedFill"});
    s.execute("home.conditionalFormat", json!({"range": "A1:A5", "rule": rule("A1>5")})).unwrap();
    s.execute("home.conditionalFormat", json!({"range": "C1:C3", "rule": rule("C1<0")})).unwrap();
    s
}

#[test]
fn conditional_formats_same_selects_only_the_active_cells_rule() {
    let mut s = formatted();
    select(&mut s, "C2", "C2");
    let (reply, areas) = special(&mut s, json!({"kind": "conditionalFormats", "same": true}));
    assert_eq!(areas, vec!["C1:C3".to_string()], "{reply}");
    assert_eq!(reply["count"].as_f64(), Some(3.0), "{reply}");
    select(&mut s, "A4", "A4");
    let (_, areas) = special(&mut s, json!({"kind": "conditionalFormats", "same": true}));
    assert_eq!(areas, vec!["A1:A5".to_string()]);
}

#[test]
fn conditional_formats_same_over_whole_columns_is_not_refused() {
    let mut s = book();
    let rule = json!({"type": "expression", "formula": "E1>5", "preset": "lightRedFill"});
    s.execute("home.conditionalFormat", json!({"range": "E:H", "rule": rule})).unwrap();
    select(&mut s, "F9", "F9");
    let (reply, areas) = special(&mut s, json!({"kind": "conditionalFormats", "same": true}));
    assert_eq!(areas, vec!["E:H".to_string()], "{reply}");
}

#[test]
fn same_on_a_cell_without_a_rule_finds_nothing_and_keeps_the_selection() {
    let mut s = formatted();
    select(&mut s, "B2", "B2");
    for kind in ["conditionalFormats", "dataValidation"] {
        let err = s.execute("edit.goToSpecial", json!({"kind": kind, "same": true})).unwrap_err().to_string();
        assert!(err.contains("No cells were found"), "{kind}: {err}");
        assert_eq!(active(&s), "B2");
    }
}

#[test]
fn conditional_formats_all_still_selects_every_formatted_cell() {
    let mut s = formatted();
    select(&mut s, "A1:D5", "D5");
    let (_, areas) = special(&mut s, json!({"kind": "conditionalFormats"}));
    let cells: usize = s.doc().unwrap().selection.ranges.iter().map(|r| r.count() as usize).sum();
    assert_eq!(cells, 8, "{areas:?}");
}
