//! Every page-setup paper size id survives open and save, as in Excel, which writes back the id it
//! read (ECMA-376 Part 1 §18.3.1.63 `paperSize`; [MS-XLS] 2.4.257 Setup `iPaperSize`), including
//! printer-specific ids it has no size for.

use super::minimal;
use super::xls_chart_tests::{bof, book, cat, cells, eof, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};
use gridcraft_model::Workbook;

const DATA: &str = r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#;

/// The `<pageSetup ...>` start tag of sheet 1 when `wb` is saved (empty if none is written).
fn written_setup(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let xml = crate::package::Package::open(&bytes).unwrap().read("xl/worksheets/sheet1.xml").unwrap().unwrap();
    let xml = String::from_utf8(xml).unwrap();
    xml.split_once("<pageSetup").and_then(|(_, r)| r.split_once('>')).map(|(t, _)| t.to_string()).unwrap_or_default()
}

fn xlsx_with_paper(id: u32) -> Workbook {
    let body = format!(r#"{DATA}<pageSetup paperSize="{id}" orientation="portrait"/>"#);
    read_xlsx(&minimal(&body, &[], "", "")).unwrap().0
}

#[test]
fn xlsx_paper_ids_are_written_back_unchanged() {
    // 10 A4 small, 14 Folio, 17 11x17, 66 A2, then printer-specific ids from real files.
    for id in [2u32, 9, 10, 14, 17, 66, 102, 119, 185, 256] {
        let tag = written_setup(&xlsx_with_paper(id));
        assert!(tag.contains(&format!(r#"paperSize="{id}""#)), "paperSize {id}: wrote <pageSetup{tag}>");
    }
}

#[test]
fn same_sized_papers_keep_distinct_ids() {
    // A4 small is A4-sized and 11x17 is Tabloid-sized, but each keeps its own id.
    assert_ne!(xlsx_with_paper(10).sheet(0).unwrap().print.paper, xlsx_with_paper(9).sheet(0).unwrap().print.paper);
    assert_ne!(xlsx_with_paper(17).sheet(0).unwrap().print.paper, xlsx_with_paper(3).sheet(0).unwrap().print.paper);
}

#[test]
fn letter_stays_unwritten_and_zero_stays_unstated() {
    assert!(!written_setup(&xlsx_with_paper(1)).contains("paperSize="));
    assert!(!written_setup(&xlsx_with_paper(0)).contains("paperSize="));
}

#[test]
fn changing_the_paper_replaces_a_kept_id() {
    for id in [10u32, 185] {
        let mut wb = xlsx_with_paper(id);
        wb.sheet_mut(0).unwrap().print.paper = "A4".to_string();
        let tag = written_setup(&wb);
        assert!(tag.contains(r#"paperSize="9""#), "paperSize {id} changed to A4: wrote <pageSetup{tag}>");
        assert!(!tag.contains(&format!(r#"paperSize="{id}""#)), "{tag}");
    }
}

/// A one-sheet .xls whose worksheet has a Setup record with paper `id` (landscape, 100%).
fn xls_with_paper(id: u16) -> Workbook {
    let setup = cat(&[&u16s(&[id, 100, 1, 1, 1, 0, 600, 600]), &0.3f64.to_le_bytes(), &0.3f64.to_le_bytes(), &[1, 0]]);
    let sheet = cat(&[&bof(0x0010), &cells(), &rec(0x00A1, &setup), &eof()]);
    read_xls(&book(&[("Sheet1", 0, sheet)])).unwrap().0
}

#[test]
fn xls_paper_ids_reach_the_saved_xlsx() {
    for id in [9u16, 10, 17, 66, 256] {
        let tag = written_setup(&xls_with_paper(id));
        assert!(tag.contains(&format!(r#"paperSize="{id}""#)), ".xls iPaperSize {id}: wrote <pageSetup{tag}>");
    }
}
