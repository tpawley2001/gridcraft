//! `.xls` and XLSB chart sheets keep their page setup: orientation, paper, margins and
//! header/footer reach the printable copy and the chart sheet part saved to .xlsx. Files are built
//! in code from the [MS-XLS] 2.4 and [MS-XLSB] 2.4 record layouts.
use std::collections::HashMap;
use std::io::Read as _;

use gridcraft_model::{Orientation, PrintSettings, Workbook};

use super::xls_chart_tests::{bof, book, cat, cells, column_chart, eof, rec};
use super::xls_tests::xlstr;
use super::xlsb_chart_sheet_tests::package_with_chart_part;
use super::xlsb_tests::{record, wide};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

const BOTTOM_MARGIN: f64 = 0.9;

// --- .xls ---

/// A workbook with a worksheet and a chart sheet whose substream holds `page` after its BOF.
fn xls_book(page: &[u8]) -> Vec<u8> {
    let chart = column_chart();
    let head = bof(0x0020).len();
    let body = cat(&[&chart[..head], page, &chart[head..]]);
    book(&[("Sheet1", 0, cat(&[&bof(0x0010), &cells(), &eof()])), ("Chart1", 2, body)])
}

/// Setup (0x00A1) with paper 9 (A4), landscape (fPortrait clear), header/footer margins 0.3.
fn xls_page() -> Vec<u8> {
    let setup = cat(&[&super::xls_chart_tests::u16s(&[9, 100, 1, 1, 1, 0, 600, 600]), &0.3f64.to_le_bytes(), &0.3f64.to_le_bytes(), &[1, 0]]);
    cat(&[&rec(0x0014, &xlstr("&CTitle")), &rec(0x00A1, &setup), &rec(0x0026, &1.0f64.to_le_bytes()), &rec(0x0029, &BOTTOM_MARGIN.to_le_bytes())])
}

fn read_xls_book(page: &[u8]) -> Workbook {
    let (wb, report) = read_xls(&xls_book(page)).unwrap();
    assert_eq!(wb.chart_sheets.len(), 1, "{:?}", report.warnings);
    wb
}

// --- XLSB ---

/// BrtCsPageSetup (652): iPaperSize, iRes, iVRes, iCopies, iPageStart, flags, szRelID.
fn cs_page_setup(paper: u32, flags: u16) -> Vec<u8> {
    let mut p =
        cat(&[&paper.to_le_bytes(), &600u32.to_le_bytes(), &600u32.to_le_bytes(), &1u32.to_le_bytes(), &1u16.to_le_bytes(), &flags.to_le_bytes()]);
    p.extend(u32::MAX.to_le_bytes()); // A null szRelID.
    record(652, &p)
}

/// BrtMargins (476): left, right, top, bottom, header, footer.
fn margins(m: [f64; 6]) -> Vec<u8> {
    record(476, &m.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
}

/// BrtBeginHeaderFooter (479) with an odd-page header, then BrtEndHeaderFooter (480).
fn header(text: &str) -> Vec<u8> {
    let mut p = 0x000Cu16.to_le_bytes().to_vec(); // fHFScaleWithDoc, fHFAlignMargins.
    p.extend(wide(text));
    for _ in 0..5 {
        p.extend(u32::MAX.to_le_bytes());
    }
    cat(&[&record(479, &p), &record(480, &[])])
}

/// A chart sheet part: `page` records, then BrtDrawing naming the drawing.
fn xlsb_part(page: &[u8]) -> Vec<u8> {
    cat(&[page, &record(550, &wide("rId3"))])
}

fn xlsb_page() -> Vec<u8> {
    cat(&[&margins([1.0, 0.7, 0.75, BOTTOM_MARGIN, 0.3, 0.3]), &cs_page_setup(9, 0x0001), &header("&CTitle")])
}

fn read_xlsb_part(part: &[u8]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xlsb(&package_with_chart_part(part)).unwrap();
    assert_eq!(wb.chart_sheets.len(), 1, "{:?}", report.warnings);
    (wb, report.warnings)
}

// --- the saved chart sheet part ---

fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut data = vec![];
        f.read_to_end(&mut data).unwrap();
        out.insert(f.name().to_string(), data);
    }
    out
}

/// The XML of the one chart sheet part of a saved .xlsx.
fn saved_chart_sheet(wb: &Workbook) -> String {
    let parts = unzip(&write_xlsx(wb).unwrap());
    let mut sheets: Vec<_> =
        parts.iter().filter(|(n, _)| n.starts_with("xl/chartsheets/") && !n.contains("/_rels/") && n.ends_with(".xml")).collect();
    assert_eq!(sheets.len(), 1, "{:?}", parts.keys().collect::<Vec<_>>());
    String::from_utf8(sheets.remove(0).1.clone()).unwrap()
}

/// The start of element `name` in `xml` and its start tag (or `None` when absent).
fn element<'a>(xml: &'a str, name: &str) -> Option<(usize, &'a str)> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(i) = xml[from..].find(&open).map(|i| i + from) {
        let after = xml[i + open.len()..].chars().next();
        if matches!(after, Some(' ' | '>' | '/')) {
            let end = xml[i..].find('>').unwrap() + i + 1;
            return Some((i, &xml[i..end]));
        }
        from = i + open.len();
    }
    None
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let i = tag.find(&key)? + key.len();
    Some(&tag[i..i + tag[i..].find('"')?])
}

fn assert_kept(p: &PrintSettings) {
    assert_eq!(p.orientation, Orientation::Landscape);
    assert_eq!(p.paper, "A4");
    assert_eq!(p.margins[0], 1.0);
    assert_eq!(p.margins[3], BOTTOM_MARGIN as f32);
    assert_eq!(p.header, "&CTitle");
    // The header codes are kept verbatim: `&C` puts "Title" in the centre section.
    assert_eq!(p.header.strip_prefix("&C"), Some("Title"));
}

fn assert_saved_xml(xml: &str) {
    let (margins_at, margins) = element(xml, "pageMargins").expect(xml);
    assert_eq!(attr(margins, "left"), Some("1"));
    assert_eq!(attr(margins, "bottom"), Some("0.9"));
    let (setup_at, setup) = element(xml, "pageSetup").expect(xml);
    assert_eq!(attr(setup, "orientation"), Some("landscape"));
    assert_eq!(attr(setup, "paperSize"), Some("9"));
    let (hf_at, _) = element(xml, "headerFooter").expect(xml);
    assert!(xml.contains("<oddHeader>&amp;CTitle</oddHeader>"), "{xml}");
    let (drawing_at, _) = element(xml, "drawing").expect(xml);
    // CT_Chartsheet order: pageMargins, pageSetup, headerFooter, drawing.
    assert!(margins_at < setup_at && setup_at < hf_at && hf_at < drawing_at, "{xml}");
}

fn assert_default_xml(xml: &str) {
    let (_, margins) = element(xml, "pageMargins").expect(xml);
    for (k, v) in [("left", "0.7"), ("right", "0.7"), ("top", "0.75"), ("bottom", "0.75"), ("header", "0.3"), ("footer", "0.3")] {
        assert_eq!(attr(margins, k), Some(v), "{xml}");
    }
    assert!(element(xml, "pageSetup").is_none(), "{xml}");
    assert!(element(xml, "headerFooter").is_none(), "{xml}");
    assert!(element(xml, "drawing").is_some(), "{xml}");
}

/// The chart sheet's print settings after saving as .xlsx and reading it back.
fn via_xlsx(wb: &Workbook) -> PrintSettings {
    let (back, _) = read_xlsx(&write_xlsx(wb).unwrap()).unwrap();
    assert_eq!(back.chart_sheets.len(), 1);
    back.chart_sheets[0].print.clone()
}

#[test]
fn xls_chart_sheet_keeps_its_page_setup() {
    let wb = read_xls_book(&xls_page());
    assert_kept(&wb.chart_sheets[0].print);
    assert_saved_xml(&saved_chart_sheet(&wb));
}

#[test]
fn xls_chart_sheet_without_page_records_has_default_print_settings() {
    let wb = read_xls_book(&[]);
    assert_eq!(wb.chart_sheets[0].print, PrintSettings::default());
    assert_default_xml(&saved_chart_sheet(&wb));
}

#[test]
fn xls_chart_sheet_page_setup_round_trips_through_xlsx() {
    for page in [xls_page(), vec![]] {
        let wb = read_xls_book(&page);
        assert_eq!(via_xlsx(&wb), wb.chart_sheets[0].print);
    }
}

#[test]
fn xlsb_chart_sheet_keeps_its_page_setup() {
    let (wb, warnings) = read_xlsb_part(&xlsb_part(&xlsb_page()));
    assert!(!warnings.iter().any(|w| w.contains("damaged")), "{warnings:?}");
    assert_kept(&wb.chart_sheets[0].print);
    assert_saved_xml(&saved_chart_sheet(&wb));
}

#[test]
fn xlsb_chart_sheet_without_page_records_has_default_print_settings() {
    let (wb, _) = read_xlsb_part(&xlsb_part(&[]));
    assert_eq!(wb.chart_sheets[0].print, PrintSettings::default());
    assert_default_xml(&saved_chart_sheet(&wb));
}

#[test]
fn xlsb_chart_sheet_page_setup_round_trips_through_xlsx() {
    for page in [xlsb_page(), vec![]] {
        let (wb, _) = read_xlsb_part(&xlsb_part(&page));
        assert_eq!(via_xlsx(&wb), wb.chart_sheets[0].print);
    }
}

#[test]
fn xlsb_truncated_cs_page_setup_keeps_the_chart_sheet_and_warns_once() {
    let truncated = record(652, &[9, 0, 0]);
    let (wb, warnings) = read_xlsb_part(&xlsb_part(&truncated));
    assert_eq!(wb.chart_sheets[0].print, PrintSettings::default());
    let damaged: Vec<_> = warnings.iter().filter(|w| w.contains("BrtCsPageSetup")).collect();
    assert_eq!(damaged.len(), 1, "{warnings:?}");
}

#[test]
fn xlsb_custom_view_page_setup_is_not_the_sheet_page_setup() {
    // BrtBeginUserCsView (655) … BrtEndUserCsView (656) hold a custom view's own settings.
    let view = cat(&[&record(655, &[0; 24]), &cs_page_setup(9, 0x0001), &record(656, &[])]);
    let (wb, _) = read_xlsb_part(&xlsb_part(&view));
    assert_eq!(wb.chart_sheets[0].print, PrintSettings::default());
}

#[test]
fn xlsb_cs_page_setup_no_orient_keeps_portrait() {
    // fNoOrient (bit 3) leaves the orientation to the printer, even with fLandscape set.
    let (wb, _) = read_xlsb_part(&xlsb_part(&cs_page_setup(9, 0x0009)));
    let p = &wb.chart_sheets[0].print;
    assert_eq!((p.orientation, p.paper.as_str()), (Orientation::Portrait, "A4"));
}
