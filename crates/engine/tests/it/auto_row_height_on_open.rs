//! Automatic (non-custom) row heights: opening and recalculating a file keeps the stored height, and
//! AutoFit ignores cells in hidden columns, as Excel does. A real .xls with wrapped text in a hidden
//! column came back with rows of 409.5 pt and printed 126 pages where Excel prints 9.

use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::model::{Cell, Formula, LineInfo, Style, Workbook};
use gridcraft_engine::{DocState, Session, io};
use serde_json::json;

const PX_PER_PT: f32 = 96.0 / 72.0;

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// Row 2 (index 1) holds wrapped headings in columns A:G, columns B:E are wide, F:G hidden, and the
/// row stores an automatic height of 33.75 pt (BIFF ROW miyRw = 675 twips with fUnsynced clear).
fn workbook() -> Workbook {
    let mut wb = Workbook::new();
    let mut wrap = Style::default();
    wrap.align.wrap = true;
    let wrap = wb.styles.intern(wrap);
    let sh = wb.sheet_mut(0).unwrap();
    for (col, chars, hidden) in
        [(0u32, 12.14, false), (1, 36.14, false), (2, 35.71, false), (3, 30.71, false), (4, 10.71, false), (5, 12.71, true), (6, 18.28, true)]
    {
        let px = (chars * 7.0 + 5.0) as f32;
        sh.cols.insert(col, LineInfo { size: Some(px), hidden, custom: true, ..LineInfo::default() });
    }
    for (cell, text) in
        [("A2", "First Author"), ("B2", "Title"), ("C2", "Authors"), ("D2", "Affiliation"), ("E2", "Country"), ("G2", "e-mail address")]
    {
        let mut c = Cell::value(Value::text(text));
        c.style = wrap;
        sh.cells.set(at(cell), c);
    }
    sh.cells.set(at("A3"), Cell::formula(Formula::new("1+1")));
    sh.rows.insert(1, LineInfo { size: Some(33.75 * PX_PER_PT), ..LineInfo::default() });
    wb
}

fn session(wb: Workbook) -> Session {
    let mut s = Session::new();
    s.add_document(DocState::new(wb, None, "heights".into(), gridcraft_engine::Trust::FromFile));
    s
}

fn row2_pt(s: &Session) -> f32 {
    s.doc().unwrap().wb.active().unwrap().row_height(1) / PX_PER_PT
}

#[test]
fn open_and_recalculate_keep_the_stored_automatic_height() {
    let bytes = io::save_bytes(&workbook(), "heights.xlsx").expect("save");
    let (reopened, _) = io::open_bytes("heights.xlsx", &bytes).expect("open");
    let mut s = session(reopened);
    assert!((row2_pt(&s) - 33.75).abs() < 0.5, "after open: {} pt", row2_pt(&s));
    s.execute("formulas.calculateNow", json!({})).expect("recalc");
    assert!((row2_pt(&s) - 33.75).abs() < 0.5, "after recalc: {} pt", row2_pt(&s));
    let pdf_bytes = io::save_bytes(&s.doc().unwrap().wb, "heights.xlsx").expect("save again");
    let (again, _) = io::open_bytes("heights.xlsx", &pdf_bytes).expect("reopen");
    let ht = again.sheet(0).unwrap().row_height(1) / PX_PER_PT;
    assert!((ht - 33.75).abs() < 0.5, "after save + reopen: {ht} pt");
}

#[test]
fn autofit_ignores_wrapped_text_in_hidden_columns() {
    let mut s = session(workbook());
    s.execute("home.autofitRowHeight", json!({"rows": "2:2"})).expect("autofit");
    let fitted = row2_pt(&s);
    // Every visible heading fits on one line of its column: one line of the default font.
    let default = s.doc().unwrap().wb.active().unwrap().default_row_height / PX_PER_PT;
    assert!(fitted < 409.0, "hidden column G must not drive the height: {fitted} pt");
    assert!((fitted - default).abs() < 1.0, "one line expected ({default} pt), got {fitted} pt");
}

#[test]
fn autofit_wraps_against_the_visible_column_width() {
    let mut s = session(workbook());
    {
        let wb = Arc::make_mut(&mut s.active_mut().unwrap().wb);
        let sh = wb.sheet_mut(0).unwrap();
        let mut c = sh.cells.get(at("B2")).cloned().unwrap();
        c.value = Value::text("one\ntwo\nthree");
        sh.cells.set(at("B2"), c);
    }
    s.execute("home.autofitRowHeight", json!({"rows": "2:2"})).expect("autofit");
    let default = s.doc().unwrap().wb.active().unwrap().default_row_height / PX_PER_PT;
    let fitted = row2_pt(&s);
    assert!(fitted > 2.5 * default && fitted < 3.5 * default, "three lines expected (~{} pt), got {fitted} pt", 3.0 * default);
}
