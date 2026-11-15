//! Printed column width for Normal fonts that Excel prints in Arial. Workbooks from older Excel
//! versions often have MS Sans Serif 10 (a bitmap screen font) as the Normal font; Excel prints
//! their cells in Arial, and their columns at Arial's printed digit width. Measured from an
//! Excel 365 PDF of such a workbook (no stored column widths, Letter portrait, 0.75 in
//! left/right margins): a default column prints 50.52 pt wide, so columns A:I fit on the first
//! page and J starts the next band.
//!
//! Expected scales: dots = round(round(size x 600 / 72) x digit em), scale = dots x 0.12 / 5.25
//! (printed points over the 7 screen pixels a character is stored as). Size 10 is 83 dots an em;
//! Arial's digit is 1139/2048 em, so 46 dots; Calibri's (the fallback for unknown names) is
//! 1038/2048, so 42 dots.
use serde_json::json;

use super::*;

/// A workbook whose Normal style font is `font` 10, numbers in A1:T5, Letter portrait with
/// 0.75 in left/right margins and no scaling.
fn session(font: &str) -> Session {
    session_sized(font, 10.0)
}

fn session_sized(font: &str, size: f32) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut normal = Style::default();
    normal.font.name = font.to_string();
    normal.font.size = size;
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

/// (Normal font as stored, printed column scale, first page's columns).
const CASES: &[(&str, f32, &str)] = &[
    ("MS Sans Serif", 46.0 * 0.12 / 5.25, "A:I"), // 1.05143: measured in Excel's PDF
    ("ms sans serif", 46.0 * 0.12 / 5.25, "A:I"), // names match case-insensitively
    ("Arial", 46.0 * 0.12 / 5.25, "A:I"),         // control: Arial itself
    ("Arial Cyr", 46.0 * 0.12 / 5.25, "A:I"),     // charset variants of Arial: Arial's widths
    ("Arial Tur", 46.0 * 0.12 / 5.25, "A:I"),
    ("Arial CE", 46.0 * 0.12 / 5.25, "A:I"),
    ("Arial Baltic", 46.0 * 0.12 / 5.25, "A:I"),
    ("Arial Greek", 46.0 * 0.12 / 5.25, "A:I"),
];

#[test]
fn fonts_excel_prints_in_arial_use_arials_digit_width() {
    for &(font, scale, band) in CASES {
        let mut s = session(font);
        let got = layout::print_col_scale(&s.doc().unwrap().wb);
        assert!((got - scale).abs() <= 1e-4, "{font}: scale {got}, expected {scale}");
        assert_eq!(first_band(&mut s), band, "{font}");
    }
}

/// East Asian Normal fonts whose digits are half-width (half an em): SimSun and MS (P)Gothic, under
/// their English and their stored local names. At 12 pt an em is 100 dots, so 50 dots a digit
/// (scale 1.142857); the Calibri fallback would give 51 (1.165714). Measured on the frozen Excel
/// set: with 50 dots, SimSun 12 forms whose columns A:O just fit one page print on one page wide
/// as in Excel's PDFs; at 51 they spill a column onto extra pages.
const HALF_WIDTH: &[&str] = &["SimSun", "宋体", "NSimSun", "新宋体", "MS PGothic", "ＭＳ Ｐゴシック", "MS Gothic", "ＭＳ ゴシック"];

#[test]
fn half_width_east_asian_fonts_use_half_an_em() {
    for font in HALF_WIDTH {
        let s = session_sized(font, 12.0);
        let got = layout::print_col_scale(&s.doc().unwrap().wb);
        assert!((got - 50.0 * 0.12 / 5.25).abs() <= 1e-4, "{font}: scale {got}");
    }
}

/// An unknown name keeps the Calibri fallback: 42 dots, scale 0.96.
#[test]
fn unknown_font_keeps_the_calibri_fallback() {
    let s = session("Foo Sans");
    let got = layout::print_col_scale(&s.doc().unwrap().wb);
    assert!((got - 42.0 * 0.12 / 5.25).abs() <= 1e-4, "{got}");
}
