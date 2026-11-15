//! Overflowing Greek and Cyrillic text widens the printed range by its printed width: Excel prints
//! these letters at ordinary proportional widths (about half an em in Calibri), not at the full em
//! of CJK ideographs. Page setup as `justify_wraps_tests.rs`: Letter portrait, 0.5" margins,
//! Calibri 11, default column widths; numbers in A1 and L1, so without overflow the printed column
//! bands are A:J and K:L. The text under test overflows from L1.
use serde_json::json;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "pageLayout.pageSetup",
        json!({"paper": "Letter", "orientation": "portrait", "margins": {"left": 0.5, "right": 0.5, "top": 0.5, "bottom": 0.5}}),
    )
    .unwrap();
    s
}

/// The distinct printed column bands (pageList `columns`), sorted.
fn bands_with_l1(text: Option<&str>) -> Vec<String> {
    let mut s = session();
    s.execute("cell.set", json!({"cell": "A1", "input": "1"})).unwrap();
    match text {
        Some(t) => s.execute("range.setValues", json!({"range": "L1", "values": [[t]]})).unwrap(),
        None => s.execute("cell.set", json!({"cell": "L1", "input": "1"})).unwrap(),
    };
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    let list = pv["pageList"].as_array().unwrap();
    let mut b: Vec<String> = list.iter().map(|p| p["columns"].as_str().unwrap().to_string()).collect();
    b.sort();
    b.dedup();
    b
}

/// The last column (0-based) of the band that starts at K.
fn k_band_end(bands: &[String]) -> u32 {
    let k = bands.iter().find(|b| b.starts_with("K:")).unwrap_or_else(|| panic!("no K band: {bands:?}"));
    let end = k.split(':').nth(1).unwrap();
    end.bytes().fold(0u32, |n, c| n * 26 + u32::from(c - b'A' + 1)) - 1
}

const CYRILLIC: &str = "абвгдежзи ";
const GREEK: &str = "αβγδεζηθι ";
const LATIN: &str = "abcdefghi ";

#[test]
fn no_overflow_prints_two_bands() {
    assert_eq!(bands_with_l1(None), ["A:J", "K:L"]);
}

#[test]
fn latin_text_stays_in_the_second_band() {
    let b = bands_with_l1(Some(&LATIN.repeat(7)));
    assert_eq!(b.len(), 2, "{b:?}");
    assert_eq!(b[0], "A:J");
    assert!(b[1].starts_with("K:"), "{b:?}");
}

#[test]
fn cyrillic_text_is_measured_like_latin_letters() {
    let b = bands_with_l1(Some(&CYRILLIC.repeat(7)));
    assert_eq!(b.len(), 2, "no extra strip for 70 Cyrillic characters: {b:?}");
    assert!(!b.iter().any(|x| x.starts_with("U:")), "{b:?}");
}

#[test]
fn greek_text_is_measured_like_latin_letters() {
    let b = bands_with_l1(Some(&GREEK.repeat(7)));
    assert_eq!(b.len(), 2, "no extra strip for 70 Greek characters: {b:?}");
    assert!(!b.iter().any(|x| x.starts_with("U:")), "{b:?}");
}

#[test]
fn cyrillic_reaches_about_as_far_as_latin_text() {
    let l = 11; // column L
    let latin = k_band_end(&bands_with_l1(Some(&LATIN.repeat(7)))) - l;
    let cyr = k_band_end(&bands_with_l1(Some(&CYRILLIC.repeat(7)))) - l;
    assert!(cyr >= latin, "Cyrillic {cyr} columns past L, Latin {latin}");
    assert!(cyr * 4 <= latin * 5, "Cyrillic {cyr} columns past L, Latin {latin}: at most 1.25x");
}

#[test]
fn cjk_text_keeps_its_full_width() {
    let b = bands_with_l1(Some(&"漢".repeat(70)));
    assert!(b.iter().any(|x| x.starts_with("U:")), "70 ideographs still run past column T: {b:?}");
}
