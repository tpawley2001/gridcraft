//! Page Setup's Fit To and the page counts a sheet keeps while Fit To is off (`pageSetup
//! fitToWidth` / `fitToHeight` without `pageSetUpPr fitToPage`, ECMA-376 Part 1 §18.3.1.63 and
//! §18.3.1.65): switching Fit To on starts from the kept counts, switching it off keeps them.

use std::sync::Arc;

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

type Counts = Option<(u16, u16)>;

/// A new workbook whose active sheet has these Fit To settings.
fn sheet(fit_to: Counts, stored: Counts) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let wb = Arc::make_mut(&mut s.doc_mut().unwrap().wb);
    let pr = &mut wb.sheet_mut(0).unwrap().print;
    pr.fit_to = fit_to;
    pr.fit_to_stored = stored;
    s
}

/// (`fit_to`, `fit_to_stored`, `scale`) of the active sheet.
fn state(s: &Session) -> (Counts, Counts, u16) {
    let pr = &s.doc().unwrap().wb.sheet(0).unwrap().print;
    (pr.fit_to, pr.fit_to_stored, pr.scale)
}

/// Runs one command on a sheet that starts as (`fit_to`, `stored`) and returns its end state.
fn run(fit_to: Counts, stored: Counts, command: &str, params: Json) -> (Counts, Counts, u16) {
    let mut s = sheet(fit_to, stored);
    s.execute(command, params).unwrap();
    state(&s)
}

const SETUP: &str = "pageLayout.pageSetup";
const RIBBON: &str = "pageLayout.scaleToFit";

#[test]
fn fit_to_page_on_starts_from_the_stored_counts() {
    assert_eq!(run(None, Some((2, 3)), SETUP, json!({"fitToPage": true})), (Some((2, 3)), None, 100));
    assert_eq!(run(None, None, SETUP, json!({"fitToPage": true})), (Some((1, 1)), None, 100));
}

#[test]
fn fit_to_page_on_keeps_the_counts_of_a_sheet_that_already_fits() {
    assert_eq!(run(Some((2, 3)), None, SETUP, json!({"fitToPage": true})), (Some((2, 3)), None, 100));
}

#[test]
fn fit_to_page_off_keeps_the_counts_for_later() {
    assert_eq!(run(Some((2, 3)), None, SETUP, json!({"fitToPage": false})), (None, Some((2, 3)), 100));
    // 1 by 1 is the default, which is not stored.
    assert_eq!(run(Some((1, 1)), None, SETUP, json!({"fitToPage": false})), (None, None, 100));
    // Already off: the stored counts stay.
    assert_eq!(run(None, Some((2, 3)), SETUP, json!({"fitToPage": false})), (None, Some((2, 3)), 100));
}

#[test]
fn a_percentage_scale_turns_fit_to_off_and_keeps_the_counts() {
    assert_eq!(run(Some((2, 0)), None, SETUP, json!({"scale": 80})), (None, Some((2, 0)), 80));
    assert_eq!(run(None, Some((2, 3)), SETUP, json!({"scale": 80})), (None, Some((2, 3)), 80));
    assert_eq!(run(Some((2, 3)), None, RIBBON, json!({"scale": 90})), (None, Some((2, 3)), 90));
}

#[test]
fn one_count_alone_takes_the_other_from_the_stored_counts() {
    assert_eq!(run(None, Some((2, 3)), SETUP, json!({"fitWidth": 4})), (Some((4, 3)), None, 100));
    assert_eq!(run(None, Some((2, 3)), SETUP, json!({"fitHeight": 5})), (Some((2, 5)), None, 100));
    assert_eq!(run(None, None, SETUP, json!({"fitWidth": 4})), (Some((4, 1)), None, 100));
    assert_eq!(run(Some((2, 3)), None, SETUP, json!({"fitHeight": 5})), (Some((2, 5)), None, 100));
}

#[test]
fn counts_given_with_fit_to_page_are_kept() {
    assert_eq!(run(None, Some((2, 3)), SETUP, json!({"fitWidth": 4, "fitToPage": true})), (Some((4, 3)), None, 100));
    assert_eq!(run(None, None, SETUP, json!({"fitWidth": 2, "fitHeight": 0, "fitToPage": true})), (Some((2, 0)), None, 100));
}

#[test]
fn the_ribbon_width_and_height_clear_the_stored_counts() {
    assert_eq!(run(None, Some((2, 3)), RIBBON, json!({"width": 1, "height": 0})), (Some((1, 0)), None, 100));
}

#[test]
fn off_then_on_again_gives_the_same_counts_and_undo_restores_them() {
    let mut s = sheet(Some((3, 7)), None);
    s.execute(SETUP, json!({"fitToPage": false})).unwrap();
    assert_eq!(state(&s), (None, Some((3, 7)), 100));
    s.execute(SETUP, json!({"fitToPage": true})).unwrap();
    assert_eq!(state(&s), (Some((3, 7)), None, 100));
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(state(&s), (None, Some((3, 7)), 100));
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(state(&s), (Some((3, 7)), None, 100));
}
