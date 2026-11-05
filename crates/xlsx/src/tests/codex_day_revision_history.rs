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

fn revisions() -> Vec<u8> {
    let headers = format!(
        r#"<headers xmlns="{MAIN}" xmlns:r="{REL}" guid="{{11111111-1111-1111-1111-111111111111}}" lastGuid="{{22222222-2222-2222-2222-222222222222}}" shared="1"><header guid="{{22222222-2222-2222-2222-222222222222}}" dateTime="2026-10-01T00:00:00Z" userName="Ada &amp; Bo" r:id="log" minRId="1" maxRId="1" maxSheetId="2"><sheetIdMap count="1"><sheetId val="1"/></sheetIdMap></header></headers>"#
    );
    let log = format!(r#"<revisions xmlns="{MAIN}"><rcc rId="1" sId="1"><oc r="A1"><v>1</v></oc><nc r="A1"><v>2</v></nc></rcc></revisions>"#);
    let hr = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="log" Type="{REL}/revisionLog" Target="revisionLog1.xml"/></Relationships>"#
    );
    let users = format!(r#"<users xmlns="{MAIN}" count="0"/>"#);
    let rels = format!(
        r#"<Relationship Id="history" Type="{REL}/revisionHeaders" Target="revisions/revisionHeaders.xml"/><Relationship Id="users" Type="{REL}/usernames" Target="revisions/userNames.xml"/>"#
    );
    package(
        "",
        r#"<sheetData><row r="1"><c r="A1"><v>2</v></c></row></sheetData>"#,
        &[
            ("xl/revisions/revisionHeaders.xml", headers.as_bytes()),
            ("xl/revisions/revisionLog1.xml", log.as_bytes()),
            ("xl/revisions/_rels/revisionHeaders.xml.rels", hr.as_bytes()),
            ("xl/revisions/userNames.xml", users.as_bytes()),
        ],
        &rels,
    )
}
fn resolved(owner: &str, target: &str) -> String {
    let joined = if target.starts_with('/') { target.to_string() } else { format!("{}/{target}", owner.rsplit_once('/').unwrap().0) };
    let mut segs = Vec::new();
    for s in joined.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                segs.pop();
            }
            _ => segs.push(s),
        }
    }
    segs.join("/")
}
fn relationship(bytes: &[u8], owner: &str, kind: &str) -> String {
    let (dir, file) = owner.rsplit_once('/').unwrap();
    let rels = part(bytes, &format!("{dir}/_rels/{file}.rels"));
    let ty = format!("{REL}/{kind}");
    let rel = rels.kids("Relationship").find(|r| r.attr("Type") == Some(ty.as_str())).unwrap();
    resolved(owner, rel.attr("Target").unwrap())
}
#[test]
fn revision_graph_authors_and_old_new_values_survive() {
    let out = save(&revisions());
    let h = relationship(&out, "xl/workbook.xml", "revisionHeaders");
    let root = part(&out, &h);
    let header = root.child("header").unwrap();
    assert_eq!(header.attr("userName"), Some("Ada & Bo"));
    assert_eq!(header.attr_u32("minRId"), Some(1));
    let log = relationship(&out, &h, "revisionLog");
    let revisions = part(&out, &log);
    let c = revisions.child("rcc").unwrap();
    assert_eq!(c.path(&["oc", "v"]).unwrap().text, "1");
    assert_eq!(c.path(&["nc", "v"]).unwrap().text, "2");
    let users = relationship(&out, "xl/workbook.xml", "usernames");
    assert_eq!(part(&out, &users).name, "users");
}
#[test]
fn ordinary_cell_edit_keeps_existing_history_after_two_saves() {
    let (mut wb, _) = read_xlsx(&revisions()).unwrap();
    wb.sheet_mut(0).unwrap().cells.set(gridcraft_core::CellRef::new(1, 0), gridcraft_model::Cell::value(gridcraft_core::Value::Number(3.0)));
    let out = save(&write_xlsx(&wb).unwrap());
    let h = relationship(&out, "xl/workbook.xml", "revisionHeaders");
    let header = part(&out, &h);
    assert_eq!(header.child("header").unwrap().attr("userName"), Some("Ada & Bo"));
    let sheet = part(&out, "xl/worksheets/sheet1.xml");
    let row = sheet.child("sheetData").unwrap().kids("row").find(|r| r.attr("r") == Some("2")).unwrap();
    assert_eq!(row.child("c").unwrap().child("v").unwrap().text, "3");
}
