//! A sheet read from .xlsx prints a row without `customHeight` (ECMA-376 Part 1, 18.3.1.73) at the
//! height its content fits. Excel's fit counts a superscript cell font (18.8.44 `vertAlign`) and a
//! thick bottom border: measured in Excel's own saves of real workbooks with Arial 10 as the Normal
//! font, such rows are stored at 14.25 pt (19 px, superscript) and 13.5 pt (18 px, thick bottom
//! border, `row@thickBot`) where a plain Arial 10 row is 12.75 pt (17 px).
//!
//! Each case compares the page list with a control sheet whose rows are custom at the expected
//! height, and checks that the control tells the expected height from 12.75 pt.
use serde_json::json;

use super::*;

/// Points to 96-dpi pixels.
fn px(pt: f32) -> f32 {
    pt * 96.0 / 72.0
}

/// What every cell in column A carries besides Arial 10.
#[derive(Clone, Copy)]
enum Look {
    Superscript,
    ThickBottom,
    ThinBottom,
    /// Superscript in A, a thick bottom border in B (the larger need wins, they do not add up).
    SuperscriptAndThickBottomBeside,
}

/// Rows 1..=`n` hold a value in A (and B for `SuperscriptAndThickBottomBeside`) in Arial 10 with
/// `look`; the Normal font is Arial 10; Letter portrait, 0.75 in top/bottom margins; rows stored at
/// 12.75 pt. With `custom_pt` every row is custom at that height (the control). Returns each
/// printed page's `rows`.
fn page_rows(n: u32, look: Look, custom_pt: Option<f32>) -> Vec<String> {
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
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let style = |wb: &mut gridcraft_model::Workbook, sup: bool, bottom: gridcraft_model::BorderStyle| {
            let mut st = Style::default();
            st.font.name = "Arial".to_string();
            st.font.size = 10.0;
            if sup {
                st.font.vert = gridcraft_model::VertAlign::Superscript;
            }
            st.borders.bottom.style = bottom;
            wb.styles.intern(st)
        };
        use gridcraft_model::BorderStyle as B;
        let (a, b) = match look {
            Look::Superscript => (style(wb, true, B::None), None),
            Look::ThickBottom => (style(wb, false, B::Thick), None),
            Look::ThinBottom => (style(wb, false, B::Thin), None),
            Look::SuperscriptAndThickBottomBeside => (style(wb, true, B::None), Some(style(wb, false, B::Thick))),
        };
        let sh = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        sh.default_row_height = px(12.75);
        sh.default_row_custom_height = None;
        for r in 0..n {
            sh.set_cell(CellRef::new(r, 0), gridcraft_model::Cell::styled(Value::Number(f64::from(r + 1)), a));
            if let Some(b) = b {
                sh.set_cell(CellRef::new(r, 1), gridcraft_model::Cell::styled(Value::Number(f64::from(r + 1)), b));
            }
            let info = sh.rows.entry(r).or_default();
            info.size = Some(px(custom_pt.unwrap_or(12.75)));
            info.custom = custom_pt.is_some();
            // The row flag Excel's own save writes next to `ht` for a thick bottom border.
            info.thick_bottom = matches!(look, Look::ThickBottom | Look::SuperscriptAndThickBottomBeside);
        }
        // The .xlsx reader's flags: non-custom rows are fitted, with the sheet default as the floor.
        sh.fit_auto_row_heights_in_print = custom_pt.is_none();
        sh.fit_print_rows_without_default_floor = false;
    }
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

const N: u32 = 200;

#[test]
fn a_superscript_cell_font_fits_the_row_to_14_25_pt() {
    let got = page_rows(N, Look::Superscript, None);
    let want = page_rows(N, Look::Superscript, Some(14.25));
    let plain = page_rows(N, Look::Superscript, Some(12.75));
    assert_ne!(want, plain, "the case must tell 14.25 pt from 12.75 pt rows");
    assert_eq!(got, want);
}

#[test]
fn a_thick_bottom_border_fits_the_row_to_13_5_pt() {
    let got = page_rows(N, Look::ThickBottom, None);
    let want = page_rows(N, Look::ThickBottom, Some(13.5));
    let plain = page_rows(N, Look::ThickBottom, Some(12.75));
    assert_ne!(want, plain, "the case must tell 13.5 pt from 12.75 pt rows");
    assert_eq!(got, want);
}

#[test]
fn a_thin_bottom_border_keeps_the_plain_row() {
    let got = page_rows(N, Look::ThinBottom, None);
    let want = page_rows(N, Look::ThinBottom, Some(12.75));
    assert_eq!(got, want);
}

#[test]
fn superscript_and_a_thick_border_in_one_row_take_the_larger_need() {
    let got = page_rows(N, Look::SuperscriptAndThickBottomBeside, None);
    let want = page_rows(N, Look::SuperscriptAndThickBottomBeside, Some(14.25));
    let added = page_rows(N, Look::SuperscriptAndThickBottomBeside, Some(15.0));
    assert_ne!(want, added, "the case must tell 14.25 pt from 15 pt rows");
    assert_eq!(got, want);
}
