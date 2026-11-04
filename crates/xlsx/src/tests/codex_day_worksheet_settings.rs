//! Synthetic acceptance tests, planned by Codex.

use super::make_zip;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
fn package(wb_body: &str, sheet_body: &str, extra: &[(&str, &[u8])], wb_rels_extra: &str) -> Vec<u8> {
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#;
    let mut ct = ct
        .replace(r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#, "");
    ct.truncate(ct.len() - "</Types>".len());
    for (name, _) in extra {
        let ty = match *name {
            "xl/styles.xml" => "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
            "xl/theme/theme1.xml" => "application/vnd.openxmlformats-officedocument.theme+xml",
            "xl/tables/table1.xml" => "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
            "xl/revisions/revisionHeaders.xml" => "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionHeaders+xml",
            "xl/revisions/revisionLog1.xml" => "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionLog+xml",
            "xl/revisions/userNames.xml" => "application/vnd.openxmlformats-officedocument.spreadsheetml.userNames+xml",
            _ => continue,
        };
        ct.push_str(&format!(r#"<Override PartName="/{name}" ContentType="{ty}"/>"#));
    }
    ct.push_str("</Types>");
    let root = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let wb =
        format!(r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}">{wb_body}<sheets><sheet name="Data" sheetId="1" r:id="sheet"/></sheets></workbook>"#);
    let wr = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/>{wb_rels_extra}</Relationships>"#
    );
    let sheet = format!(r#"<worksheet xmlns="{MAIN}" xmlns:r="{REL}">{sheet_body}</worksheet>"#);
    let mut parts = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wr.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ];
    parts.extend_from_slice(extra);
    make_zip(&parts)
}
fn part(bytes: &[u8], name: &str) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut b = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut b).unwrap();
    parse(&b).unwrap()
}
fn save(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}

#[test]
fn hidden_default_does_not_expand_the_grid() {
    let bytes = package(
        "",
        r#"<sheetFormatPr defaultRowHeight="15" zeroHeight="1"/><sheetData><row r="2" hidden="0"><c r="A2"><v>7</v></c></row></sheetData>"#,
        &[],
        "",
    );
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let sh = wb.sheet(0).unwrap();
    assert!(sh.is_row_hidden(0));
    assert!(!sh.is_row_hidden(1));
    assert!(sh.is_row_hidden(1_048_575));
    assert!(sh.rows.len() < 10);
    let out = write_xlsx(&wb).unwrap();
    let xml = part(&out, "xl/worksheets/sheet1.xml");
    assert_eq!(xml.child("sheetFormatPr").unwrap().attr_bool("zeroHeight"), Some(true));
    let again = save(&out);
    assert_eq!(part(&again, "xl/worksheets/sheet1.xml").child("sheetFormatPr").unwrap().attr_bool("zeroHeight"), Some(true));
}
#[test]
fn explicit_custom_default_height_is_not_inferred_away() {
    let bytes = package("", r#"<sheetFormatPr defaultRowHeight="15" customHeight="1"/><sheetData/>"#, &[], "");
    let xml = part(&save(&bytes), "xl/worksheets/sheet1.xml");
    assert_eq!(xml.child("sheetFormatPr").unwrap().attr_bool("customHeight"), Some(true));
}
#[test]
fn protection_permission_bits_are_independent() {
    let bytes = package("", r#"<sheetData/><sheetProtection sheet="1" objects="0" scenarios="0" insertHyperlinks="0" pivotTables="0"/>"#, &[], "");
    let xml = part(&save(&bytes), "xl/worksheets/sheet1.xml");
    let p = xml.child("sheetProtection").unwrap();
    for key in ["objects", "scenarios", "insertHyperlinks", "pivotTables"] {
        assert!(!p.flag(key, true), "{key}");
    }
}
#[test]
fn inactive_protection_keeps_its_options_without_locking_the_sheet() {
    let bytes = package("", r#"<sheetData/><sheetProtection sheet="0" insertHyperlinks="0" pivotTables="0"/>"#, &[], "");
    let xml = part(&save(&bytes), "xl/worksheets/sheet1.xml");
    let p = xml.child("sheetProtection").unwrap();
    assert!(!p.flag("sheet", false));
    assert!(!p.flag("insertHyperlinks", true));
    assert!(!p.flag("pivotTables", true));
}
#[test]
fn validation_prompt_container_settings_survive() {
    let bytes = package(
        "",
        r#"<sheetData/><dataValidations count="1" disablePrompts="1" xWindow="91" yWindow="278"><dataValidation type="whole" sqref="A1" showInputMessage="1"><formula1>1</formula1><formula2>9</formula2></dataValidation></dataValidations>"#,
        &[],
        "",
    );
    let xml = part(&save(&bytes), "xl/worksheets/sheet1.xml");
    let v = xml.child("dataValidations").unwrap();
    assert_eq!(v.attr_bool("disablePrompts"), Some(true));
    assert_eq!(v.attr_u32("xWindow"), Some(91));
    assert_eq!(v.attr_u32("yWindow"), Some(278));
    assert_eq!(v.attr_u32("count"), Some(v.kids("dataValidation").count() as u32));
    assert!(v.child("dataValidation").unwrap().flag("showInputMessage", false));
}
#[test]
fn explicit_prompt_position_is_preserved_even_when_prompts_are_enabled() {
    let bytes = package(
        "",
        r#"<sheetData/><dataValidations count="1" disablePrompts="0" xWindow="91" yWindow="278"><dataValidation type="list" sqref="A1"><formula1>"one,two"</formula1></dataValidation></dataValidations>"#,
        &[],
        "",
    );
    let xml = part(&save(&bytes), "xl/worksheets/sheet1.xml");
    let v = xml.child("dataValidations").unwrap();
    assert!(!v.flag("disablePrompts", false));
    assert_eq!(v.attr_u32("xWindow"), Some(91));
    assert_eq!(v.attr_u32("yWindow"), Some(278));
}
