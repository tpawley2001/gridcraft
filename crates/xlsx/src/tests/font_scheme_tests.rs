//! Theme-linked fonts (`<scheme val="major|minor|none"/>`, ECMA-376 §18.8.35) on cell, row, column
//! and named-style fonts, and the controls that must keep their stated face.

use super::{CT, ROOT_RELS, make_zip};
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use gridcraft_core::CellRef;
use gridcraft_model::Workbook;
use std::io::Read;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Fonts, in order: body-linked, heading-linked, then the controls.
const FONTS: [(&str, Option<&str>); 7] = [
    ("Body Face", Some("minor")),
    ("Heading Face", Some("major")),
    ("Heading Face", Some("none")),
    ("Heading Face", None),
    ("Heading Face", Some("bogus")),
    ("Stale Body", Some("minor")),
    ("Explicit Face", Some("none")),
];

fn book() -> Vec<u8> {
    let mut fonts = String::new();
    let mut xfs = String::new();
    let mut cells = String::new();
    for (i, (name, scheme)) in FONTS.iter().enumerate() {
        let sc = scheme.map(|v| format!(r#"<scheme val="{v}"/>"#)).unwrap_or_default();
        fonts.push_str(&format!(r#"<font><sz val="11"/><name val="{name}"/>{sc}</font>"#));
        xfs.push_str(&format!(r#"<xf numFmtId="0" fontId="{i}" fillId="0" borderId="0" xfId="0"/>"#));
        cells.push_str(&format!(r#"<c r="{}1" s="{i}"><v>1</v></c>"#, (b'A' + i as u8) as char));
    }
    let styles = format!(
        r#"<styleSheet xmlns="{MAIN}"><fonts count="7">{fonts}</fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="7">{xfs}</cellXfs><cellStyles count="2"><cellStyle name="Normal" xfId="0" builtinId="0"/><cellStyle name="Big Title" xfId="1"/></cellStyles></styleSheet>"#
    );
    let mut wb = Workbook::new();
    wb.theme.major_font = "Heading Face".into();
    wb.theme.minor_font = "Body Face".into();
    let theme = crate::theme::write_theme(&wb.theme);
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><cols><col min="10" max="10" width="9" style="1" customWidth="1"/></cols><sheetData><row r="1">{cells}</row><row r="3" s="1" customFormat="1"/></sheetData></worksheet>"#
    );
    let workbook = format!(r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Data" sheetId="1" r:id="s1"/></sheets></workbook>"#);
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="st" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="th" Type="{REL}/theme" Target="theme/theme1.xml"/></Relationships>"#
    );
    make_zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/styles.xml", styles.as_bytes()),
        ("xl/theme/theme1.xml", theme.as_bytes()),
    ])
}

fn part(bytes: &[u8], name: &str) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut b = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut b).unwrap();
    parse(&b).unwrap()
}

fn save(bytes: &[u8]) -> Vec<u8> {
    write_xlsx(&read_xlsx(bytes).unwrap().0).unwrap()
}

/// (name, scheme) of the saved font that cell format `xf` of `list` (`cellXfs` / `cellStyleXfs`) uses.
fn font_of(styles: &El, list: &str, xf: usize) -> (String, Option<String>) {
    let xf = styles.child(list).unwrap().kids("xf").nth(xf).unwrap();
    let font = styles.child("fonts").unwrap().kids("font").nth(xf.attr_u32("fontId").unwrap_or(0) as usize).unwrap();
    (font.child_val("name").unwrap().to_string(), font.child_val("scheme").map(str::to_string))
}

fn cell_font(bytes: &[u8], cell: &str) -> (String, Option<String>) {
    let sheet = part(bytes, "xl/worksheets/sheet1.xml");
    let rows = sheet.child("sheetData").unwrap();
    let c = rows.kids("row").flat_map(|r| r.kids("c")).find(|c| c.attr("r") == Some(cell)).unwrap();
    font_of(&part(bytes, "xl/styles.xml"), "cellXfs", c.attr_u32("s").unwrap_or(0) as usize)
}

fn pair(name: &str, scheme: Option<&str>) -> (String, Option<String>) {
    (name.to_string(), scheme.map(str::to_string))
}

#[test]
fn cell_fonts_keep_their_scheme_distinctions_after_two_saves() {
    let out = save(&save(&book()));
    assert_eq!(cell_font(&out, "A1"), pair("Body Face", Some("minor")));
    assert_eq!(cell_font(&out, "B1"), pair("Heading Face", Some("major")));
    // The heading face stated explicitly (scheme none, missing or unknown) is not linked to the theme.
    assert_eq!(cell_font(&out, "C1"), pair("Heading Face", None));
    assert_eq!(cell_font(&out, "D1"), pair("Heading Face", None));
    assert_eq!(cell_font(&out, "E1"), pair("Heading Face", None));
    // A stale cached body face resolves to the theme's; an explicit face keeps its name.
    assert_eq!(cell_font(&out, "F1"), pair("Body Face", Some("minor")));
    assert_eq!(cell_font(&out, "G1"), pair("Explicit Face", None));
}

#[test]
fn row_column_and_named_style_fonts_keep_the_heading_link() {
    let out = save(&save(&book()));
    let sheet = part(&out, "xl/worksheets/sheet1.xml");
    let styles = part(&out, "xl/styles.xml");
    let row = sheet.child("sheetData").unwrap().kids("row").find(|r| r.attr("r") == Some("3")).unwrap();
    assert_eq!(font_of(&styles, "cellXfs", row.attr_u32("s").unwrap() as usize), pair("Heading Face", Some("major")));
    let col = sheet.child("cols").unwrap().kids("col").find(|c| c.attr("min") == Some("10")).unwrap();
    assert_eq!(font_of(&styles, "cellXfs", col.attr_u32("style").unwrap() as usize), pair("Heading Face", Some("major")));
    let named = styles.child("cellStyles").unwrap().kids("cellStyle").find(|c| c.attr("name") == Some("Big Title")).unwrap();
    assert_eq!(font_of(&styles, "cellStyleXfs", named.attr_u32("xfId").unwrap() as usize), pair("Heading Face", Some("major")));
}

#[test]
fn bold_keeps_the_heading_link_and_a_new_face_ends_it() {
    let (mut wb, _) = read_xlsx(&book()).unwrap();
    let b1 = CellRef { row: 0, col: 1 };
    let sheet = wb.sheets.first().unwrap();
    let mut bold = wb.styles.get(sheet.style_id(b1)).clone();
    bold.font.bold = true;
    let mut renamed = bold.clone();
    renamed.font.name = "Other Face".into();
    let (bold, renamed) = (wb.styles.intern(bold), wb.styles.intern(renamed));
    let sheet = std::sync::Arc::make_mut(wb.sheets.first_mut().unwrap());
    sheet.set_style(b1, bold);
    sheet.set_style(CellRef { row: 0, col: 2 }, renamed);
    let out = save(&write_xlsx(&wb).unwrap());
    assert_eq!(cell_font(&out, "B1"), pair("Heading Face", Some("major")));
    assert_eq!(cell_font(&out, "C1"), pair("Other Face", None));
}

#[test]
fn without_a_theme_part_the_cached_face_is_kept() {
    let styles = format!(
        r#"<styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="Cached Heading"/><scheme val="major"/></font></fonts><cellXfs count="1"><xf fontId="0"/></cellXfs></styleSheet>"#
    );
    let bytes = super::minimal(
        r#"<sheetData><row r="1"><c r="A1" s="0"><v>1</v></c></row></sheetData>"#,
        &[("xl/styles.xml", &styles)],
        &format!(r#"<Relationship Id="st" Type="{REL}/styles" Target="styles.xml"/>"#),
        "",
    );
    let (wb, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(wb.styles.get(gridcraft_model::StyleId::DEFAULT).font.name, "Cached Heading");
}
