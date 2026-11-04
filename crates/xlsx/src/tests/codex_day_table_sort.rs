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

fn table_package(sort: &str, nested: bool) -> Vec<u8> {
    let table = format!(
        r#"<table xmlns="{MAIN}" id="1" name="Sales" displayName="Sales" ref="A1:B3" totalsRowShown="0"><autoFilter ref="A1:B3">{}</autoFilter>{}<tableColumns count="2"><tableColumn id="1" name="Name"/><tableColumn id="2" name="Amount"/></tableColumns></table>"#,
        if nested { sort } else { "" },
        if nested { "" } else { sort }
    );
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="table" Type="{REL}/table" Target="../tables/table1.xml"/></Relationships>"#
    );
    package(
        "",
        r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row><row r="2"><c r="B2"><v>2</v></c></row><row r="3"><c r="B3"><v>1</v></c></row></sheetData><tableParts count="1"><tablePart r:id="table"/></tableParts>"#,
        &[("xl/tables/table1.xml", table.as_bytes()), ("xl/worksheets/_rels/sheet1.xml.rels", rels.as_bytes())],
        "",
    )
}
#[test]
fn table_multikey_sort_order_and_custom_list_survive() {
    let bytes = table_package(
        r#"<sortState ref="A2:B3" caseSensitive="1" sortMethod="stroke"><sortCondition ref="A2:A3" customList="High,Medium,Low"/><sortCondition ref="B2:B3" descending="1"/></sortState>"#,
        false,
    );
    let xml = part(&save(&bytes), "xl/tables/table1.xml");
    let s = xml.child("sortState").unwrap();
    assert_eq!(s.attr("ref"), Some("A2:B3"));
    assert!(s.flag("caseSensitive", false));
    assert_eq!(s.attr("sortMethod"), Some("stroke"));
    let c: Vec<_> = s.kids("sortCondition").collect();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].attr("ref"), Some("A2:A3"));
    assert_eq!(c[0].attr("customList"), Some("High,Medium,Low"));
    assert_eq!(c[1].attr("ref"), Some("B2:B3"));
    assert!(c[1].flag("descending", false));
}
#[test]
fn autofilter_sort_stays_in_its_slot_after_two_saves() {
    let bytes = table_package(r#"<sortState ref="A2:B3" columnSort="0"><sortCondition ref="B2:B3" descending="1"/></sortState>"#, true);
    let xml = part(&save(&save(&bytes)), "xl/tables/table1.xml");
    assert!(xml.child("sortState").is_none());
    let s = xml.path(&["autoFilter", "sortState"]).unwrap();
    assert_eq!(s.attr("ref"), Some("A2:B3"));
    assert!(s.child("sortCondition").unwrap().flag("descending", false));
}
