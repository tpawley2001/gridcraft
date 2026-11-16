//! A sheet read from .xlsx prints a row without `customHeight` (ECMA-376 Part 1, 18.3.1.73) at the
//! height its content fits. Excel's fit counts a thick TOP border like a thick bottom one (1 px), and
//! the two add up: measured in Excel's own save of a real workbook whose Normal font is Trebuchet MS
//! 11 (default row 22 px, 16.5 pt), rows are stored at 17.25 pt (23 px) with a thick top or a thick
//! bottom border and at 18 pt (24 px) with both.
//!
//! Each case compares the page list with a control sheet whose rows are custom at the expected
//! height, and checks that the control tells the expected height from its neighbours.
use serde_json::json;

use super::*;

/// Points to 96-dpi pixels.
fn px(pt: f32) -> f32 {
    pt * 96.0 / 72.0
}

/// The borders the cells of each row carry (all cells Trebuchet MS 11).
#[derive(Clone, Copy)]
enum Look {
    /// A thick top border on A.
    ThickTop,
    /// A thin top border on A.
    ThinTop,
    /// Thick top and thick bottom borders on A.
    ThickTopAndBottom,
    /// A thick top border on A, a thick bottom border on B.
    ThickTopABottomB,
    /// Thick top borders on A and B (counted once).
    ThickTopTwice,
}

/// Rows 1..=`n` hold values in A (and B where `look` names it) in Trebuchet MS 11 with `look`'s
/// borders; the Normal font is Trebuchet MS 11; Letter portrait, 0.75 in top/bottom margins; rows
/// stored at 16.5 pt. With `custom_pt` every row is custom at that height (the control). Returns each
/// printed page's `rows`.
fn page_rows(n: u32, look: Look, custom_pt: Option<f32>) -> Vec<String> {
    let mut s = Session::new();
    s.new_workbook();
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let mut st = Style::default();
        st.font.name = "Trebuchet MS".to_string();
        st.font.size = 11.0;
        wb.styles = gridcraft_model::StyleTable::with_default(st);
    }
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "margins": {"top": 0.75, "bottom": 0.75}})).unwrap();
    {
        let doc = s.doc_mut().unwrap();
        let wb = std::sync::Arc::make_mut(&mut doc.wb);
        let style = |wb: &mut gridcraft_model::Workbook, top: gridcraft_model::BorderStyle, bottom: gridcraft_model::BorderStyle| {
            let mut st = Style::default();
            st.font.name = "Trebuchet MS".to_string();
            st.font.size = 11.0;
            st.borders.top.style = top;
            st.borders.bottom.style = bottom;
            wb.styles.intern(st)
        };
        use gridcraft_model::BorderStyle as B;
        let (a, b) = match look {
            Look::ThickTop => (style(wb, B::Thick, B::None), None),
            Look::ThinTop => (style(wb, B::Thin, B::None), None),
            Look::ThickTopAndBottom => (style(wb, B::Thick, B::Thick), None),
            Look::ThickTopABottomB => (style(wb, B::Thick, B::None), Some(style(wb, B::None, B::Thick))),
            Look::ThickTopTwice => {
                let t = style(wb, B::Thick, B::None);
                (t, Some(t))
            }
        };
        let sh = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        sh.default_row_height = px(16.5);
        sh.default_row_custom_height = None;
        for r in 0..n {
            sh.set_cell(CellRef::new(r, 0), gridcraft_model::Cell::styled(Value::Number(f64::from(r + 1)), a));
            if let Some(b) = b {
                sh.set_cell(CellRef::new(r, 1), gridcraft_model::Cell::styled(Value::Number(f64::from(r + 1)), b));
            }
            let info = sh.rows.entry(r).or_default();
            info.size = Some(px(custom_pt.unwrap_or(16.5)));
            info.custom = custom_pt.is_some();
            // The row flags Excel's own save writes next to `ht` for these borders.
            info.thick_top = !matches!(look, Look::ThinTop);
            info.thick_bottom = matches!(look, Look::ThickTopAndBottom | Look::ThickTopABottomB);
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
fn a_thick_top_border_fits_the_row_to_17_25_pt() {
    let got = page_rows(N, Look::ThickTop, None);
    let want = page_rows(N, Look::ThickTop, Some(17.25));
    let plain = page_rows(N, Look::ThickTop, Some(16.5));
    assert_ne!(want, plain, "the case must tell 17.25 pt from 16.5 pt rows");
    assert_eq!(got, want);
}

#[test]
fn a_thin_top_border_keeps_the_plain_row() {
    let got = page_rows(N, Look::ThinTop, None);
    let want = page_rows(N, Look::ThinTop, Some(16.5));
    assert_eq!(got, want);
}

#[test]
fn thick_top_and_bottom_on_one_cell_add_up_to_18_pt() {
    let got = page_rows(N, Look::ThickTopAndBottom, None);
    let want = page_rows(N, Look::ThickTopAndBottom, Some(18.0));
    let one = page_rows(N, Look::ThickTopAndBottom, Some(17.25));
    assert_ne!(want, one, "the case must tell 18 pt from 17.25 pt rows");
    assert_eq!(got, want);
}

#[test]
fn thick_top_and_bottom_on_different_cells_add_up_to_18_pt() {
    let got = page_rows(N, Look::ThickTopABottomB, None);
    let want = page_rows(N, Look::ThickTopABottomB, Some(18.0));
    let one = page_rows(N, Look::ThickTopABottomB, Some(17.25));
    assert_ne!(want, one, "the case must tell 18 pt from 17.25 pt rows");
    assert_eq!(got, want);
}

#[test]
fn two_thick_top_borders_count_once() {
    let got = page_rows(N, Look::ThickTopTwice, None);
    let want = page_rows(N, Look::ThickTopTwice, Some(17.25));
    let twice = page_rows(N, Look::ThickTopTwice, Some(18.0));
    assert_ne!(want, twice, "the case must tell 17.25 pt from 18 pt rows");
    assert_eq!(got, want);
}
