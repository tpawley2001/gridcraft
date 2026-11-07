//! XLSB links to other workbooks: a cached BrtExternCellReal holding NaN or an infinity is saved
//! as the error `#NUM!`, never as `NaN`/`inf` text, which is not a SpreadsheetML number.
use super::xlsb_external_link_cache_tests::{file, link_part, saved_part};
use super::xlsb_tests::record;
use crate::{read_xlsb, read_xlsx, write_xlsx};

/// BrtExternCellReal at B2 holding `x`.
fn real(x: f64) -> Vec<u8> {
    let mut data = 1u32.to_le_bytes().to_vec();
    data.extend(x.to_le_bytes());
    record(368, &data)
}

fn saved_link(x: f64) -> (Vec<u8>, String) {
    let (wb, _) = read_xlsb(&file(&[link_part("Data", &real(x))])).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    (saved, link)
}

#[test]
fn non_finite_cached_numbers_are_saved_as_num_errors() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (saved, link) = saved_link(x);
        assert!(link.contains(r##"<cell r="B2" t="e"><v>#NUM!</v></cell>"##), "{x}: {link}");
        let lower = link.to_ascii_lowercase();
        assert!(!lower.contains("nan") && !lower.contains("inf<"), "{x}: {link}");
        // The saved workbook opens again.
        read_xlsx(&saved).unwrap();
    }
}

#[test]
fn finite_cached_numbers_are_unchanged() {
    let (_, link) = saved_link(42.5);
    assert!(link.contains(r#"<cell r="B2"><v>42.5</v></cell>"#), "{link}");
    let (_, link) = saved_link(1e-300);
    let v = link.split(r#"<cell r="B2"><v>"#).nth(1).and_then(|s| s.split("</v>").next()).expect("B2 number");
    assert_eq!(v.parse::<f64>().unwrap(), 1e-300, "{link}");
}
