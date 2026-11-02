//! Synthetic workbook windows from the public [MS-XLS] and [MS-XLSB] layouts.
use super::xls_tests::{cat, compound, rec, u16s};
use super::xlsb_tests::{RELS, ROOT, TYPES, book, package, record, sst, worksheet};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};
use gridcraft_model::{Workbook, WorkbookView};

fn xls(views: &[&[u8]]) -> Workbook {
    let bof = |kind| rec(0x0809, &u16s(&[0x0600, kind, 0, 0, 0, 0, 0, 0]));
    let globals = |pos: u32| {
        let mut out = bof(0x0005);
        for view in views {
            out.extend(rec(0x003D, view));
        }
        out.extend(rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 1, 0, b'A']])));
        out.extend(rec(0x000A, &[]));
        out
    };
    let data = cat(&[&globals(globals(0).len() as u32), &bof(0x0010), &rec(0x000A, &[])]);
    read_xls(&compound("Workbook", &data)).unwrap().0
}

fn window(flags: u16, ratio: u16) -> Vec<u8> {
    u16s(&[0, 0, 100, 100, flags, 0, 0, 1, ratio])
}

fn xlsb(views: &[&[u8]]) -> Workbook {
    let mut data = record(131, &[]);
    data.extend(record(135, &[]));
    for view in views {
        data.extend(record(158, view));
    }
    data.extend(record(136, &[]));
    // The helper's workbook begins with the three-byte BrtBeginBook record.
    data.extend_from_slice(&book(false)[3..]);
    read_xlsb(&package(&data, &worksheet(&[], &[]), &sst(), ROOT, RELS, TYPES)).unwrap().0
}

fn book_view(flags: u8, ratio: u32) -> Vec<u8> {
    let mut out = vec![0; 29];
    out[16..20].copy_from_slice(&ratio.to_le_bytes());
    out[28] = flags;
    out
}

#[test]
fn xls_window_display_options_and_ratio() {
    for (flags, ratio, expected) in [
        (0x0038, 600, WorkbookView::default()),
        (0x0018, 250, WorkbookView { show_sheet_tabs: false, tab_ratio: 250, ..WorkbookView::default() }),
        (0x0020, 600, WorkbookView { show_horizontal_scroll: false, show_vertical_scroll: false, ..WorkbookView::default() }),
        (0x0038, 1500, WorkbookView { tab_ratio: 1000, ..WorkbookView::default() }),
        (0x0038, 0, WorkbookView { tab_ratio: 0, ..WorkbookView::default() }),
    ] {
        assert_eq!(xls(&[&window(flags, ratio)]).view, expected);
    }
}

#[test]
fn xlsb_window_display_options_and_ratio() {
    for (flags, ratio, expected) in [
        (0x38, 600, WorkbookView::default()),
        (0x18, 300, WorkbookView { show_sheet_tabs: false, tab_ratio: 300, ..WorkbookView::default() }),
        (0x20, 600, WorkbookView { show_horizontal_scroll: false, show_vertical_scroll: false, ..WorkbookView::default() }),
        (0x38, u32::MAX, WorkbookView { tab_ratio: 1000, ..WorkbookView::default() }),
        (0x38, 0, WorkbookView { tab_ratio: 0, ..WorkbookView::default() }),
    ] {
        assert_eq!(xlsb(&[&book_view(flags, ratio)]).view, expected);
    }
}

#[test]
fn truncated_windows_keep_available_fields_and_ignore_later_windows() {
    let first = window(0x0038, 250);
    let later = window(0, 0);
    for len in 0..18 {
        assert_eq!(xls(&[&first[..len], &later]).view, WorkbookView::default(), "Window1 length {len}");
    }
    let first = window(0x0018, 250);
    assert!(!xls(&[&first[..10]]).view.show_sheet_tabs);
    let first = book_view(0x38, 300);
    let later = book_view(0, 0);
    for len in 0..29 {
        let expected = WorkbookView { tab_ratio: if len >= 20 { 300 } else { 600 }, ..WorkbookView::default() };
        assert_eq!(xlsb(&[&first[..len], &later]).view, expected, "BrtBookView length {len}");
    }
    assert_eq!(xls(&[]).view, WorkbookView::default());
    assert_eq!(xlsb(&[]).view, WorkbookView::default());
    assert_eq!(xls(&[&window(0x0018, 250), &later]).view.tab_ratio, 250);
    assert_eq!(xlsb(&[&book_view(0x18, 300), &later]).view.tab_ratio, 300);
}

#[test]
fn hidden_tabs_survive_xlsx_conversion() {
    for wb in [xls(&[&window(0x0018, 250)]), xlsb(&[&book_view(0x18, 300)])] {
        let bytes = write_xlsx(&wb).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("xl/workbook.xml").unwrap(), &mut xml).unwrap();
        assert!(xml.contains("showSheetTabs=\"0\""));
        assert_eq!(read_xlsx(&bytes).unwrap().0.view, wb.view);
    }
}
