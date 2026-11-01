//! A page setup whose `scale` is 0 (ECMA-376 Part 1 §18.3.1.64: a percentage, 10 to 400, default
//! 100) prints at 100% as in Excel, which treats the 0 as not stated and drops it on save.

use super::minimal;
use crate::{read_xlsx, write_xlsx};

/// The print scale read from a sheet whose body is `body`, and the `<pageSetup ...>` start tag
/// written when it is saved again (empty if none is written).
fn scale_and_written_setup(body: &str) -> (u16, String) {
    let (wb, _) = read_xlsx(&minimal(body, &[], "", "")).unwrap();
    let scale = wb.sheet(0).unwrap().print.scale;
    let bytes = write_xlsx(&wb).unwrap();
    let xml = crate::package::Package::open(&bytes).unwrap().read("xl/worksheets/sheet1.xml").unwrap().unwrap();
    let xml = String::from_utf8(xml).unwrap();
    let tag = xml.split_once("<pageSetup").and_then(|(_, r)| r.split_once('>')).map(|(t, _)| t.to_string()).unwrap_or_default();
    (scale, tag)
}

const DATA: &str = r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#;

#[test]
fn scale_zero_reads_as_one_hundred_percent() {
    let (scale, tag) = scale_and_written_setup(&format!(r#"{DATA}<pageSetup scale="0" orientation="portrait"/>"#));
    assert_eq!(scale, 100);
    assert!(!tag.contains("scale="), "{tag}");
}

#[test]
fn all_zero_page_setup_keeps_default_scale_and_paper() {
    let zero = format!(
        r#"{DATA}<pageSetup paperSize="0" scale="0" firstPageNumber="0" orientation="portrait" usePrinterDefaults="0" horizontalDpi="0" verticalDpi="0" copies="0"/>"#
    );
    let (wb, _) = read_xlsx(&minimal(&zero, &[], "", "")).unwrap();
    let (plain, _) = read_xlsx(&minimal(DATA, &[], "", "")).unwrap();
    assert_eq!(wb.sheet(0).unwrap().print.scale, 100);
    assert_eq!(wb.sheet(0).unwrap().print.paper, plain.sheet(0).unwrap().print.paper);
    let (_, tag) = scale_and_written_setup(&zero);
    assert!(!tag.contains("scale="), "{tag}");
}

#[test]
fn stated_scale_is_kept() {
    let (scale, tag) = scale_and_written_setup(&format!(r#"{DATA}<pageSetup scale="55" orientation="portrait"/>"#));
    assert_eq!(scale, 55);
    assert!(tag.contains(r#"scale="55""#), "{tag}");
}

#[test]
fn no_page_setup_is_one_hundred_percent() {
    let (scale, tag) = scale_and_written_setup(DATA);
    assert_eq!(scale, 100);
    assert!(!tag.contains("scale="), "{tag}");
}
