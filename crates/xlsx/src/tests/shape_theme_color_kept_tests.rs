//! A shape's fill and outline given as a theme colour (`a:schemeClr`, ECMA-376 Part 1 §20.1.2.3.29),
//! with or without colour transforms (`a:lumMod`, `a:lumOff`, `a:shade`, ...), are written back as the
//! same theme colour on save, like Excel, instead of a fixed `a:srgbClr`. A recoloured shape writes its
//! new colour.

use gridcraft_model::Color;

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

fn shape(fill: &str, ln: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Rectangle 1"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}{ln}</xdr:spPr></xdr:sp>"#
    )
}

/// The written `xdr:spPr` of the first shape.
fn sp_pr(xml: &str) -> &str {
    let start = xml.find("<xdr:spPr>").expect("spPr written");
    let end = start + xml[start..].find("</xdr:spPr>").expect("spPr closed");
    &xml[start..end]
}

/// The written `xdr:spPr` without its `a:ln` (the shape fill only).
fn fill_part(xml: &str) -> String {
    let sp = sp_pr(xml);
    match (sp.find("<a:ln"), sp.find("</a:ln>")) {
        (Some(a), Some(b)) => format!("{}{}", &sp[..a], &sp[b + 7..]),
        _ => sp.to_string(),
    }
}

/// The written `a:ln` element of the first shape.
fn line_part(xml: &str) -> &str {
    let sp = sp_pr(xml);
    let a = sp.find("<a:ln").expect("a:ln written");
    let b = sp.find("</a:ln>").expect("a:ln closed");
    &sp[a..b + 7]
}

const FILL_LUM: &str = r#"<a:solidFill><a:schemeClr val="accent1"><a:lumMod val="75000"/></a:schemeClr></a:solidFill>"#;
const LINE_SHADE: &str = r#"<a:ln w="12700"><a:solidFill><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:solidFill></a:ln>"#;

#[test]
fn theme_fill_and_outline_with_transforms_are_kept() {
    let wb = read_drawing(&drawing(&[shape(FILL_LUM, LINE_SHADE)]));
    let xml = written_drawing(&wb);
    assert!(fill_part(&xml).contains(FILL_LUM), "{xml}");
    let ln = line_part(&xml);
    assert!(ln.starts_with(r#"<a:ln w="12700">"#), "{ln}");
    assert!(ln.contains(r#"<a:solidFill><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:solidFill>"#), "{ln}");
    assert!(!sp_pr(&xml).contains("srgbClr"), "{xml}");
}

#[test]
fn plain_theme_fill_is_kept() {
    let fill = r#"<a:solidFill><a:schemeClr val="accent2"/></a:solidFill>"#;
    let wb = read_drawing(&drawing(&[shape(fill, "")]));
    let xml = written_drawing(&wb);
    assert!(fill_part(&xml).contains(fill), "{xml}");
}

#[test]
fn transforms_keep_their_order() {
    let fill = r#"<a:solidFill><a:schemeClr val="tx1"><a:lumMod val="65000"/><a:lumOff val="35000"/></a:schemeClr></a:solidFill>"#;
    let wb = read_drawing(&drawing(&[shape(fill, "")]));
    let xml = written_drawing(&wb);
    assert!(fill_part(&xml).contains(fill), "{xml}");
}

#[test]
fn rgb_outline_with_a_shade_keeps_the_shade() {
    let ln = r#"<a:ln w="28575"><a:solidFill><a:srgbClr val="4F81BD"><a:shade val="50000"/></a:srgbClr></a:solidFill></a:ln>"#;
    let wb = read_drawing(&drawing(&[shape("", ln)]));
    assert_eq!(wb.sheets[0].shapes[0].line, Color::Rgb(0x385D8A));
    let xml = written_drawing(&wb);
    assert!(line_part(&xml).contains(r#"<a:srgbClr val="4F81BD"><a:shade val="50000"/></a:srgbClr>"#), "{xml}");
}

#[test]
fn drawn_colours_do_not_change_on_the_round_trip() {
    let wb = read_drawing(&drawing(&[shape(FILL_LUM, LINE_SHADE)]));
    let sh = &wb.sheets[0].shapes[0];
    assert_ne!(sh.fill, Color::Auto);
    assert_ne!(sh.line, Color::Auto);
    let again = read_drawing(&written_drawing(&wb));
    let sh2 = &again.sheets[0].shapes[0];
    assert_eq!((sh2.fill, sh2.line), (sh.fill, sh.line));
}

#[test]
fn a_recoloured_shape_writes_its_new_colour() {
    let mut wb = read_drawing(&drawing(&[shape(FILL_LUM, LINE_SHADE)]));
    std::sync::Arc::make_mut(&mut wb.sheets[0]).shapes[0].fill = Color::Rgb(0xFF0000);
    let xml = written_drawing(&wb);
    let fill = fill_part(&xml);
    assert!(fill.contains(r#"<a:srgbClr val="FF0000"/>"#), "{xml}");
    assert!(!fill.contains("schemeClr"), "{xml}");
    // The outline was not recoloured, so it keeps its theme colour.
    assert!(line_part(&xml).contains(r#"<a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr>"#), "{xml}");
}

#[test]
fn an_unknown_scheme_name_is_not_written() {
    let fill = r#"<a:solidFill><a:schemeClr val="accent9"><a:lumMod val="75000"/></a:schemeClr></a:solidFill>"#;
    let wb = read_drawing(&drawing(&[shape(fill, "")]));
    let xml = written_drawing(&wb);
    assert!(!sp_pr(&xml).contains("schemeClr"), "{xml}");
}
