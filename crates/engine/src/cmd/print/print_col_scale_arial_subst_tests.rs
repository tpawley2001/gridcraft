//! Printed column width for sans-serif Normal fonts that Excel does not have and prints in Arial:
//! Liberation Sans (metric-compatible with Arial), the generic "Sans" names ODS producers write,
//! Helvetica and its variants, Geneva, Clear Sans and Droid Sans Fallback; and Cambria, whose
//! measured page counts match Arial's width. Their columns print at Arial's digit width.
//!
//! Expected scales: dots = round(round(size x 600 / 72) x digit em), scale = dots x 0.12 / 5.25.
//! Size 10 is 83 dots an em; Arial's digit is 1139/2048 em, so 46 dots (scale 1.051429, columns
//! A:I on a Letter portrait page with 0.75 in margins); Calibri's (the fallback for unknown names)
//! is 1038/2048, so 42 dots (scale 0.96).
use serde_json::json;

use super::*;

/// A workbook whose Normal style font is `font` 10, numbers in A1:T5, Letter portrait with
/// 0.75 in left/right margins and no scaling.
fn session(font: &str) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut normal = Style::default();
    normal.font.name = font.to_string();
    normal.font.size = 10.0;
    wb.styles = gridcraft_model::StyleTable::with_default(normal);
    let rows: Vec<Vec<Json>> = (0..5).map(|r| (0..20).map(|c| json!(r * 100 + c)).collect()).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "scale": 100, "margins": {"left": 0.75, "right": 0.75}}))
        .unwrap();
    s
}

fn first_band(s: &mut Session) -> String {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    pv["pageList"][0]["columns"].as_str().unwrap().to_string()
}

const ARIAL: f32 = 46.0 * 0.12 / 5.25;
const CALIBRI: f32 = 42.0 * 0.12 / 5.25;

/// (Normal font as stored, printed column scale).
const CASES: &[(&str, f32)] = &[
    ("Liberation Sans", ARIAL),
    ("'Liberation Sans'", ARIAL), // quoted, as some ODS files store it
    ("\"Liberation Sans\"", ARIAL),
    ("Sans", ARIAL),
    ("sans-serif", ARIAL),
    ("Sans Serif", ARIAL),
    ("Helvetica", ARIAL),
    ("Helvetica Neue", ARIAL),
    ("HelveticaNeue LightExt", ARIAL),
    ("Helvetica Neue Light", ARIAL),
    ("Geneva", ARIAL),
    ("GENEVA ", ARIAL), // case and surrounding whitespace don't matter
    ("Clear Sans", ARIAL),
    ("Droid Sans Fallback", ARIAL),
    ("Cambria", ARIAL),
    // controls: names already in the table keep their values, unknown names keep Calibri's
    ("Arial", ARIAL),
    ("MS Sans Serif", ARIAL),
    ("Times New Roman", 42.0 * 0.12 / 5.25), // 83 x 1024/2048 = 41.5 -> 42 dots
    ("Foo Sans", CALIBRI),
    ("Comic Sans MS", CALIBRI),
    ("Sansation", CALIBRI), // only the exact generic names, not every name starting "sans"
];

#[test]
fn sans_fonts_excel_prints_in_arial_use_arials_digit_width() {
    for &(font, scale) in CASES {
        let s = session(font);
        let got = layout::print_col_scale(&s.doc().unwrap().wb);
        assert!((got - scale).abs() <= 1e-4, "{font:?}: scale {got}, expected {scale}");
    }
}

#[test]
fn liberation_sans_default_columns_print_a_to_i_on_the_first_page() {
    for font in ["Liberation Sans", "Geneva", "Helvetica"] {
        let mut s = session(font);
        assert_eq!(first_band(&mut s), "A:I", "{font}");
    }
}
