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

fn font_package(scheme: &str, cached: &str) -> Vec<u8> {
    let style = format!(
        r#"<styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="{cached}"/><scheme val="{scheme}"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="1"><xf fontId="0" fillId="0" borderId="0" numFmtId="0"/></cellXfs></styleSheet>"#
    );
    let mut wb = gridcraft_model::Workbook::new();
    wb.theme.major_font = "Heading Face".into();
    wb.theme.minor_font = "Body Face".into();
    let theme = crate::theme::write_theme(&wb.theme);
    let rels = format!(
        r#"<Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="theme" Type="{REL}/theme" Target="theme/theme1.xml"/>"#
    );
    package(
        "",
        r#"<sheetData><row r="1"><c r="A1" s="0"><v>1</v></c></row></sheetData>"#,
        &[("xl/styles.xml", style.as_bytes()), ("xl/theme/theme1.xml", theme.as_bytes())],
        &rels,
    )
}
fn cell_font(bytes: &[u8]) -> El {
    let sheet = part(bytes, "xl/worksheets/sheet1.xml");
    let c = sheet.path(&["sheetData", "row", "c"]).unwrap();
    let style = c.attr_u32("s").unwrap_or(0) as usize;
    let styles = part(bytes, "xl/styles.xml");
    let xf = styles.child("cellXfs").unwrap().kids("xf").nth(style).unwrap();
    let font = xf.attr_u32("fontId").unwrap_or(0) as usize;
    styles.child("fonts").unwrap().kids("font").nth(font).unwrap().clone()
}
#[test]
fn major_scheme_is_written_back() {
    let out = save(&save(&font_package("major", "Heading Face")));
    let f = cell_font(&out);
    assert_eq!(f.child_val("scheme"), Some("major"));
    assert_eq!(f.child_val("name"), Some("Heading Face"));
}
#[test]
fn stale_cached_heading_face_resolves_to_theme_heading_font() {
    let out = save(&font_package("major", "Stale Cached Heading"));
    let f = cell_font(&out);
    assert_eq!(f.child_val("scheme"), Some("major"));
    assert_eq!(f.child_val("name"), Some("Heading Face"));
}
