//! A workbook's stored window position and size (`workbookView xWindow`, `yWindow`,
//! `windowWidth`, `windowHeight`, twips; ECMA-376 Part 1 §18.2.30) survive open + save.
//! Excel reopens a workbook in the window the file records.
use std::io::Read;

use gridcraft_model::{WindowRect, Workbook};

use crate::{read_xlsx, write_xlsx};

fn open(book_views: &str) -> Workbook {
    read_xlsx(&super::minimal("", &[], "", book_views)).unwrap().0
}

fn workbook_xml(wb: &Workbook) -> String {
    let out = write_xlsx(wb).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let mut xml = String::new();
    z.by_name("xl/workbook.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

/// The one `<workbookView .../>` element of a saved workbook.
fn view_element(xml: &str) -> &str {
    let start = xml.find("<workbookView").expect("a workbookView element");
    let end = start + xml[start..].find("/>").expect("a self-closing workbookView");
    &xml[start..end]
}

const GEOMETRY: [&str; 4] = ["xWindow", "yWindow", "windowWidth", "windowHeight"];

#[test]
fn workbook_window_rect_is_read_from_the_view() {
    let wb = open(r#"<bookViews><workbookView xWindow="-120" yWindow="-120" windowWidth="15600" windowHeight="11760"/></bookViews>"#);
    assert_eq!(wb.view.window, Some(WindowRect { x: -120, y: -120, width: 15600, height: 11760 }));
}

#[test]
fn workbook_window_rect_is_written_back() {
    let wb = open(r#"<bookViews><workbookView xWindow="-120" yWindow="-120" windowWidth="15600" windowHeight="11760"/></bookViews>"#);
    let xml = workbook_xml(&wb);
    let view = view_element(&xml);
    for attr in [r#" xWindow="-120""#, r#" yWindow="-120""#, r#" windowWidth="15600""#, r#" windowHeight="11760""#] {
        assert!(view.contains(attr), "the writer keeps{attr} in {view}");
    }
    assert_eq!(xml.matches("<workbookView").count(), 1, "still one window: {xml}");
    let back = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap().0;
    assert_eq!(back.view, wb.view, "a second open reads the same view");
}

#[test]
fn workbook_window_rect_is_kept_next_to_the_other_view_settings() {
    let wb = open(
        r#"<bookViews><workbookView xWindow="2340" yWindow="1005" windowWidth="20730" windowHeight="11160" tabRatio="250" showSheetTabs="0"/></bookViews>"#,
    );
    assert_eq!(wb.view.window, Some(WindowRect { x: 2340, y: 1005, width: 20730, height: 11160 }));
    assert_eq!(wb.view.tab_ratio, 250);
    assert!(!wb.view.show_sheet_tabs);
    let xml = workbook_xml(&wb);
    let view = view_element(&xml);
    for attr in [
        r#" xWindow="2340""#,
        r#" yWindow="1005""#,
        r#" windowWidth="20730""#,
        r#" windowHeight="11160""#,
        r#" tabRatio="250""#,
        r#" showSheetTabs="0""#,
    ] {
        assert!(view.contains(attr), "the writer keeps{attr} in {view}");
    }
}

#[test]
fn workbook_window_rect_needs_all_four_valid_values() {
    for attrs in [
        r#"xWindow="240" yWindow="15""#,
        r#"windowWidth="15600" windowHeight="11760""#,
        r#"xWindow="abc" yWindow="0" windowWidth="100" windowHeight="100""#,
        r#"xWindow="0" yWindow="0" windowWidth="-5" windowHeight="100""#,
        r#"xWindow="0" yWindow="0" windowWidth="100" windowHeight="""#,
        r#"xWindow="0" yWindow="99999999999" windowWidth="100" windowHeight="100""#,
    ] {
        let wb = open(&format!("<bookViews><workbookView {attrs}/></bookViews>"));
        assert_eq!(wb.view.window, None, "{attrs}");
        let xml = workbook_xml(&wb);
        let view = view_element(&xml);
        for attr in GEOMETRY {
            assert!(!view.contains(attr), "{attr} is not written for {attrs}: {view}");
        }
    }
}

#[test]
fn workbook_window_rect_absent_stays_absent() {
    assert_eq!(Workbook::new().view.window, None);
    assert_eq!(open("").view.window, None, "no <bookViews>");
    assert_eq!(open(r#"<bookViews><workbookView activeTab="0"/></bookViews>"#).view.window, None);
    for wb in [Workbook::new(), open("")] {
        let xml = workbook_xml(&wb);
        assert!(xml.contains(r#"<workbookView activeTab="0"/>"#), "a workbook without a stored window is written as before: {xml}");
    }
}

#[test]
fn workbook_window_rect_comes_from_the_selected_window() {
    // The first visible window is the one modelled; its geometry is the one kept.
    let wb = open(concat!(
        r#"<bookViews><workbookView visibility="hidden" xWindow="1" yWindow="2" windowWidth="3" windowHeight="4"/>"#,
        r#"<workbookView xWindow="10" yWindow="20" windowWidth="3000" windowHeight="4000"/></bookViews>"#
    ));
    assert_eq!(wb.view.window, Some(WindowRect { x: 10, y: 20, width: 3000, height: 4000 }));
    let xml = workbook_xml(&wb);
    let view = view_element(&xml);
    for attr in [r#" xWindow="10""#, r#" yWindow="20""#, r#" windowWidth="3000""#, r#" windowHeight="4000""#] {
        assert!(view.contains(attr), "the writer keeps{attr} in {view}");
    }
}

#[test]
fn workbook_window_rect_set_in_the_model_is_written() {
    let mut wb = Workbook::new();
    wb.view.window = Some(WindowRect { x: 0, y: 0, width: 28800, height: 12300 });
    let xml = workbook_xml(&wb);
    let view = view_element(&xml);
    for attr in [r#" xWindow="0""#, r#" yWindow="0""#, r#" windowWidth="28800""#, r#" windowHeight="12300""#] {
        assert!(view.contains(attr), "the writer keeps{attr} in {view}");
    }
}
