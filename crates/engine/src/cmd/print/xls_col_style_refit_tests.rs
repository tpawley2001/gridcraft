//! A legacy .xls row that is not custom-height ([MS-XLS] 2.4.221 Row, `fUnsynced` = 0) is re-fitted by
//! Excel when it opens the file. Besides the fonts of the row's cells, Excel counts the font of every
//! column that has a column format (COLINFO ixfe, [MS-XLS] 2.4.53) and no cell in that row: the empty cell
//! shows the column's format. Measured on Excel's re-save of a real .xls (SimSun 12 Normal, Arial 9/10
//! cells in A-H, every column formatted Arial 12, rows stored at 15 pt): Excel kept 15 pt (Arial 12's
//! default row height); Arial 10/Arial 8 column formats over Arial 10/Arial 8 cells kept 12.75/11.25 pt.
//!
//! Each case compares the page list with a control sheet whose rows are custom at the expected height,
//! and checks that the control tells the expected height from the other candidate.
use serde_json::json;

use super::*;

/// Points to 96-dpi pixels.
fn px(pt: f32) -> f32 {
    pt * 96.0 / 72.0
}

/// A column format: columns `first..=last` (0-based) formatted with `font` at `size`, hidden or not.
#[derive(Clone, Copy)]
struct ColFmt {
    first: u32,
    last: u32,
    font: &'static str,
    size: f32,
    hidden: bool,
}

const fn cols(first: u32, last: u32, font: &'static str, size: f32) -> ColFmt {
    ColFmt { first, last, font, size, hidden: false }
}

/// Rows 1..=`n` hold a value in each of `cell_cols` (0-based columns), all in `cell_font`; the Normal
/// font is Arial 10 (12.75 pt default row), the sheet default row is 12.75 pt; Letter portrait, 0.75 in
/// top/bottom margins. `col_fmts` are applied to `sheet.cols` after the cells are written. With
/// `custom_pt` every row is custom at that height (the control) and the sheet carries no .xls flags;
/// otherwise every row is stored at `stored_pt`, not custom, and the sheet carries the .xls flags.
/// Returns each printed page's `rows`.
fn page_rows(n: u32, cell_cols: &[u32], cell_font: (&str, f64), col_fmts: &[ColFmt], stored_pt: f32, custom_pt: Option<f32>) -> Vec<String> {
    let mut s = Session::new();
    s.new_workbook();
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let mut st = Style::default();
        st.font.name = "Arial".to_string();
        st.font.size = 10.0;
        wb.styles = gridcraft_model::StyleTable::with_default(st);
    }
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "margins": {"top": 0.75, "bottom": 0.75}})).unwrap();
    for &c in cell_cols {
        let letter = (b'A' + c as u8) as char;
        let range = format!("{letter}1:{letter}{n}");
        let values: Vec<Vec<u32>> = (1..=n).map(|r| vec![r]).collect();
        s.execute("range.setValues", json!({"range": range, "values": values})).unwrap();
        s.execute("home.fontName", json!({"range": range, "name": cell_font.0})).unwrap();
        s.execute("home.fontSize", json!({"range": range, "size": cell_font.1})).unwrap();
    }
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let mut ids = Vec::new();
        for f in col_fmts {
            let mut st = Style::default();
            st.font.name = f.font.to_string();
            st.font.size = f.size;
            ids.push(wb.styles.intern(st));
        }
        let sh = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        for (f, id) in col_fmts.iter().zip(ids) {
            for c in f.first..=f.last {
                let info = sh.cols.entry(c).or_default();
                info.style = Some(id);
                info.hidden = f.hidden;
            }
        }
        sh.default_row_height = px(12.75);
        sh.default_row_custom_height = None;
        for r in 0..n {
            let info = sh.rows.entry(r).or_default();
            info.size = Some(px(custom_pt.unwrap_or(stored_pt)));
            info.custom = custom_pt.is_some();
        }
        let xls = custom_pt.is_none();
        sh.fit_auto_row_heights_in_print = xls;
        sh.fit_print_rows_without_default_floor = xls;
    }
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

const N: u32 = 200;
/// Cells in A..=H, like the measured file.
const CELLS: [u32; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

/// The measured case: Arial 10 cells in A-H, every column formatted Arial 12, rows stored at 15 pt:
/// Excel keeps 15 pt (columns I onward are empty Arial 12 cells). Today we print 12.75 pt.
#[test]
fn formatted_empty_columns_size_the_row() {
    let fmts = [cols(0, 255, "Arial", 12.0)];
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(15.0));
    let cells_only = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(12.75));
    assert_eq!(got, want);
    assert_ne!(want, cells_only, "the case must tell 15 pt from 12.75 pt rows");
}

/// The column format counts even when the row is stored shorter: stored 12.75 pt, Arial 12 on column J
/// only (no cells there) -> 15 pt.
#[test]
fn one_formatted_empty_column_raises_the_row() {
    let fmts = [cols(9, 9, "Arial", 12.0)];
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 12.75, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 12.75, Some(15.0));
    let cells_only = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 12.75, Some(12.75));
    assert_eq!(got, want);
    assert_ne!(want, cells_only);
}

/// A column whose cell exists in the row counts through the cell's own font only: Arial 12 format on
/// columns A-H (all have Arial 10 cells), rows stored 15 pt -> 12.75 pt.
#[test]
fn a_column_with_a_cell_in_the_row_uses_the_cells_font() {
    let fmts = [cols(0, 7, "Arial", 12.0)];
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(12.75));
    let col_font = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(15.0));
    assert_eq!(got, want);
    assert_ne!(want, col_font);
}

/// A smaller column format does not lower the row below its cells' fonts: Arial 8 on every column,
/// Arial 10 cells, stored 15 pt -> 12.75 pt (and no floor from the format either way).
#[test]
fn a_smaller_column_format_does_not_lower_the_row() {
    let fmts = [cols(0, 255, "Arial", 8.0)];
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(12.75));
    let small = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(11.25));
    assert_eq!(got, want);
    assert_ne!(want, small);
}

/// Arial 8 cells and an Arial 8 format on every column, stored 11.25 pt: kept at 11.25 pt (measured).
#[test]
fn matching_small_column_format_keeps_the_small_row() {
    let fmts = [cols(0, 255, "Arial", 8.0)];
    let got = page_rows(N, &CELLS, ("Arial", 8.0), &fmts, 11.25, None);
    let want = page_rows(N, &CELLS, ("Arial", 8.0), &fmts, 11.25, Some(11.25));
    let normal = page_rows(N, &CELLS, ("Arial", 8.0), &fmts, 11.25, Some(12.75));
    assert_eq!(got, want);
    assert_ne!(want, normal);
}

/// A hidden formatted column does not count: Arial 12 on hidden column J, stored 15 pt -> 12.75 pt.
#[test]
fn a_hidden_formatted_column_does_not_count() {
    let fmts = [ColFmt { hidden: true, ..cols(9, 9, "Arial", 12.0) }];
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(12.75));
    let counted = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 15.0, Some(15.0));
    assert_eq!(got, want);
    assert_ne!(want, counted);
}

/// A column format in a font with no known default row height keeps the stored height (13.55 pt), as an
/// unknown cell font does.
#[test]
fn an_unknown_column_font_keeps_the_stored_height() {
    let fmts = [cols(9, 9, "No Such Font 123", 12.0)];
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 13.55, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 13.55, Some(13.55));
    let fitted = page_rows(N, &CELLS, ("Arial", 10.0), &fmts, 13.55, Some(12.75));
    assert_eq!(got, want);
    assert_ne!(want, fitted);
}

/// Without column formats the P04-060 rule is unchanged: Arial 10 cells stored at 15 pt -> 12.75 pt.
#[test]
fn no_column_formats_keeps_the_cell_rule() {
    let got = page_rows(N, &CELLS, ("Arial", 10.0), &[], 15.0, None);
    let want = page_rows(N, &CELLS, ("Arial", 10.0), &[], 15.0, Some(12.75));
    assert_eq!(got, want);
}
