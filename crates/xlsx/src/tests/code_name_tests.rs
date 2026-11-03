//! VBA code names of sheets (`<sheetPr codeName>`, CodeName, BrtWsProp) and of the workbook
//! (`<workbookPr codeName>`, CodeName in the globals, BrtWbProp) survive open and save, so a kept
//! VBA project's document modules still match their sheets.

use gridcraft_model::{Color, Workbook};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, xlstr};
use super::xlsb_tests::{RELS, ROOT, TYPES, record, sst, wide};
use super::{CT, ROOT_RELS, make_zip};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// A two-sheet package with the given `<sheetPr>` of each sheet and `<workbookPr>`.
fn xlsx(pr1: &str, pr2: &str, wb_pr: &str) -> Vec<u8> {
    let wb = format!(
        r#"<?xml version="1.0"?><workbook {NS}>{wb_pr}<sheets><sheet name="One" sheetId="1" r:id="rId1"/><sheet name="Two" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#;
    let sheet = |pr: &str| format!(r#"<?xml version="1.0"?><worksheet {NS}>{pr}<sheetData/></worksheet>"#);
    let (s1, s2) = (sheet(pr1), sheet(pr2));
    make_zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", s1.as_bytes()),
        ("xl/worksheets/sheet2.xml", s2.as_bytes()),
    ])
}

fn part(bytes: &[u8], name: &str) -> String {
    String::from_utf8(crate::package::Package::open(bytes).unwrap().read(name).unwrap().unwrap()).unwrap()
}

fn names(wb: &Workbook) -> (Option<&str>, Vec<Option<&str>>) {
    (wb.code_name.as_deref(), wb.sheets.iter().map(|s| s.code_name.as_deref()).collect())
}

fn reopen(wb: &Workbook) -> Workbook {
    read_xlsx(&write_xlsx(wb).unwrap()).unwrap().0
}

#[test]
fn xlsx_code_names_survive_a_save() {
    let file = xlsx(
        r#"<sheetPr codeName="Sheet1"/>"#,
        r#"<sheetPr codeName="Data"><tabColor rgb="FFFF0000"/></sheetPr>"#,
        r#"<workbookPr codeName="ThisWorkbook"/>"#,
    );
    let (wb, _) = read_xlsx(&file).unwrap();
    assert_eq!(names(&wb), (Some("ThisWorkbook"), vec![Some("Sheet1"), Some("Data")]));
    let red = Some(Color::Rgb(0xFF0000));
    assert_eq!(wb.sheets[1].tab_color, red);
    let again = reopen(&wb);
    assert_eq!(names(&again), names(&wb));
    assert_eq!(again.sheets[1].tab_color, red);
}

#[test]
fn a_sheet_without_a_code_name_gets_none() {
    let (wb, _) = read_xlsx(&xlsx("", "<sheetPr/>", "<workbookPr/>")).unwrap();
    assert_eq!(names(&wb), (None, vec![None, None]));
    let out = write_xlsx(&wb).unwrap();
    for p in ["xl/workbook.xml", "xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml"] {
        assert!(!part(&out, p).contains("codeName"), "{p}");
    }
    assert!(!part(&out, "xl/worksheets/sheet1.xml").contains("<sheetPr"));
}

#[test]
fn a_code_name_with_an_ampersand_is_escaped() {
    let mut wb = Workbook::new();
    wb.code_name = Some("Book&Co".into());
    std::sync::Arc::make_mut(&mut wb.sheets[0]).code_name = Some("A&B".into());
    let out = write_xlsx(&wb).unwrap();
    assert!(part(&out, "xl/worksheets/sheet1.xml").contains(r#"<sheetPr codeName="A&amp;B">"#));
    assert!(part(&out, "xl/workbook.xml").contains(r#"codeName="Book&amp;Co""#));
    let (again, _) = read_xlsx(&out).unwrap();
    assert_eq!(names(&again), (Some("Book&Co"), vec![Some("A&B")]));
}

#[test]
fn xls_code_names_are_read_and_kept_in_xlsx() {
    let code = |name: &str| rec(0x01BA, &xlstr(name));
    let globals = |pos: u32| cat(&[&bof(0x0005), &code("ThisWorkbook"), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &eof()]);
    let sheet = cat(&[&bof(0x0010), &code("Sheet1"), &rec(0x0203, &cat(&[&cell(0, 0, 0), &7.0f64.to_le_bytes()])), &eof()]);
    let file = compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]));
    let (wb, _) = read_xls(&file).unwrap();
    assert_eq!(names(&wb), (Some("ThisWorkbook"), vec![Some("Sheet1")]));
    assert_eq!(names(&reopen(&wb)), names(&wb));
}

#[test]
fn xlsb_code_names_are_read_and_kept_in_xlsx() {
    let mut book = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide("ThisWorkbook"));
    book.extend(record(153, &props));
    book.extend(record(143, &[]));
    for (id, rel, name) in [(1u32, "one", "Data"), (2, "two", "Other")] {
        let mut data = 0u32.to_le_bytes().to_vec();
        data.extend(id.to_le_bytes());
        data.extend(wide(rel));
        data.extend(wide(name));
        book.extend(record(156, &data));
    }
    book.extend(record(144, &[]));
    book.extend(record(132, &[]));
    // BrtWsProp: flags with fFitToPage, a red RGB tab colour, rwSync, colSync, then strName.
    let ws_prop = |name: Option<&str>| {
        let mut out = vec![0xc0, 0x05, 0x00];
        out.extend([(2 << 1) | 1, 0, 0, 0, 0xFF, 0, 0, 0xFF]);
        out.extend(u32::MAX.to_le_bytes());
        out.extend(u32::MAX.to_le_bytes());
        out.extend(name.map_or_else(|| u32::MAX.to_le_bytes().to_vec(), wide));
        record(147, &out)
    };
    let sheet = |name: Option<&str>| [record(129, &[]), ws_prop(name), record(145, &[]), record(146, &[]), record(130, &[])].concat();
    let file = make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", &sheet(Some("Sheet1"))),
        ("data/two.bin", &sheet(None)),
        ("text/strings.bin", &sst()),
    ]);
    let (wb, _) = read_xlsb(&file).unwrap();
    assert_eq!(names(&wb), (Some("ThisWorkbook"), vec![Some("Sheet1"), None]));
    assert_eq!(wb.sheets[0].tab_color, Some(Color::Rgb(0xFF0000)));
    assert!(wb.sheets[0].print.fit_to.is_some());
    assert_eq!(names(&reopen(&wb)), names(&wb));
}
