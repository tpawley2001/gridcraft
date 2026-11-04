//! Negative and repeated-save coverage for worksheet settings (ECMA-376 §18.3.1).
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;

fn fixture(body: &str) -> Vec<u8> {
    let sheet = format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{body}</worksheet>"#);
    super::make_zip(&[
        ("xl/workbook.xml", br#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="s"/></sheets></workbook>"#),
        ("xl/_rels/workbook.xml.rels", br#"<Relationships><Relationship Id="s" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}
fn sheet_xml(bytes: &[u8], number: u32) -> El {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = Vec::new();
    zip.by_name(&format!("xl/worksheets/sheet{number}.xml")).unwrap().read_to_end(&mut xml).unwrap();
    parse(&xml).unwrap()
}
fn twice(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    let first = write_xlsx(&wb).unwrap();
    let (wb, _) = read_xlsx(&first).unwrap();
    write_xlsx(&wb).unwrap()
}

#[test]
fn visible_exceptions_and_false_defaults_survive_two_saves() {
    for hidden in [false, true] {
        let bytes = fixture(&format!(
            r#"<sheetFormatPr defaultRowHeight="15" zeroHeight="{}" customHeight="1"/><sheetData><row r="2" hidden="0"/><row r="4" hidden="1"/></sheetData>"#,
            u8::from(hidden)
        ));
        let out = twice(&bytes);
        let (wb, _) = read_xlsx(&out).unwrap();
        let sh = wb.sheet(0).unwrap();
        assert_eq!(sh.is_row_hidden(0), hidden);
        assert!(!sh.is_row_hidden(1));
        assert!(sh.is_row_hidden(3));
        assert_eq!(sh.row_height(0), if hidden { 0.0 } else { 20.0 });
        assert_eq!(sh.row_height(1), 20.0);
        if hidden {
            assert_eq!(sh.row_top(1), 0.0);
            assert_eq!(sh.row_top(2), 20.0);
            assert_eq!(sh.row_at(1.0), 1);
        }
        assert!(sh.rows.len() <= 2);
        let xml = sheet_xml(&out, 1);
        let format = xml.child("sheetFormatPr").unwrap();
        assert_eq!(format.attr_bool("zeroHeight"), Some(hidden));
        assert_eq!(format.attr_bool("customHeight"), Some(true));
        if hidden {
            let row = xml.child("sheetData").unwrap().kids("row").find(|r| r.attr_u32("r") == Some(2)).unwrap();
            assert_eq!(row.attr_bool("hidden"), Some(false));
        }
    }
}

#[test]
fn inactive_password_and_schema_permission_defaults_do_not_protect_cells() {
    for sheet in ["", r#" sheet="0""#] {
        let bytes = fixture(&format!(r#"<sheetData/><sheetProtection{sheet} password="ABCD" insertHyperlinks="0" pivotTables="1"/>"#));
        let out = twice(&bytes);
        let (wb, _) = read_xlsx(&out).unwrap();
        let sh = wb.sheet(0).unwrap();
        assert!(!sh.is_protected());
        assert!(sh.protection.is_none());
        let xml = sheet_xml(&out, 1);
        let p = xml.child("sheetProtection").unwrap();
        assert_eq!(p.attr_bool("sheet"), Some(false));
        assert_eq!(p.attr("password"), Some("ABCD"));
        assert!(!p.flag("objects", false));
        assert!(!p.flag("scenarios", false));
        assert!(!p.flag("insertHyperlinks", true));
        assert!(p.flag("pivotTables", true));
        assert!(p.flag("formatCells", true));
        assert!(!p.flag("selectLockedCells", false));
    }
}

#[test]
fn hostile_prompt_positions_are_dropped_and_count_uses_surviving_rules() {
    for value in ["-1", "4294967296", "1.5", "NaN", "9999999999999999999999999999999999"] {
        let bytes = fixture(&format!(
            r#"<sheetData/><dataValidations count="4294967295" disablePrompts="0" xWindow="{value}" yWindow="4294967295"><dataValidation type="whole" sqref="A1" showInputMessage="0"><formula1>1</formula1></dataValidation><dataValidation type="whole" sqref="bad"><formula1>1</formula1></dataValidation></dataValidations>"#
        ));
        let out = twice(&bytes);
        let xml = sheet_xml(&out, 1);
        let v = xml.child("dataValidations").unwrap();
        assert_eq!(v.attr("xWindow"), None);
        assert_eq!(v.attr_u32("yWindow"), Some(u32::MAX));
        assert_eq!(v.attr_bool("disablePrompts"), Some(false));
        assert_eq!(v.attr_u32("count"), Some(1));
        assert_eq!(v.kids("dataValidation").count(), 1);
        assert!(!v.child("dataValidation").unwrap().flag("showInputMessage", false));
    }
}

#[test]
fn settings_are_per_sheet_and_new_files_do_not_invent_positions() {
    let bytes = fixture(
        r#"<sheetFormatPr defaultRowHeight="15" zeroHeight="1" customHeight="0"/><sheetData/><dataValidations count="0" disablePrompts="1" xWindow="0" yWindow="0"/>"#,
    );
    let (mut wb, _) = read_xlsx(&bytes).unwrap();
    let mut second = gridcraft_model::Sheet::new("Other");
    second.validations = read_xlsx(&fixture(
        r#"<sheetData/><dataValidations><dataValidation type="list" sqref="A1"><formula1>"a,b"</formula1></dataValidation></dataValidations>"#,
    ))
    .unwrap()
    .0
    .sheet(0)
    .unwrap()
    .validations
    .clone();
    wb.sheets.push(second.into());
    let out = twice(&write_xlsx(&wb).unwrap());
    let first = sheet_xml(&out, 1);
    let second = sheet_xml(&out, 2);
    assert_eq!(first.child("sheetFormatPr").unwrap().attr_bool("zeroHeight"), Some(true));
    assert_eq!(first.child("sheetFormatPr").unwrap().attr_bool("customHeight"), Some(false));
    assert_eq!(first.child("dataValidations").unwrap().attr_u32("count"), Some(0));
    assert!(!second.child("sheetFormatPr").unwrap().flag("zeroHeight", false));
    let validation = second.child("dataValidations").unwrap();
    for attr in ["disablePrompts", "xWindow", "yWindow"] {
        assert_eq!(validation.attr(attr), None);
    }
}

#[test]
fn inactive_iterated_password_metadata_survives() {
    let (mut wb, _) = read_xlsx(&fixture("<sheetData/>")).unwrap();
    let password = gridcraft_model::PasswordHash::new_sha512("test", b"0123456789abcdef");
    wb.sheet_mut(0).unwrap().inactive_protection = Some(gridcraft_model::SheetProtection { password: Some(password.clone()), ..Default::default() });
    let out = twice(&write_xlsx(&wb).unwrap());
    let (again, _) = read_xlsx(&out).unwrap();
    let sheet = again.sheet(0).unwrap();
    assert!(!sheet.is_protected());
    assert_eq!(sheet.inactive_protection.as_ref().unwrap().password, Some(password));
    let xml = sheet_xml(&out, 1);
    let p = xml.child("sheetProtection").unwrap();
    assert_eq!(p.attr_bool("sheet"), Some(false));
    assert_eq!(p.attr("algorithmName"), Some("SHA-512"));
    for attr in ["hashValue", "saltValue", "spinCount"] {
        assert!(p.attr(attr).is_some());
    }
}
