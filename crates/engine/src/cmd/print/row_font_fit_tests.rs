//! A row read from .xlsx that is not custom-height (ECMA-376 Part 1, 18.3.1.73 `row@customHeight`)
//! prints at the default row height of the largest font in it, whether or not it stores an `ht`,
//! and never below the sheet's default. Measured from Excel's own PDFs (Letter portrait, top and
//! bottom margins 0.75 in): a Calibri 11 sheet with column A in Calibri 20 prints 26 rows a page
//! (35 px rows = 211 dots under Calibri 11's pitch); with `customHeight="1"` the stored height wins.
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

/// `xml` with every `ht="…"` and `customHeight="…"` attribute removed from its `<row>` elements,
/// then `attrs` (may be empty) added to each.
fn rows_with(xml: &str, attrs: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(i) = rest.find("<row ") {
        out.push_str(&rest[..i]);
        let end = i + rest[i..].find('>').expect("a <row> start tag ends");
        let mut tag = rest[i..end].to_string();
        for name in [" ht=\"", " customHeight=\""] {
            while let Some(a) = tag.find(name) {
                let close = a + name.len() + tag[a + name.len()..].find('"').unwrap();
                tag.replace_range(a..=close, "");
            }
        }
        if attrs.is_empty() {
            out.push_str(&tag);
        } else {
            out.push_str(&format!("<row {attrs} {}", &tag["<row ".len()..]));
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Normal font `normal` at `normal_size`; A1:A`n` numbered, in `cell_font` at `cell_size`; saved
/// as .xlsx, every `<row>` given exactly `row_attrs`, reopened. Returns the `rows` of each printed page.
fn page_rows(normal: &str, normal_size: f32, cell_font: &str, cell_size: f64, n: u32, row_attrs: &str) -> Vec<String> {
    use std::io::{Read, Write};
    let mut s = session(normal, normal_size);
    let rows: Vec<Vec<Json>> = (1..=n).map(|i| vec![json!(i)]).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    let range = format!("A1:A{n}");
    s.execute("home.fontName", json!({"range": range, "name": cell_font})).unwrap();
    s.execute("home.fontSize", json!({"range": range, "size": cell_size})).unwrap();
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
            let xml = std::str::from_utf8(&data).unwrap();
            assert!(xml.contains("<row "), "the saved sheet has <row> elements: {xml}");
            data = rows_with(xml, row_attrs).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    let edited = out.finish().unwrap().into_inner();
    s.execute("file.open", json!({"name": "rows.xlsx", "base64": crate::io::base64_encode(&edited)})).unwrap();
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    assert_eq!(pv["pages"], list.len());
    list.iter().map(|p| p["rows"].as_str().unwrap().to_string()).collect()
}

#[test]
fn rows_helper_strips_heights() {
    let xml = r#"<sheetData><row r="1" spans="1:1" ht="30" customHeight="1"><c r="A1"/></row><row r="2"><c r="A2"/></row></sheetData>"#;
    assert_eq!(rows_with(xml, ""), r#"<sheetData><row r="1" spans="1:1"><c r="A1"/></row><row r="2"><c r="A2"/></row></sheetData>"#);
    assert_eq!(
        rows_with(xml, r#"ht="15""#),
        r#"<sheetData><row ht="15" r="1" spans="1:1"><c r="A1"/></row><row ht="15" r="2"><c r="A2"/></row></sheetData>"#
    );
}

/// Probe 1113ha `autobig`: Calibri 11 Normal, rows `ht="15"` without customHeight, A in Calibri 20:
/// Excel prints 26 rows a page (ours printed the stored 15 pt: 47).
#[test]
fn a_bigger_font_fits_a_row_stored_without_custom_height() {
    let pages = page_rows("Calibri", 11.0, "Calibri", 20.0, 52, r#"ht="15""#);
    assert_eq!(pages, ["1:26", "27:52"]);
}

/// The same rows with no stored height at all: Excel fits them to the Calibri 20 text, 26 a page.
#[test]
fn a_bigger_font_fits_a_row_with_no_stored_height() {
    let pages = page_rows("Calibri", 11.0, "Calibri", 20.0, 52, "");
    assert_eq!(pages, ["1:26", "27:52"]);
}

/// Probe 1113hb `autobig`: Courier New 8 Normal (default 12 pt), rows `ht="12"` not custom, A in
/// Calibri 20: Excel 30 rows a page (35 px = 184 dots under Courier New 8's pitch).
#[test]
fn a_bigger_font_fits_the_row_under_another_normal_font() {
    let pages = page_rows("Courier New", 8.0, "Calibri", 20.0, 60, r#"ht="12""#);
    assert_eq!(pages, ["1:30", "31:60"]);
}

/// Probe 1113ha `settall`-style control: a custom height wins over the font (15 pt = 121 dots, 47 a page).
#[test]
fn a_custom_height_still_wins_over_a_bigger_font() {
    let pages = page_rows("Calibri", 11.0, "Calibri", 20.0, 52, r#"ht="15" customHeight="1""#);
    assert_eq!(pages, ["1:47", "48:52"]);
}

/// Probe 1113ha `autosmall`: A in Calibri 8, rows `ht="15"` not custom: Excel 47 a page, the
/// Normal default (a smaller font never shrinks the row below it).
#[test]
fn a_smaller_font_never_shrinks_the_row_below_the_default() {
    let pages = page_rows("Calibri", 11.0, "Calibri", 8.0, 52, r#"ht="15""#);
    assert_eq!(pages, ["1:47", "48:52"]);
}

/// A cell font with a smaller default row than Normal's (Arial 10, 17 px, on Calibri 11, 20 px),
/// no stored height: the Normal default, 47 a page.
#[test]
fn a_font_with_a_shorter_default_row_keeps_the_normal_default() {
    let pages = page_rows("Calibri", 11.0, "Arial", 10.0, 52, "");
    assert_eq!(pages, ["1:47", "48:52"]);
}

/// Arial 10 Normal (17 px, 103 dots), A in Trebuchet MS 11, no stored heights. Excel writes
/// `defaultRowHeight="16.5"` (22 px) for a Trebuchet MS 11 Normal font, so these rows print at
/// 22 px = floor((8 x 22 x 103 + 17) / 136) = 133 dots: 42 rows a page (ours printed 55).
#[test]
fn trebuchet_cells_on_an_arial_sheet_print_at_trebuchets_row() {
    let pages = page_rows("Arial", 10.0, "Trebuchet MS", 11.0, 84, "");
    assert_eq!(pages, ["1:42", "43:84"]);
}
