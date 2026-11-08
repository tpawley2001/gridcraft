//! Named cell styles keep their built-in identity through open and save (ECMA-376 Part 1 §18.8.7
//! `cellStyle`: `builtinId`, `iLevel`, `hidden`, `customBuiltin`), and a built-in style made in code
//! is written with Excel's `builtinId`.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use gridcraft_model::Workbook;
use gridcraft_model::style::Style;

fn part(bytes: &[u8], name: &str) -> El {
    parse(&Package::open(bytes).unwrap().read(name).unwrap().unwrap()).unwrap()
}

/// A package whose styles.xml has one `cellStyleXfs` entry per `cellStyle` given (plus Normal).
fn with_styles(cell_styles: &[&str]) -> Vec<u8> {
    let xfs = r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0"/>"#.repeat(cell_styles.len() + 1);
    let styles = format!(
        r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Example Sans"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="{n}">{xfs}</cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="{n}"><cellStyle name="Normal" xfId="0" builtinId="0"/>{list}</cellStyles></styleSheet>"#,
        n = cell_styles.len() + 1,
        list = cell_styles.concat()
    );
    minimal(
        r#"<sheetData><row r="1"><c r="A1" s="0"><v>7</v></c></row></sheetData>"#,
        &[("xl/styles.xml", &styles)],
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        "",
    )
}

/// styles.xml after open, save, open, save.
fn saved_twice(bytes: &[u8]) -> El {
    let (wb, _) = read_xlsx(bytes).unwrap();
    let once = write_xlsx(&wb).unwrap();
    let (wb, _) = read_xlsx(&once).unwrap();
    part(&write_xlsx(&wb).unwrap(), "xl/styles.xml")
}

/// The written `<cellStyle>` named `name`.
fn style<'a>(x: &'a El, name: &str) -> &'a El {
    let all: Vec<&El> = x.child("cellStyles").expect("cellStyles").kids("cellStyle").filter(|c| c.attr("name") == Some(name)).collect();
    assert_eq!(all.len(), 1, "exactly one cellStyle named {name:?}");
    all[0]
}

fn ids(c: &El) -> (Option<&str>, Option<&str>, Option<&str>, Option<&str>) {
    (c.attr("builtinId"), c.attr("iLevel"), c.attr("hidden"), c.attr("customBuiltin"))
}

#[test]
fn builtin_ids_from_the_file_survive_two_saves() {
    let x = saved_twice(&with_styles(&[
        r#"<cellStyle name="Comma" xfId="1" builtinId="3"/>"#,
        r#"<cellStyle name="Hyperlink" xfId="2" builtinId="8"/>"#,
        r#"<cellStyle name="Good" xfId="3" builtinId="26"/>"#,
        r#"<cellStyle name="Currency [0]" xfId="4" builtinId="7"/>"#,
    ]));
    assert_eq!(ids(style(&x, "Comma")), (Some("3"), None, None, None));
    assert_eq!(ids(style(&x, "Hyperlink")), (Some("8"), None, None, None));
    assert_eq!(ids(style(&x, "Good")), (Some("26"), None, None, None));
    assert_eq!(ids(style(&x, "Currency [0]")), (Some("7"), None, None, None));
}

#[test]
fn outline_level_styles_keep_their_level() {
    let x = saved_twice(&with_styles(&[
        r#"<cellStyle name="RowLevel_1" xfId="1" builtinId="1" iLevel="0"/>"#,
        r#"<cellStyle name="ColLevel_2" xfId="2" builtinId="2" iLevel="1"/>"#,
    ]));
    assert_eq!(ids(style(&x, "RowLevel_1")), (Some("1"), Some("0"), None, None));
    assert_eq!(ids(style(&x, "ColLevel_2")), (Some("2"), Some("1"), None, None));
}

#[test]
fn hidden_and_custom_builtin_are_kept_only_where_they_were() {
    let x = saved_twice(&with_styles(&[
        r#"<cellStyle name="Followed Hyperlink" xfId="1" builtinId="9" hidden="1"/>"#,
        r#"<cellStyle name="Heading 1" xfId="2" builtinId="16" customBuiltin="1"/>"#,
        r#"<cellStyle name="Neutral" xfId="3" builtinId="28"/>"#,
    ]));
    let fh = style(&x, "Followed Hyperlink");
    assert_eq!(fh.attr("builtinId"), Some("9"));
    assert_eq!(fh.attr_bool("hidden"), Some(true));
    assert_eq!(fh.attr("customBuiltin"), None);
    let h1 = style(&x, "Heading 1");
    assert_eq!(h1.attr("builtinId"), Some("16"));
    assert_eq!(h1.attr_bool("customBuiltin"), Some(true));
    assert_eq!(h1.attr("hidden"), None);
    assert_eq!(ids(style(&x, "Neutral")), (Some("28"), None, None, None));
}

#[test]
fn a_user_style_stays_a_user_style() {
    let x = saved_twice(&with_styles(&[r#"<cellStyle name="Brand" xfId="1"/>"#, r#"<cellStyle name="Comma" xfId="2" builtinId="3"/>"#]));
    assert_eq!(style(&x, "Brand").attr("builtinId"), None);
    assert_eq!(style(&x, "Brand").attr("iLevel"), None);
    assert_eq!(style(&x, "Comma").attr("builtinId"), Some("3"));
}

#[test]
fn builtin_styles_made_in_code_get_excels_builtin_id() {
    let mut wb = Workbook::new();
    for name in ["Good", "Heading 1", "Hyperlink", "Explanatory Text", "RowLevel_3", "ColLevel_1", "Brand", "good"] {
        wb.cell_styles.push((name.to_string(), Style::default()));
    }
    let x = part(&write_xlsx(&wb).unwrap(), "xl/styles.xml");
    assert_eq!(ids(style(&x, "Good")), (Some("26"), None, None, None));
    assert_eq!(ids(style(&x, "Heading 1")), (Some("16"), None, None, None));
    assert_eq!(ids(style(&x, "Hyperlink")), (Some("8"), None, None, None));
    assert_eq!(ids(style(&x, "Explanatory Text")), (Some("53"), None, None, None));
    assert_eq!(ids(style(&x, "RowLevel_3")), (Some("1"), Some("2"), None, None));
    assert_eq!(ids(style(&x, "ColLevel_1")), (Some("2"), Some("0"), None, None));
    assert_eq!(style(&x, "Brand").attr("builtinId"), None);
    assert_eq!(style(&x, "good").attr("builtinId"), None, "name match is case-sensitive");
}

#[test]
fn normal_is_written_once_with_builtin_id_zero() {
    let x = saved_twice(&with_styles(&[r#"<cellStyle name="Comma" xfId="1" builtinId="3"/>"#]));
    let normal = style(&x, "Normal");
    assert_eq!((normal.attr("xfId"), normal.attr("builtinId")), (Some("0"), Some("0")));
}
