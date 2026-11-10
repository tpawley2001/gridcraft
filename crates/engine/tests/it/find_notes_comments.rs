//! Find's "Look in" box: Notes searches cell notes, Comments searches threaded comments (and their
//! replies); Formulas and Values search neither.

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

fn workbook() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params).unwrap()
}

fn set(s: &mut Session, cell: &str, input: &str) {
    run(s, "cell.set", json!({"cell": cell, "input": input}));
}

fn note(s: &mut Session, cell: &str, text: &str) {
    run(s, "review.newNote", json!({"cell": cell, "text": text, "author": "Ann"}));
}

fn comment(s: &mut Session, cell: &str, text: &str) {
    run(s, "review.newComment", json!({"cell": cell, "text": text, "author": "Ann"}));
}

fn select(s: &mut Session, cell: &str) {
    run(s, "selection.set", json!({"range": cell}));
}

/// Find All: the hit cells in the order Find lists them.
fn find_all(s: &mut Session, params: Json) -> Vec<String> {
    let mut p = params;
    p["all"] = json!(true);
    match s.execute("edit.find", p) {
        Ok(r) => r["results"].as_array().unwrap().iter().map(|h| h["cell"].as_str().unwrap().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

#[test]
fn notes_are_found_on_empty_cells() {
    let mut s = workbook();
    set(&mut s, "A1", "alpha");
    note(&mut s, "C3", "check the budget");
    select(&mut s, "A1");
    let r = run(&mut s, "edit.find", json!({"what": "budget", "lookIn": "notes"}));
    assert_eq!(r["cell"], "C3");
}

#[test]
fn formulas_and_values_do_not_search_notes() {
    let mut s = workbook();
    set(&mut s, "A1", "alpha");
    note(&mut s, "C3", "check the budget");
    for look in ["formulas", "values"] {
        assert!(s.execute("edit.find", json!({"what": "budget", "lookIn": look})).is_err(), "{look}");
    }
    assert!(s.execute("edit.find", json!({"what": "budget"})).is_err());
}

#[test]
fn notes_search_honours_match_case_and_skips_cell_text() {
    let mut s = workbook();
    set(&mut s, "A1", "budget");
    note(&mut s, "B2", "Budget Q1");
    note(&mut s, "D4", "budget q2");
    assert_eq!(find_all(&mut s, json!({"what": "budget", "lookIn": "notes", "matchCase": true})), ["D4"]);
    assert_eq!(find_all(&mut s, json!({"what": "budget", "lookIn": "notes"})), ["B2", "D4"]);
}

#[test]
fn notes_are_listed_by_rows_or_by_columns() {
    let mut s = workbook();
    note(&mut s, "B5", "x one");
    note(&mut s, "A2", "x two");
    note(&mut s, "C1", "x three");
    assert_eq!(find_all(&mut s, json!({"what": "x", "lookIn": "notes"})), ["C1", "A2", "B5"]);
    assert_eq!(find_all(&mut s, json!({"what": "x", "lookIn": "notes", "byColumns": true})), ["A2", "B5", "C1"]);
}

#[test]
fn notes_search_uses_wildcards() {
    let mut s = workbook();
    note(&mut s, "B2", "re-port");
    assert_eq!(find_all(&mut s, json!({"what": "re*rt", "lookIn": "notes"})), ["B2"]);
}

#[test]
fn comments_search_finds_text_in_replies() {
    let mut s = workbook();
    comment(&mut s, "B2", "first");
    run(&mut s, "review.replyComment", json!({"cell": "B2", "text": "see invoice 7", "author": "Bo"}));
    select(&mut s, "A1");
    let r = run(&mut s, "edit.find", json!({"what": "invoice", "lookIn": "comments"}));
    assert_eq!(r["cell"], "B2");
}

#[test]
fn notes_and_comments_are_searched_apart() {
    let mut s = workbook();
    comment(&mut s, "B2", "invoice thread");
    note(&mut s, "D1", "invoice");
    assert_eq!(find_all(&mut s, json!({"what": "invoice", "lookIn": "comments"})), ["B2"]);
    assert_eq!(find_all(&mut s, json!({"what": "invoice", "lookIn": "notes"})), ["D1"]);
}

#[test]
fn notes_search_within_workbook_goes_to_the_other_sheet() {
    let mut s = workbook();
    run(&mut s, "home.insertSheet", json!({"name": "Two"}));
    run(&mut s, "review.newNote", json!({"sheet": 1, "cell": "B1", "text": "zebra", "author": "Ann"}));
    run(&mut s, "sheet.activate", json!({"sheet": 0}));
    select(&mut s, "A1");
    let r = run(&mut s, "edit.find", json!({"what": "zebra", "lookIn": "notes", "within": "workbook"}));
    assert_eq!((r["sheet"].clone(), r["cell"].clone()), (json!(1), json!("B1")));
}
