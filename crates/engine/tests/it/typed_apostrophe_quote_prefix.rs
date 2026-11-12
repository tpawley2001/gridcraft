//! Typing a leading apostrophe stores the text without it and marks the cell's format with the
//! quote prefix (`Style::quote_prefix`, XLSX `xf quotePrefix`), as Excel does; the editor shows
//! the apostrophe again for a marked text cell. An entry without an apostrophe clears the mark.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

fn new_session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn set(s: &mut Session, cell: &str, input: &str) {
    s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
}

/// (value, quote_prefix of the cell's style, editor text) of a cell on the active sheet.
fn state(s: &Session, cell: &str) -> (Value, bool, String) {
    let d = s.doc().unwrap();
    let sh = d.wb.active().unwrap();
    let at = CellRef::parse(cell).unwrap();
    let c = sh.cell(at).unwrap();
    (c.value.clone(), d.wb.styles.get(c.style).quote_prefix, gridcraft_engine::display::edit_text(&d.wb, sh, at))
}

fn text(t: &str) -> Value {
    Value::Text(t.into())
}

#[test]
fn an_apostrophe_entry_sets_the_quote_prefix_and_the_editor_shows_it() {
    let mut s = new_session();
    for (cell, input, stored) in [("A1", "'0123", "0123"), ("A2", "'hello", "hello"), ("A3", "''abc", "'abc"), ("A4", "'=1+1", "=1+1")] {
        set(&mut s, cell, input);
        assert_eq!(state(&s, cell), (text(stored), true, input.to_string()), "{cell}");
    }
}

#[test]
fn an_entry_without_an_apostrophe_has_no_quote_prefix() {
    let mut s = new_session();
    set(&mut s, "A1", "hello");
    assert_eq!(state(&s, "A1"), (text("hello"), false, "hello".to_string()));
    set(&mut s, "A2", "123");
    assert_eq!(state(&s, "A2"), (Value::Number(123.0), false, "123".to_string()));
}

#[test]
fn typing_over_a_quote_prefixed_cell_without_an_apostrophe_clears_the_mark() {
    let mut s = new_session();
    set(&mut s, "A1", "'hello");
    set(&mut s, "A1", "hello");
    assert_eq!(state(&s, "A1"), (text("hello"), false, "hello".to_string()));
    set(&mut s, "A2", "'0123");
    set(&mut s, "A2", "0123");
    assert_eq!(state(&s, "A2"), (Value::Number(123.0), false, "123".to_string()));
    set(&mut s, "A3", "'7");
    set(&mut s, "A3", "=1+1");
    let (value, quote_prefix, _) = state(&s, "A3");
    assert_eq!((value, quote_prefix), (Value::Number(2.0), false));
}

#[test]
fn the_quote_prefix_keeps_the_rest_of_the_cell_format() {
    let mut s = new_session();
    s.execute("selection.set", json!({"range": "B2"})).unwrap();
    s.execute("home.bold", json!({})).unwrap();
    set(&mut s, "B2", "'5");
    assert_eq!(state(&s, "B2"), (text("5"), true, "'5".to_string()));
    {
        let d = s.doc().unwrap();
        let sh = d.wb.active().unwrap();
        assert!(d.wb.styles.get(sh.style_id(CellRef::parse("B2").unwrap())).font.bold);
    }
    // A neighbour with the same bold format is not marked.
    s.execute("selection.set", json!({"range": "B3"})).unwrap();
    s.execute("home.bold", json!({})).unwrap();
    set(&mut s, "B3", "x");
    assert_eq!(state(&s, "B3"), (text("x"), false, "x".to_string()));
}

#[test]
fn undo_takes_back_the_entry_and_its_quote_prefix_together() {
    let mut s = new_session();
    set(&mut s, "A1", "plain");
    set(&mut s, "A1", "'0123");
    assert_eq!(state(&s, "A1"), (text("0123"), true, "'0123".to_string()));
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(state(&s, "A1"), (text("plain"), false, "plain".to_string()));
}
