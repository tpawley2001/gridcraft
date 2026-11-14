//! Rows print at Excel's printed height for the workbook's Normal font, not at their point height.
//! Excel places rows on the printer's 600-dpi grid: a row of `px` screen pixels (points x 96 / 72)
//! prints `floor((8 x px x D + P) / (8 x P))` dots, where P is the Normal font's default row height in
//! screen pixels and D the printed height of that default row in dots; a zoomed sheet rounds
//! `dots x scale / 100` half up. Every number here is measured from Excel's own PDFs of
//! workbooks built for the purpose (54 set heights for each of nine Normal fonts, nine scales).
//! Letter portrait, top/bottom margins 0.75 in = 5700 dots of printable height.
use serde_json::json;

use super::metrics::printed_row_dots;
use super::*;

/// A workbook whose Normal style font is `font` at `size`.
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

/// Values in A1:A<n>, every row set to `height_pt`; returns the `rows` of each printed page.
fn page_rows(s: &mut Session, n: u32, height_pt: f64) -> Vec<String> {
    let rows: Vec<Vec<Json>> = (1..=n).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("home.rowHeight", json!({"rows": format!("1:{n}"), "points": height_pt})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

/// (Normal font, size, row height in points, rows in the sheet, Excel's pages, Excel's rows on a full page).
/// The row counts are chosen so that the page count is wrong at the point height wherever the
/// rows on a page differ (2 x Excel's rows where Excel fits more, Excel's rows + 1 where fewer).
const PAGES: &[(&str, f32, f64, u32, usize, u32)] = &[
    ("Calibri", 11.0, 15.0, 94, 2, 47),           // 121 dots; at 15 pt: 45 a page, 3 pages
    ("Calibri", 11.0, 12.0, 118, 2, 59),          // 96 dots; at 12 pt: 57
    ("Calibri", 11.0, 13.5, 104, 2, 52),          // 109 dots, the first height with the extra dot; 50
    ("Calibri", 11.0, 20.0, 70, 2, 35),           // 161 dots (not a whole pixel: 26.67 px); 34
    ("Arial", 10.0, 12.75, 110, 2, 55),           // 103 dots; 53
    ("Times New Roman", 10.0, 12.75, 106, 2, 53), // 106 dots; 53 as well (guard)
    ("Times New Roman", 12.0, 15.75, 90, 2, 45),  // 126 dots; 43
    ("Arial", 8.0, 11.25, 132, 2, 66),            // 85 dots: 67 x 85 = 5695 does NOT fit 5700; 60
    ("Arial", 12.0, 15.0, 92, 2, 46),             // 123 dots; 45
    ("Tahoma", 10.0, 12.75, 53, 2, 52),           // 108 dots; 53 at 12.75 pt (one page)
    ("Courier New", 10.0, 13.5, 118, 2, 59),      // 96 dots; 50
    ("Aptos Narrow", 11.0, 15.0, 45, 2, 44),      // 127 dots; 45 at 15 pt (one page)
];

#[test]
fn rows_per_page_follow_the_normal_fonts_printed_row_height() {
    for &(font, size, pt, n, pages, per_page) in PAGES {
        let mut s = session(font, size);
        let rows = page_rows(&mut s, n, pt);
        let what = format!("{font} {size}, {pt} pt rows, {n} rows");
        assert_eq!(rows.len(), pages, "{what}: pages {rows:?}");
        assert_eq!(rows[0], format!("1:{per_page}"), "{what}: page 1");
        assert_eq!(rows[1], format!("{}:{n}", per_page + 1), "{what}: page 2");
    }
}

/// A zoomed sheet: each row prints round-half-up(dots x scale / 100) dots. Calibri 11, 15 pt rows
/// (121 dots at 100 %), 200 rows: Excel's rows per page and pages at each scale.
#[test]
fn a_scaled_sheet_rounds_each_rows_dots_half_up() {
    // (scale %, dots per row, rows per page, pages of 200 rows)
    for (scale, per_page, pages) in [(50, 93u32, 3usize), (70, 66, 4), (75, 62, 4), (85, 55, 4), (90, 52, 4), (115, 40, 5), (150, 31, 7)] {
        let mut s = session("Calibri", 11.0);
        s.execute("pageLayout.pageSetup", json!({"scale": scale})).unwrap();
        let rows = page_rows(&mut s, 200, 15.0);
        assert_eq!(rows.len(), pages, "scale {scale}: pages {rows:?}");
        assert_eq!(rows[0], format!("1:{per_page}"), "scale {scale}: page 1");
    }
}

/// A Normal font (name, size) and Excel's printed dots for row heights in points.
type FontDots = (&'static str, f32, &'static [(f64, u32)]);

/// Excel's printed dots for a row height under each Normal font (whole-pixel heights only).
#[test]
fn printed_row_dots_match_excel() {
    let table: &[FontDots] = &[
        ("Calibri", 11.0, &[(9.75, 78), (12.0, 96), (12.75, 102), (13.5, 109), (15.0, 121), (28.5, 230), (30.0, 242), (45.0, 363)]),
        ("Arial", 10.0, &[(10.5, 84), (11.25, 91), (12.0, 97), (12.75, 103), (15.0, 121), (24.0, 194), (30.0, 242), (45.0, 363)]),
        ("Times New Roman", 10.0, &[(6.0, 50), (9.75, 81), (12.75, 106), (15.0, 124), (30.0, 249)]),
        ("Times New Roman", 12.0, &[(6.0, 48), (12.0, 96), (15.75, 126), (30.0, 240), (45.75, 366)]),
        ("Arial", 8.0, &[(6.0, 45), (6.75, 51), (11.25, 85), (15.0, 113), (45.0, 340)]),
        ("Arial", 12.0, &[(6.0, 49), (9.75, 80), (15.0, 123), (30.0, 246), (45.0, 369)]),
        ("Tahoma", 10.0, &[(6.0, 50), (6.75, 57), (12.75, 108), (15.0, 127), (45.0, 381)]),
        ("Courier New", 10.0, &[(6.0, 42), (6.75, 48), (9.0, 64), (13.5, 96), (15.0, 106), (45.0, 320)]),
        ("Aptos Narrow", 11.0, &[(9.75, 82), (12.0, 101), (15.0, 127), (30.0, 254), (45.0, 381)]),
    ];
    for &(font, size, heights) in table {
        for &(pt, dots) in heights {
            assert_eq!(printed_row_dots(pt, font, size), dots, "{font} {size}, {pt} pt");
        }
    }
    // A font outside the table prints with Calibri 11's pair; font names compare without case.
    assert_eq!(printed_row_dots(15.0, "No Such Font", 11.0), 121);
    assert_eq!(printed_row_dots(12.75, "arial", 10.0), 103);
    // A hidden (zero-height) row takes no dots.
    assert_eq!(printed_row_dots(0.0, "Calibri", 11.0), 0);
}

/// Calibri 11, A1:A94 numbered, saved as .xlsx with `edit` applied to the sheet's XML, reopened;
/// returns the `rows` of each printed page.
fn reopened_page_rows(edit: impl Fn(&str) -> String) -> Vec<String> {
    use std::io::{Read, Write};
    let mut s = session("Calibri", 11.0);
    let rows: Vec<Vec<Json>> = (1..=94).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    let r = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap();
    let bytes = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "xl/worksheets/sheet1.xml" {
            data = edit(std::str::from_utf8(&data).unwrap()).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    let edited = out.finish().unwrap().into_inner();
    s.execute("file.open", json!({"name": "rows.xlsx", "base64": crate::io::base64_encode(&edited)})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

/// `xml` with `attrs` added to every `<row` element.
fn with_row_attrs(xml: &str, attrs: &str) -> String {
    assert!(xml.contains("<row "), "the saved sheet has <row> elements: {xml}");
    xml.replace("<row ", &format!("<row {attrs} "))
}

/// .xlsx: a row height stored without `customHeight` (ECMA-376 Part 1, 18.3.1.73 row@ht,
/// @customHeight) is not used for printing; Excel prints such a row at the height that fits its
/// content, the Normal font's default (measured 2026-11-13, Calibri 11 probe: rows stored at
/// 30 pt without customHeight print 47 a page, like rows with no height; with customHeight="1",
/// 23 a page: 30 pt = 242 dots).
#[test]
fn an_xlsx_row_height_without_custom_height_prints_at_the_fitted_height() {
    let plain = reopened_page_rows(|xml| xml.to_string());
    assert_eq!(plain, ["1:47", "48:94"], "plain Calibri 11 rows: 47 a page");
    let stored = reopened_page_rows(|xml| with_row_attrs(xml, r#"ht="30""#));
    assert_eq!(stored, plain, "a 30 pt height without customHeight prints like no height");
    let custom = reopened_page_rows(|xml| with_row_attrs(xml, r#"ht="30" customHeight="1""#));
    assert_eq!(custom.len(), 5, "custom 30 pt rows: 23 a page, 5 pages of 94: {custom:?}");
    assert_eq!(custom[0], "1:23");
}
