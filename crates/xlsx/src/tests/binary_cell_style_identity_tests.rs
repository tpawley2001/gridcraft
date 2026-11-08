//! Named cell styles read from .xlsb (BrtStyle: fBuiltIn, fHidden, fCustom, iStyBuiltIn, iLevel) and .xls
//! (Style records) keep their built-in identity when saved as .xlsx (ECMA-376 Part 1 §18.8.7 `cellStyle`):
//! a built-in style keeps its `builtinId` whatever its name, and a user style is never written as a
//! built-in one because of its name.
use super::xls_style_tests::{CELL, STYLE, default_fonts, number, plain, rec, u16s, workbook};
use super::xls_tests::compound;
use super::xlsb_cell_style_tests::{package, styles_part, xf};
use super::xlsb_tests::{record, wide};
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xls, read_xlsb, write_xlsx};
use gridcraft_model::{CellStyleId, Workbook};

/// BrtStyle with every field given: `grbit` bit 0 fBuiltIn, bit 1 fHidden, bit 2 fCustom.
fn brt_style(ixf: u32, grbit: u16, builtin: u8, level: u8, name: &str) -> Vec<u8> {
    let mut d = ixf.to_le_bytes().to_vec();
    d.extend(grbit.to_le_bytes());
    d.extend([builtin, level]);
    d.extend(wide(name));
    record(48, &d)
}

const BUILTIN: u16 = 1;
const HIDDEN: u16 = 2;
const CUSTOM: u16 = 4;

/// An .xlsb whose styles part has Normal plus the BrtStyle records `styles` (style XF i for record i + 1).
fn xlsb(styles: &[Vec<u8>]) -> Workbook {
    let mut all = vec![brt_style(0, BUILTIN, 0, 0xFF, "Normal")];
    all.extend(styles.iter().cloned());
    let xfs: Vec<Vec<u8>> = (0..all.len()).map(|_| xf(0, 0, 0)).collect();
    read_xlsb(&package(&styles_part(&xfs, &all))).unwrap().0
}

fn id(wb: &Workbook, name: &str) -> Option<CellStyleId> {
    wb.cell_style_ids.iter().find(|(n, _)| n == name).map(|(_, id)| id.clone())
}

/// The `(builtinId, iLevel, hidden, customBuiltin)` attributes of the saved `<cellStyle>` named `name`.
fn saved(wb: &Workbook, name: &str) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let bytes = write_xlsx(wb).unwrap();
    let x: El = parse(&Package::open(&bytes).unwrap().read("xl/styles.xml").unwrap().unwrap()).unwrap();
    let all: Vec<&El> = x.child("cellStyles").expect("cellStyles").kids("cellStyle").filter(|c| c.attr("name") == Some(name)).collect();
    assert_eq!(all.len(), 1, "exactly one cellStyle named {name:?}");
    let a = |k: &str| all[0].attr(k).map(str::to_string);
    (a("builtinId"), a("iLevel"), a("hidden"), a("customBuiltin"))
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

#[test]
fn xlsb_builtin_style_with_a_localized_name_keeps_its_builtin_id() {
    // A German Excel names built-in style 26 (Good) "Gut".
    let wb = xlsb(&[brt_style(1, BUILTIN, 26, 0xFF, "Gut")]);
    assert_eq!(id(&wb, "Gut"), Some(CellStyleId { builtin_id: Some(26), ..CellStyleId::default() }));
    assert_eq!(saved(&wb, "Gut"), (s("26"), None, None, None));
}

#[test]
fn xlsb_builtin_style_keeps_hidden_and_custom_flags() {
    let wb = xlsb(&[brt_style(1, BUILTIN | HIDDEN, 8, 0xFF, "Hyperlink"), brt_style(2, BUILTIN | CUSTOM, 26, 0xFF, "Good")]);
    assert_eq!(id(&wb, "Hyperlink"), Some(CellStyleId { builtin_id: Some(8), hidden: true, ..CellStyleId::default() }));
    assert_eq!(id(&wb, "Good"), Some(CellStyleId { builtin_id: Some(26), custom_builtin: true, ..CellStyleId::default() }));
    assert_eq!(saved(&wb, "Hyperlink"), (s("8"), None, s("1"), None));
    assert_eq!(saved(&wb, "Good"), (s("26"), None, None, s("1")));
}

#[test]
fn xlsb_row_and_column_level_styles_keep_their_level() {
    // Localized names, so only the record says which level they are.
    let wb = xlsb(&[brt_style(1, BUILTIN, 1, 1, "Zeilenebene_2"), brt_style(2, BUILTIN, 2, 0, "Spaltenebene_1")]);
    assert_eq!(id(&wb, "Zeilenebene_2"), Some(CellStyleId { builtin_id: Some(1), i_level: Some(1), ..CellStyleId::default() }));
    assert_eq!(saved(&wb, "Zeilenebene_2"), (s("1"), s("1"), None, None));
    assert_eq!(saved(&wb, "Spaltenebene_1"), (s("2"), s("0"), None, None));
}

#[test]
fn xlsb_level_byte_is_ignored_unless_the_style_is_a_level_style() {
    // 0xFF (and anything 7 or more) is "no level"; a level on a non-level style is not written.
    let wb = xlsb(&[brt_style(1, BUILTIN, 1, 0xFF, "RowLevel_x"), brt_style(2, BUILTIN, 27, 3, "Schlecht")]);
    assert_eq!(id(&wb, "RowLevel_x").and_then(|i| i.i_level), None);
    assert_eq!(saved(&wb, "Schlecht"), (s("27"), None, None, None));
}

#[test]
fn xlsb_user_style_named_like_a_builtin_stays_a_user_style() {
    // fBuiltIn clear: iStyBuiltIn means nothing, and the name "Good" doesn't make it Excel's Good.
    let wb = xlsb(&[brt_style(1, 0, 0xFF, 0xFF, "Good"), brt_style(2, HIDDEN, 0xFF, 0xFF, "Money")]);
    assert_eq!(id(&wb, "Good"), Some(CellStyleId::default()));
    assert_eq!(saved(&wb, "Good"), (None, None, None, None));
    assert_eq!(saved(&wb, "Money"), (None, None, s("1"), None));
}

#[test]
fn xls_user_style_named_like_a_builtin_stays_a_user_style() {
    let mut f = default_fonts();
    f.push(super::xls_style_tests::xf(0, 0, STYLE, plain()));
    f.push(super::xls_style_tests::xf(0, 0, STYLE, plain()));
    f.push(super::xls_style_tests::xf(0, 0, CELL, plain()));
    // Normal (built-in 0), then a user style called "Bad" on style XF 1.
    f.push(rec(0x0293, &[&u16s(&[0x8000])[..], &[0u8, 0xFF][..]].concat()));
    f.push(rec(0x0293, &[&u16s(&[1, 3])[..], &[0u8][..], &b"Bad"[..]].concat()));
    let (wb, _) = read_xls(&compound("Workbook", &workbook(&f, &[number(0, 0, 2, 7.0)]))).unwrap();
    assert_eq!(wb.cell_styles.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["Bad"]);
    assert_eq!(id(&wb, "Bad"), Some(CellStyleId::default()));
    assert_eq!(saved(&wb, "Bad"), (None, None, None, None));
}

#[test]
fn xlsb_english_builtin_and_user_styles_still_save_as_before() {
    // Guard: what already worked keeps working.
    let wb = xlsb(&[brt_style(1, BUILTIN, 3, 0xFF, "Comma"), brt_style(2, 0, 0xFF, 0xFF, "Money")]);
    assert_eq!(saved(&wb, "Comma"), (s("3"), None, None, None));
    assert_eq!(saved(&wb, "Money"), (None, None, None, None));
}
