//! After a paste of copied cells, the Paste Options button redoes the paste as Values, Formulas,
//! Formatting, Transpose, Keep Source Column Widths or Paste Link, in one undo step, as in Excel.

use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A1 = 2 (bold), A2 = =A1*3; column A is 20 characters wide. A1:A2 is copied and C1 is active.
fn setup(s: &mut Session) {
    s.execute("cell.set", json!({"cell": "A1", "input": "2"})).unwrap();
    s.execute("cell.set", json!({"cell": "A2", "input": "=A1*3"})).unwrap();
    s.execute("home.bold", json!({"range": "A1", "on": true})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 20})).unwrap();
    s.execute("edit.copy", json!({"range": "A1:A2"})).unwrap();
    s.execute("selection.set", json!({"range": "C1"})).unwrap();
}

fn harness() -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    setup(&mut session);
    crate::support::stepped_harness(session)
}

fn paste(h: &mut Harness) {
    h.state_mut().paste_from_clipboard();
    h.run_steps(4);
}

fn input(h: &Harness, cell: &str) -> String {
    h.state().session.active().unwrap().wb.active().unwrap().input_text(CellRef::parse(cell).unwrap())
}

fn bold(h: &Harness, cell: &str) -> bool {
    let d = h.state().session.active().unwrap();
    let sh = d.wb.active().unwrap();
    d.wb.styles.get(sh.style_id(CellRef::parse(cell).unwrap())).font.bold
}

fn undo_len(h: &Harness) -> usize {
    h.state().session.active().unwrap().undo.len()
}

fn col_width(h: &Harness, col: u32) -> f32 {
    h.state().session.active().unwrap().wb.active().unwrap().col_width(col)
}

/// Opens the Paste Options menu and picks `item` (its label with the access key, e.g. "Values (V)").
fn pick(h: &mut Harness, item: &str) {
    h.get_by_label("Paste Options").click();
    h.run_steps(4);
    h.get_by_label(item).click();
    h.run_steps(4);
}

#[test]
fn a_paste_of_copied_cells_shows_the_button() {
    let mut h = harness();
    assert!(h.query_by_label("Paste Options").is_none());
    paste(&mut h);
    assert_eq!(input(&h, "C2"), "=C1*3");
    assert!(h.query_by_label("Paste Options").is_some());
}

#[test]
fn values_replaces_the_paste_in_one_undo_step() {
    let mut h = harness();
    let before = undo_len(&h);
    paste(&mut h);
    pick(&mut h, "Values (V)");
    assert_eq!(input(&h, "C1"), "2");
    assert_eq!(input(&h, "C2"), "6");
    assert!(!bold(&h, "C1"), "values only: the bold format is not pasted");
    assert_eq!(undo_len(&h), before + 1, "one paste step, not two");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(input(&h, "C1"), "", "one Undo restores the cells from before the paste");
    assert_eq!(input(&h, "C2"), "");
}

#[test]
fn formulas_keeps_the_formula_without_the_format() {
    let mut h = harness();
    paste(&mut h);
    pick(&mut h, "Formulas (F)");
    assert_eq!(input(&h, "C1"), "2");
    assert_eq!(input(&h, "C2"), "=C1*3");
    assert!(!bold(&h, "C1"));
}

#[test]
fn formatting_pastes_only_the_formats() {
    let mut h = harness();
    paste(&mut h);
    pick(&mut h, "Formatting (R)");
    assert_eq!(input(&h, "C1"), "");
    assert_eq!(input(&h, "C2"), "");
    assert!(bold(&h, "C1"));
}

#[test]
fn transpose_turns_the_column_into_a_row() {
    // The formula cell is whatever Paste Special with Transpose makes (how references move under
    // a transpose is Paste Special's job, not the button's).
    let mut s = Session::new();
    s.new_workbook();
    setup(&mut s);
    s.execute("edit.pasteSpecial", json!({"what": "all", "transpose": true})).unwrap();
    let want = s.active().unwrap().wb.active().unwrap().input_text(CellRef::parse("D1").unwrap());

    let mut h = harness();
    paste(&mut h);
    pick(&mut h, "Transpose (T)");
    assert_eq!(input(&h, "C1"), "2");
    assert!(input(&h, "D1").starts_with('='), "D1 holds the transposed formula");
    assert_eq!(input(&h, "D1"), want);
    assert_eq!(input(&h, "C2"), "", "the first paste's second cell is gone");
}

#[test]
fn keep_source_column_widths_also_copies_the_width() {
    let mut h = harness();
    let before = undo_len(&h);
    paste(&mut h);
    assert_ne!(col_width(&h, 2), col_width(&h, 0));
    pick(&mut h, "Keep Source Column Widths (W)");
    assert_eq!(col_width(&h, 2), col_width(&h, 0));
    assert_eq!(input(&h, "C2"), "=C1*3");
    assert!(bold(&h, "C1"));
    assert_eq!(undo_len(&h), before + 1);
}

#[test]
fn paste_link_gives_what_paste_special_link_gives() {
    // The expected cells are what Paste Special with Paste Link makes in a fresh workbook.
    let mut s = Session::new();
    s.new_workbook();
    setup(&mut s);
    s.execute("edit.pasteSpecial", json!({"what": "all", "link": true})).unwrap();
    let want: Vec<String> = ["C1", "C2"].iter().map(|c| s.active().unwrap().wb.active().unwrap().input_text(CellRef::parse(c).unwrap())).collect();

    let mut h = harness();
    paste(&mut h);
    pick(&mut h, "Paste Link (N)");
    assert_eq!(vec![input(&h, "C1"), input(&h, "C2")], want);
}

#[test]
fn a_second_pick_replaces_the_first() {
    let mut h = harness();
    let before = undo_len(&h);
    paste(&mut h);
    pick(&mut h, "Transpose (T)");
    pick(&mut h, "Values (V)");
    assert_eq!(input(&h, "D1"), "", "the transposed paste is undone");
    assert_eq!(input(&h, "C2"), "6");
    assert_eq!(undo_len(&h), before + 1);
}

#[test]
fn the_access_key_picks_an_item_while_the_menu_is_open() {
    let mut h = harness();
    paste(&mut h);
    h.get_by_label("Paste Options").click();
    h.run_steps(4);
    h.key_press(egui::Key::V);
    h.run_steps(4);
    assert_eq!(input(&h, "C2"), "6");
}

#[test]
fn the_button_goes_once_the_workbook_changes() {
    let mut h = harness();
    paste(&mut h);
    h.state_mut().run("cell.set", json!({"cell": "E5", "input": "x"})).unwrap();
    h.run_steps(4);
    assert!(h.query_by_label("Paste Options").is_none());
}

#[test]
fn no_button_after_pasting_a_cut() {
    let mut h = harness();
    h.state_mut().run("edit.cut", json!({"range": "A1:A2"})).unwrap();
    h.state_mut().session.execute("selection.set", json!({"range": "C1"})).unwrap();
    h.run_steps(2);
    paste(&mut h);
    assert_eq!(input(&h, "C1"), "2", "the cut moved");
    assert!(h.query_by_label("Paste Options").is_none());
}
