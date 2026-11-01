//! Synthetic packages built from the public XLSB and DrawingML layouts.
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Color, Theme};

use super::xlsb_tests::{RELS, ROOT, TYPES, book, cell, font, record, row, sst, worksheet, xf_full};
use super::{make_zip, minimal};
use crate::{read_xlsb, read_xlsx, write_xlsx};

const THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Synthetic"><a:themeElements><a:clrScheme name="Synthetic"><a:accent1><a:srgbClr val="FF6600"/></a:accent1></a:clrScheme><a:fontScheme name="Synthetic"><a:majorFont><a:latin typeface="Synthetic Heading"/></a:majorFont><a:minorFont><a:latin typeface="Calibri Light"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;
const THEME_REL: &str =
    r#"<Relationship Id="theme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/custom.xml"/>"#;

fn package(theme_rel: bool, theme: Option<&[u8]>, tab: [u8; 8]) -> Vec<u8> {
    let rels = RELS.replace("</Relationships>", &format!(
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="../text/styles.bin"/>{}</Relationships>"#,
        if theme_rel { THEME_REL } else { "" }
    ));
    let types = TYPES.replace("</Types>", r#"<Override PartName="/theme/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/text/styles.bin" ContentType="application/vnd.ms-excel.styles"/></Types>"#);
    let mut styles = record(278, &[]);
    styles.extend(record(611, &1u32.to_le_bytes()));
    styles.extend(font(220, 0, 400, 0, 0, 2, 0, [6, 4, 0, 0, 0, 0, 0, 0], 2, "Calibri"));
    styles.extend(record(612, &[]));
    styles.extend(record(617, &1u32.to_le_bytes()));
    styles.extend(xf_full(0, 0, 0, 0, 0, 2 << 3 | 1 << 12));
    styles.extend(record(618, &[]));
    styles.extend(record(279, &[]));
    let mut data = row(0);
    data.extend(cell(5, 0, &42.0f64.to_le_bytes()));
    let mut prop = vec![0; 3];
    prop.extend(tab);
    prop.extend([0; 8]); // Remaining BrtWsProp fields are not used here.
    let sheet = worksheet(&data, &record(147, &prop));
    let (book, two, strings) = (book(false), worksheet(&[], &[]), sst());
    let mut parts = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", rels.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
        ("text/styles.bin", &styles),
    ];
    if let Some(theme) = theme {
        parts.push(("theme/custom.xml", theme));
    }
    make_zip(&parts)
}

#[test]
fn xlsb_theme_matches_xlsx_and_resolves_font_colour() {
    let (wb, report) = read_xlsb(&package(true, Some(THEME.as_bytes()), [0; 8])).unwrap();
    let xlsx_rel = THEME_REL.replace("../theme/custom.xml", "theme/custom.xml");
    let xlsx = minimal("", &[("xl/theme/custom.xml", THEME)], &xlsx_rel, "");
    let (reference, _) = read_xlsx(&xlsx).unwrap();
    assert_eq!(wb.theme, reference.theme);
    assert_eq!(wb.theme.colors[4], 0xFF6600);
    assert_eq!(wb.theme.minor_font, "Calibri Light");
    let font = &wb.styles.get(wb.sheets[0].style_id(CellRef::new(0, 0))).font;
    assert_eq!(font.color, Color::Theme(4, 0));
    assert_eq!(font.color.resolve(&wb.theme), Some([0xFF, 0x66, 0x00]));
    assert!(!report.warnings.iter().any(|w| w.contains("theme")));
}

#[test]
fn xlsb_missing_theme_relationship_keeps_defaults_without_theme_warning() {
    let (wb, report) = read_xlsb(&package(false, Some(THEME.as_bytes()), [0; 8])).unwrap();
    assert_eq!(wb.theme, Theme::default());
    assert!(!report.warnings.iter().any(|w| w.contains("theme")));
}

#[test]
fn xlsb_unreadable_theme_keeps_cells_and_warns_once() {
    let oversized = vec![b' '; 1024 * 1024 + 1];
    for theme in [None, Some(b"not XML".as_slice()), Some(b"<".as_slice()), Some(oversized.as_slice())] {
        let (wb, report) = read_xlsb(&package(true, theme, [0; 8])).unwrap();
        assert_eq!(wb.theme, Theme::default());
        assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
        assert_eq!(report.warnings.iter().filter(|w| w.contains("theme")).count(), 1, "{:?}", report.warnings);
    }
}

#[test]
fn xlsb_theme_tab_colours_keep_index_and_signed_tint() {
    for (tint, expected) in [(0i16, 0), (-16384, -500)] {
        let [lo, hi] = tint.to_le_bytes();
        let (wb, _) = read_xlsb(&package(false, None, [6, 5, lo, hi, 0, 0, 0, 0])).unwrap();
        assert_eq!(wb.sheets[0].tab_color, Some(Color::Theme(5, expected)));
    }
}

#[test]
fn xlsb_automatic_tab_colour_is_absent() {
    let (wb, _) = read_xlsb(&package(false, None, [0; 8])).unwrap();
    assert_eq!(wb.sheets[0].tab_color, None);
}

#[test]
fn xlsb_theme_and_tab_colour_survive_xlsx_save() {
    let (wb, _) = read_xlsb(&package(true, Some(THEME.as_bytes()), [6, 5, 0, 0, 0, 0, 0, 0])).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.theme, wb.theme);
    assert_eq!(back.theme.colors[4], 0xFF6600);
    assert_eq!(back.sheets[0].tab_color, Some(Color::Theme(5, 0)));
}
