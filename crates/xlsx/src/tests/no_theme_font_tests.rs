//! A package with no theme part: scheme-linked fonts take Excel's built-in default theme faces
//! (body "Aptos Narrow", heading "Aptos Display"); fonts without a scheme link keep their face.

use super::{CT, ROOT_RELS, make_zip};
use crate::read_xlsx;
use gridcraft_core::CellRef;
use gridcraft_model::Workbook;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Fonts: body-linked Calibri (A1), heading-linked Calibri Light (B1), plain Calibri (C1).
fn book(with_theme: bool) -> Vec<u8> {
    let styles = format!(
        r#"<styleSheet xmlns="{MAIN}"><fonts count="3"><font><sz val="11"/><name val="Calibri"/><scheme val="minor"/></font><font><sz val="18"/><name val="Calibri Light"/><scheme val="major"/></font><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/><xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#
    );
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" s="0"><v>1</v></c><c r="B1" s="1"><v>2</v></c><c r="C1" s="2"><v>3</v></c></row></sheetData></worksheet>"#
    );
    let workbook = format!(r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Data" sheetId="1" r:id="s1"/></sheets></workbook>"#);
    let theme_rel = if with_theme { format!(r#"<Relationship Id="th" Type="{REL}/theme" Target="theme/theme1.xml"/>"#) } else { String::new() };
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="st" Type="{REL}/styles" Target="styles.xml"/>{theme_rel}</Relationships>"#
    );
    let mut t = Workbook::new();
    t.theme.major_font = "Heading Face".into();
    t.theme.minor_font = "Body Face".into();
    let theme = crate::theme::write_theme(&t.theme);
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/styles.xml", styles.as_bytes()),
    ];
    if with_theme {
        parts.push(("xl/theme/theme1.xml", theme.as_bytes()));
    }
    make_zip(&parts)
}

fn face(wb: &Workbook, cell: &str) -> String {
    let at = CellRef::parse(cell).expect("cell");
    let sheet = &wb.sheets[0];
    wb.styles.get(sheet.style_id(at)).font.name.clone()
}

#[test]
fn no_theme_part_takes_default_theme_faces() {
    let wb = read_xlsx(&book(false)).expect("read").0;
    assert_eq!(wb.theme.minor_font, "Aptos Narrow");
    assert_eq!(wb.theme.major_font, "Aptos Display");
    assert_eq!(face(&wb, "A1"), "Aptos Narrow", "scheme minor follows the body face");
    assert_eq!(face(&wb, "B1"), "Aptos Display", "scheme major follows the heading face");
    assert_eq!(face(&wb, "C1"), "Calibri", "a font without a scheme link keeps its face");
    // Everything else stays at the default theme.
    let d = gridcraft_model::Theme::default();
    assert_eq!(wb.theme.colors, d.colors);
    assert_eq!(wb.theme.minor_font_ea, d.minor_font_ea);
    assert_eq!(wb.theme.major_font_cs, d.major_font_cs);
}

#[test]
fn theme_part_keeps_its_own_faces() {
    let wb = read_xlsx(&book(true)).expect("read").0;
    assert_eq!(wb.theme.minor_font, "Body Face");
    assert_eq!(wb.theme.major_font, "Heading Face");
    assert_eq!(face(&wb, "A1"), "Body Face");
    assert_eq!(face(&wb, "B1"), "Heading Face");
    assert_eq!(face(&wb, "C1"), "Calibri");
}

#[test]
fn no_theme_faces_survive_a_save() {
    let wb = read_xlsx(&book(false)).expect("read").0;
    let bytes = crate::write_xlsx(&wb).expect("write");
    let back = read_xlsx(&bytes).expect("reread").0;
    assert_eq!(back.theme.minor_font, "Aptos Narrow");
    assert_eq!(back.theme.major_font, "Aptos Display");
    assert_eq!(face(&back, "A1"), "Aptos Narrow");
    assert_eq!(face(&back, "B1"), "Aptos Display");
    assert_eq!(face(&back, "C1"), "Calibri");
}
