//! A legacy .xls row that is not custom-height ([MS-XLS] 2.4.221 Row, `fUnsynced` = 0) prints at the
//! default row height of the largest font in it, with no floor at the Normal font's default: Excel re-fits
//! such rows when it opens the file. Measured on Excel's own re-saves of .xls files: Arial 10 rows stored
//! at 13.55 pt and 17.65 pt became 12.75 pt (Arial 10's default row); Arial 8 rows stored at 11.25 pt kept
//! 11.25 pt under an MS Sans Serif 10 Normal font (12.75 pt default); Arial 10 rows kept 12.75 pt under
//! Calibri 11 (15 pt). Rows with no cells keep the stored / sheet default height.
//!
//! Each case compares the page list with a control sheet whose rows are custom at the expected height,
//! and checks that the stored height would have printed differently where it differs.
use serde_json::json;

use super::*;

/// Points to 96-dpi pixels.
fn px(pt: f32) -> f32 {
    pt * 96.0 / 72.0
}

/// How the rows of the test sheet are set up.
#[derive(Clone, Copy)]
struct Rows {
    /// Stored height in points (`None` = no stored height).
    stored_pt: Option<f32>,
    /// The row is custom-height (`fUnsynced` = 1).
    custom: bool,
    /// The sheet carries the .xls flags (`fit_auto_row_heights_in_print` + `fit_print_rows_without_default_floor`).
    xls: bool,
}

/// A workbook with Normal font `normal` at `normal_size`, Letter portrait, 0.75 in top/bottom margins,
/// the sheet default row height `default_pt`, values in column A of rows `filled` (1-based) with the
/// fonts `fonts` cycled over them (`(name, size)`), then extra cells from `extra` (`(a1, font, size)`),
/// rows 1..=`n` given `rows`. Returns each printed page's `rows`.
#[allow(clippy::too_many_arguments)]
fn page_rows(
    normal: (&str, f32),
    default_pt: f32,
    n: u32,
    filled: &[u32],
    fonts: &[(&str, f64)],
    extra: &[(&str, &str, f64)],
    rows: Rows,
    wrap: bool,
) -> Vec<String> {
    let mut s = Session::new();
    s.new_workbook();
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let mut st = Style::default();
        st.font.name = normal.0.to_string();
        st.font.size = normal.1;
        wb.styles = gridcraft_model::StyleTable::with_default(st);
    }
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "margins": {"top": 0.75, "bottom": 0.75}})).unwrap();
    for (i, &r) in filled.iter().enumerate() {
        let a1 = format!("A{r}");
        s.execute("range.setValues", json!({"range": a1, "values": [[r]]})).unwrap();
        let (font, size) = fonts[i % fonts.len()];
        s.execute("home.fontName", json!({"range": a1, "name": font})).unwrap();
        s.execute("home.fontSize", json!({"range": a1, "size": size})).unwrap();
        if wrap {
            s.execute("home.wrapText", json!({"range": a1, "on": true})).unwrap();
        }
    }
    for &(a1, font, size) in extra {
        s.execute("range.setValues", json!({"range": a1, "values": [["x"]]})).unwrap();
        s.execute("home.fontName", json!({"range": a1, "name": font})).unwrap();
        s.execute("home.fontSize", json!({"range": a1, "size": size})).unwrap();
    }
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let sh = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        sh.default_row_height = px(default_pt);
        sh.default_row_custom_height = None;
        for r in 0..n {
            let info = sh.rows.entry(r).or_default();
            info.size = rows.stored_pt.map(px);
            info.custom = rows.custom;
        }
        sh.fit_auto_row_heights_in_print = rows.xls;
        sh.fit_print_rows_without_default_floor = rows.xls;
    }
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

fn all(n: u32) -> Vec<u32> {
    (1..=n).collect()
}

const XLS: Rows = Rows { stored_pt: None, custom: false, xls: true };

fn custom(pt: f32) -> Rows {
    Rows { stored_pt: Some(pt), custom: true, xls: false }
}

fn stored(pt: f32) -> Rows {
    Rows { stored_pt: Some(pt), custom: false, xls: true }
}

/// Arial 10 Normal, Arial 10 cells, rows stored at 13.55 pt not custom: Excel prints them at 12.75 pt.
#[test]
fn arial_rows_stored_taller_print_at_arials_default_row() {
    let n = 160;
    let got = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], stored(13.55), false);
    let want = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], custom(12.75), false);
    let old = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], custom(13.55), false);
    assert_eq!(got, want);
    assert_ne!(want, old, "the case must tell 12.75 pt from 13.55 pt rows");
}

/// Mixed Arial 10 / Arial 8 / Tahoma 8 cells in one row, stored 17.65 pt not custom: 12.75 pt (the largest).
#[test]
fn a_row_fits_its_largest_cell_font() {
    let n = 160;
    let extra: Vec<(String, &str, f64)> = (1..=n).flat_map(|r| [(format!("B{r}"), "Arial", 8.0), (format!("C{r}"), "Tahoma", 8.0)]).collect();
    let extra: Vec<(&str, &str, f64)> = extra.iter().map(|(a, f, s)| (a.as_str(), *f, *s)).collect();
    let got = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &extra, stored(17.65), false);
    let want = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &extra, custom(12.75), false);
    let old = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &extra, custom(17.65), false);
    assert_eq!(got, want);
    assert_ne!(want, old);
}

/// MS Sans Serif 10 Normal (12.75 pt default), Arial 8 cells stored 11.25 pt: Excel keeps 11.25 pt (no floor).
#[test]
fn a_smaller_cell_font_is_not_raised_to_the_normal_default() {
    let n = 200;
    let got = page_rows(("MS Sans Serif", 10.0), 12.75, n, &all(n), &[("Arial", 8.0)], &[], stored(11.25), false);
    let want = page_rows(("MS Sans Serif", 10.0), 12.75, n, &all(n), &[("Arial", 8.0)], &[], custom(11.25), false);
    let floor = page_rows(("MS Sans Serif", 10.0), 12.75, n, &all(n), &[("Arial", 8.0)], &[], custom(12.75), false);
    assert_eq!(got, want);
    assert_ne!(want, floor, "the case must tell 11.25 pt from 12.75 pt rows");
}

/// Calibri 11 Normal (15 pt default), Arial 10 cells stored 12.75 pt: kept at 12.75 pt.
#[test]
fn arial_rows_under_calibri_keep_arials_height() {
    let n = 200;
    let got = page_rows(("Calibri", 11.0), 15.0, n, &all(n), &[("Arial", 10.0)], &[], stored(12.75), false);
    let want = page_rows(("Calibri", 11.0), 15.0, n, &all(n), &[("Arial", 10.0)], &[], custom(12.75), false);
    let floor = page_rows(("Calibri", 11.0), 15.0, n, &all(n), &[("Arial", 10.0)], &[], custom(15.0), false);
    assert_eq!(got, want);
    assert_ne!(want, floor);
}

/// The same rows with no stored height also fit their cell font (12.75 pt), not Calibri's 15 pt.
#[test]
fn rows_with_no_stored_height_fit_their_cell_font() {
    let n = 200;
    let got = page_rows(("Calibri", 11.0), 15.0, n, &all(n), &[("Arial", 10.0)], &[], XLS, false);
    let want = page_rows(("Calibri", 11.0), 15.0, n, &all(n), &[("Arial", 10.0)], &[], custom(12.75), false);
    assert_eq!(got, want);
}

/// Custom (fUnsynced) rows keep their stored height even on an .xls sheet.
#[test]
fn custom_rows_keep_their_height() {
    let n = 160;
    let xls_custom = Rows { stored_pt: Some(13.55), custom: true, xls: true };
    let got = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], xls_custom, false);
    let want = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], custom(13.55), false);
    assert_eq!(got, want);
}

/// Rows with no cells print at the sheet's default row height (DEFAULTROWHEIGHT), not the Normal
/// font's pitch: Arial 10 Normal (12.75 pt) with a 15 pt sheet default, values (in Calibri 11, whose
/// default row is also 15 pt) only in A1 and A300. A 32,770-row .xls printed 886 pages instead of
/// Excel's 745 when empty rows took the Normal pitch.
#[test]
fn rows_without_cells_keep_the_sheet_default_height() {
    let n = 300;
    let got = page_rows(("Arial", 10.0), 15.0, 0, &[1, n], &[("Calibri", 11.0)], &[], XLS, false);
    let plain = Rows { stored_pt: None, custom: false, xls: false };
    let want = page_rows(("Arial", 10.0), 15.0, 0, &[1, n], &[("Calibri", 11.0)], &[], plain, false);
    let pitch = page_rows(("Arial", 10.0), 12.75, 0, &[1, n], &[("Calibri", 11.0)], &[], plain, false);
    assert_eq!(got, want);
    assert_ne!(want, pitch, "the case must tell 15 pt from 12.75 pt empty rows");
}

/// Wrapped text keeps the stored height (13.55 pt), as on .xlsx sheets.
#[test]
fn wrapped_rows_keep_their_stored_height() {
    let n = 160;
    let got = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], stored(13.55), true);
    let want = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], custom(13.55), true);
    assert_eq!(got, want);
}

/// A cell font with no known default row height keeps the stored height.
#[test]
fn an_unknown_cell_font_keeps_the_stored_height() {
    let n = 160;
    let got = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("No Such Font 123", 10.0)], &[], stored(13.55), false);
    let want = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("No Such Font 123", 10.0)], &[], custom(13.55), false);
    assert_eq!(got, want);
}

/// Without the .xls flags (an .xls sheet with no ROW records) the stored height prints, as today.
#[test]
fn sheets_without_the_flags_print_the_stored_height() {
    let n = 160;
    let plain = Rows { stored_pt: Some(13.55), custom: false, xls: false };
    let got = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], plain, false);
    let want = page_rows(("Arial", 10.0), 12.75, n, &all(n), &[("Arial", 10.0)], &[], custom(13.55), false);
    assert_eq!(got, want);
}
