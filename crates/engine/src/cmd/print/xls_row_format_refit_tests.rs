//! A legacy .xls row's re-fit ([MS-XLS] 2.4.221 Row, `fUnsynced` = 0) counts the format its empty cells
//! show only when a cell of the row counts: a row whose only cells sit in multi-cell merges keeps its
//! stored height, whatever the column formats. And an empty cell shows the row's own format (ROW
//! fGhostDirty + ixfe) when the row has one, the column format (COLINFO ixfe, 2.4.53) only otherwise.
//!
//! Each case compares the page list with a control sheet whose rows are custom at the expected height,
//! and checks that the control tells the expected height from the other candidate.
use serde_json::json;

use super::*;

/// Points to 96-dpi pixels.
fn px(pt: f32) -> f32 {
    pt * 96.0 / 72.0
}

/// How the test sheet is built.
#[derive(Clone, Copy)]
struct Case {
    /// Font size of the cells (Arial).
    cell_size: f64,
    /// Font size of the format on every column 0..=255 (Arial).
    col_size: f32,
    /// Font size of the row format on every row (Arial), if any.
    row_size: Option<f32>,
    /// Each row's only cells are inside the merge A:B of that row.
    merged: bool,
    stored_pt: f32,
}

/// Rows 1..=`n` hold a value in A (merged across A:B with `merged`), Arial `cell_size`; the Normal font
/// is Arial 10; Letter portrait, 0.75 in top/bottom margins. With `custom_pt` every row is custom at that
/// height (the control) and the sheet carries no .xls flags. Returns each printed page's `rows`.
fn page_rows(n: u32, k: Case, custom_pt: Option<f32>) -> Vec<String> {
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
    let range = format!("A1:A{n}");
    let values: Vec<Vec<u32>> = (1..=n).map(|r| vec![r]).collect();
    s.execute("range.setValues", json!({"range": range, "values": values})).unwrap();
    s.execute("home.fontName", json!({"range": range, "name": "Arial"})).unwrap();
    s.execute("home.fontSize", json!({"range": range, "size": k.cell_size})).unwrap();
    if k.merged {
        s.execute("home.mergeAcross", json!({"range": format!("A1:B{n}")})).unwrap();
    }
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let arial = |wb: &mut gridcraft_model::Workbook, size: f32| {
            let mut st = Style::default();
            st.font.name = "Arial".to_string();
            st.font.size = size;
            wb.styles.intern(st)
        };
        let col_id = arial(wb, k.col_size);
        let row_id = k.row_size.map(|size| arial(wb, size));
        let sh = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        for c in 0..=255 {
            sh.cols.entry(c).or_default().style = Some(col_id);
        }
        sh.default_row_height = px(12.75);
        sh.default_row_custom_height = None;
        for r in 0..n {
            let info = sh.rows.entry(r).or_default();
            info.size = Some(px(custom_pt.unwrap_or(k.stored_pt)));
            info.custom = custom_pt.is_some();
            info.style = row_id;
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

/// Arial 8 on every column, rows stored at 15 pt whose only cell is inside a 2-cell merge: no cell
/// counts, so the row keeps 15 pt and the column format does not shrink it to 11.25 pt.
#[test]
fn a_row_of_only_merged_cells_keeps_its_stored_height() {
    let k = Case { cell_size: 10.0, col_size: 8.0, row_size: None, merged: true, stored_pt: 15.0 };
    let got = page_rows(N, k, None);
    let want = page_rows(N, k, Some(15.0));
    let shrunk = page_rows(N, k, Some(11.25));
    assert_eq!(got, want);
    assert_ne!(want, shrunk, "the case must tell 15 pt from 11.25 pt rows");
}

/// Arial 10 on every column, a row format Arial 8 holding one Arial 8 cell, stored 11.25 pt: the empty
/// cells show the row format, so the row stays at 11.25 pt, not 12.75 pt.
#[test]
fn a_row_format_replaces_the_column_formats() {
    let k = Case { cell_size: 8.0, col_size: 10.0, row_size: Some(8.0), merged: false, stored_pt: 11.25 };
    let got = page_rows(N, k, None);
    let want = page_rows(N, k, Some(11.25));
    let by_cols = page_rows(N, k, Some(12.75));
    assert_eq!(got, want);
    assert_ne!(want, by_cols, "the case must tell 11.25 pt from 12.75 pt rows");
}

/// Without a row format the column formats still count: Arial 10 cells, Arial 12 on every column,
/// stored 15 pt -> 15 pt.
#[test]
fn without_a_row_format_the_column_formats_count() {
    let k = Case { cell_size: 10.0, col_size: 12.0, row_size: None, merged: false, stored_pt: 15.0 };
    let got = page_rows(N, k, None);
    let want = page_rows(N, k, Some(15.0));
    let cells_only = page_rows(N, k, Some(12.75));
    assert_eq!(got, want);
    assert_ne!(want, cells_only);
}
