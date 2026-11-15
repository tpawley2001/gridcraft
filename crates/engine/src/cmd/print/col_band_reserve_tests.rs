//! A column band leaves about two printed pixels free at its right edge: Excel starts a new band
//! with a column that would end less than ~1.6 pt short of the right margin.
//!
//! Arial 10 Normal font prints columns at 46 dots a character (scale 46 x 0.12 / 5.25 = 1.051429).
//! On Letter portrait with 0.75 in left/right margins the printable width is 504 pt. Columns A:I of
//! 64, 64, 64, 80, 71, 64, 69, 83 and 79 px sum to 638 px = 503.11 printed pt, 0.89 pt short of the
//! margin: Excel prints A:H on the first page and I on the next (measured on Excel 365's PDF).
use serde_json::json;

use super::*;

const WIDTHS: [f64; 9] = [64.0, 64.0, 64.0, 80.0, 71.0, 64.0, 69.0, 83.0, 79.0];

/// Arial 10 Normal font, values in A1:I1, the columns at `widths` px, Letter portrait with
/// 0.75 in left/right margins and no scaling.
fn session(widths: &[f64; 9]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut normal = Style::default();
    normal.font.name = "Arial".to_string();
    normal.font.size = 10.0;
    wb.styles = gridcraft_model::StyleTable::with_default(normal);
    let row: Vec<Json> = (0..9).map(|c| json!(c + 1)).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": [row]})).unwrap();
    for (i, w) in widths.iter().enumerate() {
        let col = char::from(b'A' + i as u8);
        s.execute("home.columnWidth", json!({"cols": format!("{col}:{col}"), "width": w})).unwrap();
    }
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "scale": 100, "margins": {"left": 0.75, "right": 0.75}}))
        .unwrap();
    s
}

fn preview(s: &mut Session) -> (String, u64) {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    (pv["pageList"][0]["columns"].as_str().unwrap().to_string(), pv["pages"].as_u64().unwrap())
}

#[test]
fn arial_10_band_breaks_before_column_ending_under_a_point_short() {
    let mut s = session(&WIDTHS);
    assert_eq!(preview(&mut s), ("A:H".to_string(), 2));
}

/// Control: with I at 77 px the columns end 2.47 pt short of the margin and still fit one page.
#[test]
fn arial_10_band_keeps_column_ending_2_5pt_short() {
    let mut w = WIDTHS;
    w[8] = 77.0;
    let mut s = session(&w);
    assert_eq!(preview(&mut s), ("A:I".to_string(), 1));
}
