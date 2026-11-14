//! Normal fonts outside the first measured set: their printed row pitch (P, D), and the localized
//! names East Asian workbooks store for them. Every number here is Excel's own: one workbook per
//! Normal font, 600 plain rows, Letter portrait, top/bottom margins 0.75 in (5700 dots, 5692 for
//! rows), printed at 100 %, 70 % and 45 %; the rows on page 1 are what Excel printed. P (the
//! font's default row height in screen pixels) is the row height Excel wrote when it saved the
//! same workbook.
use serde_json::json;

use super::*;

/// A workbook whose Normal style font is `font` at `size`, Letter portrait, 0.75 in top/bottom.
fn session(font: &str, size: f32) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let doc = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut doc.wb);
    let mut normal = Style::default();
    normal.font.name = font.to_string();
    normal.font.size = size;
    wb.styles = gridcraft_model::StyleTable::with_default(normal);
    s.execute("pageLayout.pageSetup", json!({"paper": "Letter", "orientation": "portrait", "margins": {"top": 0.75, "bottom": 0.75}})).unwrap();
    s
}

/// Values in A1:A<n>, every row set to `height_pt`, at print `scale` %; the `rows` of each printed page.
fn page_rows(s: &mut Session, n: u32, height_pt: f64, scale: u32) -> Vec<String> {
    s.execute("pageLayout.pageSetup", json!({"scale": scale})).unwrap();
    let rows: Vec<Vec<Json>> = (1..=n).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("home.rowHeight", json!({"rows": format!("1:{n}"), "points": height_pt})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

/// (Normal font as stored, size, P = default row height in px, Excel's rows on page 1 at 100 %, 70 %, 45 %).
/// Rows are set to the font's own default height (P x 0.75 pt), so each prints D dots at 100 %.
const MEASURED: &[(&str, f32, u32, [u32; 3])] = &[
    ("MS PGothic", 11.0, 18, [55, 80, 123]),      // control: already measured (D 102)
    ("ＭＳ Ｐゴシック", 11.0, 18, [55, 80, 123]), // the Japanese name of MS PGothic
    ("宋体", 11.0, 18, [50, 72, 111]),            // SimSun 11: D 113
    ("宋体", 12.0, 19, [46, 66, 103]),            // SimSun 12 (already measured: D 122)
    ("游ゴシック", 11.0, 25, [36, 52, 82]),       // Yu Gothic 11: D 154
    ("Yu Gothic", 11.0, 25, [36, 52, 82]),
    ("游ゴシック", 10.0, 22, [40, 58, 90]),     // Yu Gothic 10: D 139 or 140
    ("等线", 11.0, 19, [54, 77, 121]),          // DengXian 11: D 104
    ("新細明體", 12.0, 20, [51, 73, 113]),      // PMingLiU 12: D 110
    ("新細明體", 11.0, 19, [55, 80, 123]),      // PMingLiU 11: D 102
    ("Trebuchet MS", 11.0, 22, [45, 65, 101]),  // D 124
    ("Garamond", 11.0, 20, [51, 72, 113]),      // D 111
    ("Century Gothic", 9.0, 19, [57, 82, 126]), // D 99
    ("Arial Cyr", 10.0, 17, [55, 79, 123]),     // D 103 (Arial 10's pair)
    ("Calibri", 14.0, 25, [37, 54, 83]),        // D 150
];

#[test]
fn rows_per_page_follow_each_measured_normal_font() {
    for &(font, size, p, per_page) in MEASURED {
        for (&scale, &n) in [100u32, 70, 45].iter().zip(per_page.iter()) {
            let mut s = session(font, size);
            let rows = page_rows(&mut s, 2 * n, f64::from(p) * 0.75, scale);
            let what = format!("{font} {size} at {scale} %");
            assert_eq!(rows.len(), 2, "{what}: pages {rows:?}");
            assert_eq!(rows[0], format!("1:{n}"), "{what}: page 1");
            assert_eq!(rows[1], format!("{}:{}", n + 1, 2 * n), "{what}: page 2");
        }
    }
}

/// The localized names print exactly like the English ones, at heights other than the default too.
#[test]
fn localized_font_names_print_like_their_english_names() {
    for (local, english, size) in [
        ("ＭＳ Ｐゴシック", "MS PGothic", 11.0),
        ("游ゴシック", "Yu Gothic", 11.0),
        ("宋体", "SimSun", 12.0),
        ("等线", "DengXian", 11.0),
        ("新細明體", "PMingLiU", 12.0),
    ] {
        for pt in [12.0, 15.0, 21.0, 30.0] {
            let a = page_rows(&mut session(local, size), 150, pt, 100);
            let b = page_rows(&mut session(english, size), 150, pt, 100);
            assert_eq!(a, b, "{local} vs {english} {size}, {pt} pt rows");
        }
    }
}

/// A font still outside every table prints with Calibri 11's pair, as before.
#[test]
fn an_unmeasured_normal_font_keeps_calibri_11s_pitch() {
    let a = page_rows(&mut session("Some Unknown Font", 11.0), 94, 15.0, 100);
    assert_eq!(a, vec!["1:47".to_string(), "48:94".to_string()]);
}
