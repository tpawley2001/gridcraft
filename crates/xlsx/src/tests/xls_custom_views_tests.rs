//! Synthetic Custom Views from [MS-XLS] UserBView and UserSViewBegin.
use super::xls_tests::{cat, compound, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

pub(super) const GUID: [u8; 16] = [0x78, 0x56, 0x34, 0x12, 0xbc, 0x9a, 0xf0, 0xde, 1, 2, 3, 4, 5, 6, 7, 8];
pub(super) const GUID_XML: &str = "{12345678-9ABC-DEF0-0102-030405060708}";

fn view(guid: &[u8; 16]) -> Vec<u8> {
    let mut b = vec![0; 50];
    b[4..6].copy_from_slice(&22u16.to_le_bytes());
    b[8..24].copy_from_slice(guid);
    b[32..36].copy_from_slice(&800i32.to_le_bytes());
    b[36..40].copy_from_slice(&600i32.to_le_bytes());
    b[40..42].copy_from_slice(&500u16.to_le_bytes());
    b.extend(u16s(&[10]));
    b.push(0);
    b.extend(b"Print view");
    rec(0x01a9, &b)
}
fn sheet_view(guid: &[u8; 16]) -> Vec<u8> {
    let mut b = vec![0; 64];
    b[..16].copy_from_slice(guid);
    b[20..24].copy_from_slice(&80u32.to_le_bytes());
    let pane = rec(0x41, &u16s(&[0, 1, 1, 0, 2]));
    let selection = rec(0x1d, &cat(&[&[2], &u16s(&[1, 1, 0, 1, 1, 1]), &[1, 1]]));
    // A margin inside a view must never affect the worksheet's ordinary print settings.
    cat(&[&rec(0x01aa, &b), &pane, &selection, &rec(0x26, &9f64.to_le_bytes()), &rec(0x01ab, &[0, 0])])
}
fn file(views: &[u8], sv: &[u8]) -> Vec<u8> {
    let bof = |kind| rec(0x0809, &u16s(&[0x600, kind, 0, 0, 0, 0, 0, 0]));
    let sheet1 = cat(&[&bof(0x10), &rec(0xa, &[])]);
    let globals = |start: usize| {
        let mut b = bof(5);
        b.extend(rec(0x13d, &u16s(&[11, 22])));
        b.extend(views);
        for (offset, name) in [(start, b'A'), (start + sheet1.len(), b'B')] {
            b.extend(rec(0x85, &cat(&[&(offset as u32).to_le_bytes(), &[0, 0, 1, 0, name]])));
        }
        b.extend(rec(0xa, &[]));
        b
    };
    compound("Workbook", &cat(&[&globals(globals(0).len()), &sheet1, &bof(0x10), sv, &rec(0xa, &[])]))
}
pub(super) fn xml(bytes: &[u8], path: &str) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    z.by_name(path).unwrap().read_to_string(&mut out).unwrap();
    out
}
pub(super) fn check(wb: &gridcraft_model::Workbook, count: usize) {
    let bytes = write_xlsx(wb).unwrap();
    let book = xml(&bytes, "xl/workbook.xml");
    assert_eq!(book.matches("<customWorkbookView ").count(), count);
    if count > 0 {
        for attr in ["name=\"Print view\"", &format!("guid=\"{GUID_XML}\""), "activeSheetId=\"2\"", "tabRatio=\"500\""] {
            assert!(book.contains(attr), "{attr}: {book}");
        }
        let sheet = xml(&bytes, "xl/worksheets/sheet2.xml");
        assert!(sheet.contains("<customSheetViews>"));
        assert!(sheet.contains("scale=\"80\""));
        assert!(sheet.contains("showGridLines=\"0\""));
        assert!(sheet.find("</sheetData>").unwrap() < sheet.find("<customSheetViews>").unwrap());
        let views = sheet.split("<customSheetViews>").nth(1).unwrap().split("</customSheetViews>").next().unwrap();
        assert!(!views.contains("<pageSetup"));
        assert!(!sheet.contains("left=\"9\""));
        for attr in ["topLeftCell=\"A2\"", "activePane=\"bottomLeft\"", "activeCell=\"B2\"", "sqref=\"B2\""] {
            assert!(sheet.contains(attr), "{sheet}");
        }
        if count == 2 {
            assert!(book.find(GUID_XML).unwrap() < book.find("{09090909-0909-0909-0909-090909090909}").unwrap());
        }
    }
    let reread = read_xlsx(&bytes).unwrap().0;
    let again = write_xlsx(&reread).unwrap();
    assert_eq!(xml(&again, "xl/workbook.xml").matches("<customWorkbookView ").count(), count);
}
#[test]
fn xls_custom_views_roundtrip() {
    for count in [1, 2] {
        let views = if count == 1 { view(&GUID) } else { cat(&[&view(&GUID), &view(&[9; 16])]) };
        let (wb, _) = read_xls(&file(&views, &sheet_view(&GUID))).unwrap();
        check(&wb, count);
    }
}
#[test]
fn xls_orphan_and_truncated_custom_views() {
    let (wb, _) = read_xls(&file(&view(&GUID), &sheet_view(&[9; 16]))).unwrap();
    assert!(!xml(&write_xlsx(&wb).unwrap(), "xl/worksheets/sheet2.xml").contains("<customSheetViews"));
    let (wb, report) = read_xls(&file(&cat(&[&rec(0x1a9, &[0; 10]), &rec(0x1a9, &[])]), &[])).unwrap();
    check(&wb, 0);
    assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1);
}

#[test]
fn xls_custom_view_limit_and_damaged_sheet_block() {
    let views = view(&GUID).repeat(1025);
    let (wb, report) = read_xls(&file(&views, &[])).unwrap();
    assert_eq!(xml(&write_xlsx(&wb).unwrap(), "xl/workbook.xml").matches("<customWorkbookView ").count(), 1024);
    assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1);
    let mut sv = sheet_view(&GUID);
    sv.truncate(sv.len() - 6); // Missing UserSViewEnd.
    let (wb, report) = read_xls(&file(&view(&GUID), &sv)).unwrap();
    assert!(!xml(&write_xlsx(&wb).unwrap(), "xl/worksheets/sheet2.xml").contains("<customSheetViews"));
    assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1);
}
#[test]
fn xls_truncated_book_view_payloads() {
    let full = view(&GUID);
    for len in 0..full.len() - 4 {
        let (wb, report) = read_xls(&file(&rec(0x1a9, &full[4..4 + len]), &[])).unwrap();
        assert!(!xml(&write_xlsx(&wb).unwrap(), "xl/workbook.xml").contains("<customWorkbookViews"), "length {len}");
        assert_eq!(report.warnings.iter().filter(|s| s.contains("Custom Views")).count(), 1, "length {len}");
    }
}

#[test]
fn xls_custom_view_flags_and_geometry() {
    let mut data = view(&GUID)[4..].to_vec();
    data[24..28].copy_from_slice(&(-2i32).to_le_bytes());
    data[42..44].copy_from_slice(&0xec77u16.to_le_bytes());
    data[46..48].copy_from_slice(&3u16.to_le_bytes());
    data[48..50].copy_from_slice(&15u16.to_le_bytes());
    let (wb, _) = read_xls(&file(&rec(0x1a9, &data), &sheet_view(&GUID))).unwrap();
    let book = xml(&write_xlsx(&wb).unwrap(), "xl/workbook.xml");
    for attr in [
        "xWindow=\"-30\"",
        "windowWidth=\"12000\"",
        "showFormulaBar=\"1\"",
        "showStatusbar=\"1\"",
        "showComments=\"commIndicator\"",
        "showObjects=\"all\"",
        "includePrintSettings=\"1\"",
        "includeHiddenRowCol=\"1\"",
        "autoUpdate=\"1\"",
        "mergeInterval=\"15\"",
        "changesSavedWin=\"1\"",
        "onlySync=\"1\"",
        "personalView=\"1\"",
        "minimized=\"1\"",
    ] {
        assert!(book.contains(attr), "{attr}: {book}");
    }
}
