//! Synthetic Custom Views from [MS-XLSB] BrtUserBookView and BrtBeginUserShView.
use super::xls_custom_views_tests::{GUID, check, xml};
use super::xlsb_tests::{RELS, ROOT, TYPES, record, sst, wide, worksheet};
use crate::{read_xlsb, write_xlsx};
fn view(guid: &[u8; 16]) -> Vec<u8> {
    view_on(guid, 22)
}
/// A BrtUserBookView named "Print view" whose active sheet is the one with sheet id `sheet_id`.
pub(super) fn view_on(guid: &[u8; 16], sheet_id: u32) -> Vec<u8> {
    let mut b = Vec::new();
    for n in [0u32, 800, 0, 600, sheet_id, 500] {
        b.extend(n.to_le_bytes());
    }
    b.extend(guid);
    b.extend([0; 6]);
    b.extend(wide("Print view"));
    record(397, &b)
}
fn file(views: &[u8], guid: &[u8; 16]) -> Vec<u8> {
    let mut b = record(131, &[]);
    b.extend(views);
    b.extend(record(143, &[]));
    for (id, rel, name) in [(11u32, "one", "Data"), (22, "two", "Other")] {
        let payload = [0u32.to_le_bytes().to_vec(), id.to_le_bytes().to_vec(), wide(rel), wide(name)].concat();
        b.extend(record(156, &payload));
    }
    b.extend(record(144, &[]));
    b.extend(record(132, &[]));
    let tail = sheet_view(guid);
    super::make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &b),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", &worksheet(&[], &[])),
        ("data/two.bin", &worksheet(&[], &tail)),
        ("text/strings.bin", &sst()),
    ])
}
/// A sheet's Custom View records (BrtBeginUserShView .. BrtEndUserShView) for the view `guid`.
pub(super) fn sheet_view(guid: &[u8; 16]) -> Vec<u8> {
    let mut sv = guid.to_vec();
    for n in [22u32, 80, 0, 0, 0, 0, 0, 0] {
        sv.extend(n.to_le_bytes());
    }
    let mut pane = [0f64.to_le_bytes(), 1f64.to_le_bytes()].concat();
    for n in [1u32, 0, 2] {
        pane.extend(n.to_le_bytes());
    }
    pane.push(2);
    let selection: Vec<_> = [2u32, 1, 1, 0, 1, 1, 1, 1, 1].iter().flat_map(|n| n.to_le_bytes()).collect();
    [record(423, &sv), record(151, &pane), record(152, &selection), record(424, &[])].concat()
}
#[test]
fn xlsb_custom_views_roundtrip() {
    for count in [1, 2] {
        let views = if count == 1 { view(&GUID) } else { [view(&GUID), view(&[9; 16])].concat() };
        check(&read_xlsb(&file(&views, &GUID)).unwrap().0, count);
    }
}
#[test]
fn xlsb_orphan_and_truncated_custom_views() {
    let (wb, _) = read_xlsb(&file(&view(&GUID), &[9; 16])).unwrap();
    assert!(!xml(&write_xlsx(&wb).unwrap(), "xl/worksheets/sheet2.xml").contains("<customSheetViews"));
    let (wb, report) = read_xlsb(&file(&[record(397, &[0; 10]), record(397, &[])].concat(), &GUID)).unwrap();
    check(&wb, 0);
    assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1);
}

#[test]
fn xlsb_custom_view_limit() {
    let (wb, report) = read_xlsb(&file(&view(&GUID).repeat(1025), &GUID)).unwrap();
    assert_eq!(xml(&write_xlsx(&wb).unwrap(), "xl/workbook.xml").matches("<customWorkbookView ").count(), 1024);
    assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1);
}
#[test]
fn xlsb_truncated_book_view_payloads() {
    let mut b = Vec::new();
    for n in [0u32, 800, 0, 600, 22, 500] {
        b.extend(n.to_le_bytes());
    }
    b.extend(GUID);
    b.extend([0; 6]);
    b.extend(wide("Print view"));
    for len in 0..b.len() {
        let (wb, report) = read_xlsb(&file(&record(397, &b[..len]), &GUID)).unwrap();
        assert!(!xml(&write_xlsx(&wb).unwrap(), "xl/workbook.xml").contains("<customWorkbookViews"), "length {len}");
        assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1, "length {len}");
    }
}

#[test]
fn xlsb_custom_view_flags_and_geometry() {
    let mut data = view(&GUID)[3..].to_vec();
    data[..4].copy_from_slice(&(-10i32).to_le_bytes());
    data[40..42].copy_from_slice(&20u16.to_le_bytes());
    data[42..46].copy_from_slice(&0x1f8efu32.to_le_bytes());
    let (wb, _) = read_xlsb(&file(&record(397, &data), &GUID)).unwrap();
    let book = xml(&write_xlsx(&wb).unwrap(), "xl/workbook.xml");
    for attr in [
        "xWindow=\"-10\"",
        "windowWidth=\"810\"",
        "showFormulaBar=\"1\"",
        "showStatusbar=\"1\"",
        "showComments=\"commIndAndComment\"",
        "includePrintSettings=\"1\"",
        "includeHiddenRowCol=\"1\"",
        "autoUpdate=\"1\"",
        "mergeInterval=\"20\"",
        "changesSavedWin=\"1\"",
        "onlySync=\"1\"",
        "personalView=\"1\"",
        "minimized=\"1\"",
    ] {
        assert!(book.contains(attr), "{attr}: {book}");
    }
}
