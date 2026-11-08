//! Data › Outline › Settings: the Automatic styles check box (`automaticStyles`) is stored on the
//! sheet, reported back, and undone like the summary directions.

use gridcraft_engine::Session;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn apply_styles(s: &Session) -> bool {
    s.doc().unwrap().wb.active().unwrap().outline_apply_styles
}

#[test]
fn automatic_styles_is_off_in_a_new_sheet() {
    let s = session();
    assert!(!apply_styles(&s));
}

#[test]
fn automatic_styles_turns_on_and_is_reported() {
    let mut s = session();
    let r = s.execute("data.outlineSettings", json!({"automaticStyles": true})).unwrap();
    assert!(apply_styles(&s));
    assert_eq!(r["automaticStyles"], json!(true));
    assert_eq!(r["summaryBelow"], json!(true));
    assert_eq!(r["summaryRight"], json!(true));
}

#[test]
fn leaving_it_out_keeps_it_and_the_directions_still_work() {
    let mut s = session();
    s.execute("data.outlineSettings", json!({"automaticStyles": true})).unwrap();
    let r = s.execute("data.outlineSettings", json!({"summaryBelow": false})).unwrap();
    assert!(apply_styles(&s));
    assert_eq!(r["automaticStyles"], json!(true));
    assert!(!s.doc().unwrap().wb.active().unwrap().summary_below);
}

#[test]
fn automatic_styles_undoes() {
    let mut s = session();
    s.execute("data.outlineSettings", json!({"automaticStyles": true})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert!(!apply_styles(&s));
    s.execute("edit.redo", json!({})).unwrap();
    assert!(apply_styles(&s));
}
