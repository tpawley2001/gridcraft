//! Printed cell indent: each level is 3 spaces of the workbook's Normal style font (ECMA-376 18.8.1
//! `alignment/@indent`) and is not shrunk by page scaling. Expected values are measured from Excel's
//! own PDFs: Calibri 11 Normal 7.56 pt per level, Arial 10 Normal 8.30 pt per level at 100 %, and
//! 8.29 pt per level when the sheet printed at 35 % (fit to one page).
use serde_json::json;

use super::*;

/// A workbook whose Normal style font is `font` at `size`; column A is wide enough for the labels.
fn session(font: &str, size: f32) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut normal = Style::default();
    normal.font.name = font.to_string();
    normal.font.size = size;
    wb.styles = gridcraft_model::StyleTable::with_default(normal);
    s.execute("home.columnWidth", json!({"cols": "A:A", "chars": 40})).unwrap();
    s
}

/// Puts `text` in `cell`, left-aligned with `indent` levels, in the Normal font unless `cell_font` is given.
fn put(s: &mut Session, cell: &str, text: &str, indent: u8, cell_font: Option<(&str, f32)>) {
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut st = wb.styles.get(StyleId::DEFAULT).clone();
    st.align.h = HAlign::Left;
    st.align.indent = indent;
    if let Some((name, size)) = cell_font {
        st.font.name = name.to_string();
        st.font.size = size;
    }
    let id = wb.styles.intern(st);
    let c = CellRef::parse(cell).unwrap();
    let sh = wb.sheet_mut(0).unwrap();
    sh.set_value(c, Value::Text(text.into()));
    sh.set_style(c, id);
}

fn pdf(s: &mut Session) -> Vec<u8> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap()
}

/// The x position of the first `(text) Tj` in a PDF.
fn x_of(pdf: &[u8], text: &str) -> f32 {
    let s = String::from_utf8_lossy(pdf);
    let at = s.find(&format!("({text}) Tj")).unwrap_or_else(|| panic!("{text} not printed"));
    let before = &s[..at];
    let tm = before.rfind(" Tm").expect("Tm before text");
    let mut nums = before[..tm].rsplit([' ', '\n']);
    let _y = nums.next();
    nums.next().unwrap().parse().unwrap()
}

fn close(got: f32, want: f32, tol: f32, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got:.2} pt, Excel prints {want:.2} pt (tolerance {tol})");
}

/// Calibri 11 Normal font at 100 %: Excel 22.68 pt for 3 levels (7.56 pt per level).
#[test]
fn calibri_normal_font_indent_is_three_spaces_per_level() {
    let mut s = session("Calibri", 11.0);
    put(&mut s, "A1", "zero", 0, None);
    put(&mut s, "A2", "one", 1, None);
    put(&mut s, "A3", "three", 3, None);
    let p = pdf(&mut s);
    let x0 = x_of(&p, "zero");
    close(x_of(&p, "one") - x0, 7.56, 0.25, "Calibri 11, indent 1");
    close(x_of(&p, "three") - x0, 22.68, 0.5, "Calibri 11, indent 3");
}

/// Arial 10 Normal font at 100 %: Excel 8.30 pt per level.
#[test]
fn arial_normal_font_indent_is_three_spaces_per_level() {
    let mut s = session("Arial", 10.0);
    put(&mut s, "A1", "zero", 0, None);
    put(&mut s, "A2", "one", 1, None);
    put(&mut s, "A3", "two", 2, None);
    let p = pdf(&mut s);
    let x0 = x_of(&p, "zero");
    close(x_of(&p, "one") - x0, 8.30, 0.2, "Arial 10, indent 1");
    close(x_of(&p, "two") - x0, 16.60, 0.4, "Arial 10, indent 2");
}

/// The indent follows the Normal font, not the cell's own font: an Arial 8 cell and an Arial 20 cell
/// under an Arial 10 Normal font both indent 8.30 pt per level.
#[test]
fn indent_follows_the_normal_font_not_the_cell_font() {
    let mut s = session("Arial", 10.0);
    put(&mut s, "A1", "zero", 0, None);
    put(&mut s, "A2", "small", 1, Some(("Arial", 8.0)));
    put(&mut s, "A3", "large", 1, Some(("Arial", 20.0)));
    let p = pdf(&mut s);
    let x0 = x_of(&p, "zero");
    close(x_of(&p, "small") - x0, 8.30, 0.2, "Arial 8 cell, indent 1");
    close(x_of(&p, "large") - x0, 8.30, 0.2, "Arial 20 cell, indent 1");
}

/// Page scaling shrinks the text but not the indent (Excel printed 8.29 pt per level at 35 %).
#[test]
fn page_scaling_does_not_shrink_the_indent() {
    let mut s = session("Arial", 10.0);
    put(&mut s, "A1", "zero", 0, None);
    put(&mut s, "A2", "two", 2, None);
    s.execute("pageLayout.pageSetup", json!({"scale": 50})).unwrap();
    let p = pdf(&mut s);
    close(x_of(&p, "two") - x_of(&p, "zero"), 16.60, 0.4, "Arial 10 at 50 %, indent 2");
}

/// Guard: a cell without indent still starts at the left margin plus the cell padding
/// (only the indent rule changes).
#[test]
fn unindented_text_position_is_unchanged() {
    let mut s = session("Arial", 10.0);
    put(&mut s, "A1", "zero", 0, None);
    let full = x_of(&pdf(&mut s), "zero");
    close(full, 50.4 + 1.5, 0.05, "no indent at 100 %");
}
