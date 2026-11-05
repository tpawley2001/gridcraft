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
fn image_save_preferences_survive_two_saves() {
    let bytes = package(r#"<workbookPr autoCompressPictures="0" discardImageEditData="1" defaultImageDpi="32767"/>"#, "<sheetData/>", &[], "");
    let out = save(&save(&bytes));
    let xml = part(&out, "xl/workbook.xml");
    let p = xml.child("workbookPr").unwrap();
    assert!(!p.flag("autoCompressPictures", true));
    assert!(p.flag("discardImageEditData", false));
    assert_eq!(p.attr_u32("defaultImageDpi"), Some(32767));
}
#[test]
fn external_link_update_policy_survives() {
    for policy in ["never", "always", "userSet"] {
        let bytes = package(&format!(r#"<workbookPr updateLinks="{policy}"/>"#), "<sheetData/>", &[], "");
        let xml = part(&save(&bytes), "xl/workbook.xml");
        assert_eq!(xml.child("workbookPr").unwrap().attr("updateLinks"), Some(policy));
    }
}
#[test]
fn object_and_pivot_display_preferences_survive() {
    let bytes = package(
        r#"<workbookPr showInkAnnotation="0" hidePivotFieldList="1" showPivotChartFilter="1" showBorderUnselectedTables="0"/>"#,
        "<sheetData/>",
        &[],
        "",
    );
    let xml = part(&save(&bytes), "xl/workbook.xml");
    let p = xml.child("workbookPr").unwrap();
    assert!(!p.flag("showInkAnnotation", true));
    assert!(p.flag("hidePivotFieldList", false));
    assert!(p.flag("showPivotChartFilter", false));
    assert!(!p.flag("showBorderUnselectedTables", true));
}
#[test]
fn compatibility_preferences_survive() {
    let bytes = package(r#"<workbookPr checkCompatibility="1" dateCompatibility="0"/>"#, "<sheetData/>", &[], "");
    let xml = part(&save(&bytes), "xl/workbook.xml");
    let p = xml.child("workbookPr").unwrap();
    assert!(p.flag("checkCompatibility", false));
    assert!(!p.flag("dateCompatibility", true));
}
