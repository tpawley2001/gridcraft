//! A shape's style element (`xdr:style` with `a:lnRef`, `a:fillRef`, `a:effectRef`, `a:fontRef`;
//! ECMA-376 Part 1 §20.5.2.30, §20.1.4.2.19, §20.1.4.2.10, §20.1.4.2.8, §20.1.4.1.17) survives
//! open + save. Excel's default shapes carry no fill or outline in `xdr:spPr`: without the style
//! they are saved invisible. Packages are built in code.

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

const LN_REF: &str = r#"<a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef>"#;
const FILL_REF: &str = r#"<a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef>"#;
const EFFECT_REF: &str = r#"<a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef>"#;
const FONT_REF: &str = r#"<a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef>"#;
const RED: &str = r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#;

/// A shape as Excel writes it: `fill` inside `xdr:spPr` (may be empty), then `style` (may be empty).
fn shape(id: u32, name: &str, prst: &str, fill: &str, style: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="{name}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom>{fill}</xdr:spPr>{style}<xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>Hi</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#
    )
}

/// Open + save a drawing with a theme-styled star, a styled rectangle with its own red fill, and
/// a shape without a style; the written drawing XML.
fn saved() -> String {
    let style = format!("<xdr:style>{LN_REF}{FILL_REF}{EFFECT_REF}{FONT_REF}</xdr:style>");
    let shapes = [
        shape(2, "Star 1", "star5", "", &style),
        shape(3, "Rectangle 2", "rect", RED, &style),
        shape(4, "Diamond 3", "diamond", r#"<a:solidFill><a:srgbClr val="00B050"/></a:solidFill>"#, ""),
    ];
    written_drawing(&read_drawing(&drawing(&shapes)))
}

/// The written `<xdr:sp>...</xdr:sp>` of the shape named `name`.
fn block<'a>(xml: &'a str, name: &str) -> &'a str {
    let at = xml.find(&format!("name=\"{name}\"")).unwrap_or_else(|| panic!("shape {name} was not written: {xml}"));
    let start = xml[..at].rfind("<xdr:sp ").or_else(|| xml[..at].rfind("<xdr:sp>")).expect("xdr:sp start");
    let end = at + xml[at..].find("</xdr:sp>").expect("xdr:sp end");
    &xml[start..end]
}

/// The written `<xdr:style>...</xdr:style>` of a shape block.
fn style_of(block: &str) -> &str {
    let a = block.find("<xdr:style>").unwrap_or_else(|| panic!("xdr:style written: {block}"));
    let b = block.find("</xdr:style>").expect("xdr:style closed");
    &block[a..b]
}

#[test]
fn shape_style_refs_kept_on_a_theme_styled_shape() {
    let xml = saved();
    let star = block(&xml, "Star 1");
    let style = style_of(star);
    for part in [LN_REF, FILL_REF, EFFECT_REF, FONT_REF] {
        assert!(style.contains(part), "{part} kept in {style}");
    }
    let at = |s: &str| style.find(s).unwrap();
    assert!(
        at("<a:lnRef") < at("<a:fillRef") && at("<a:fillRef") < at("<a:effectRef") && at("<a:effectRef") < at("<a:fontRef"),
        "schema order: {style}"
    );
}

#[test]
fn shape_style_refs_kept_between_sp_pr_and_tx_body() {
    let xml = saved();
    let star = block(&xml, "Star 1");
    let (sp_pr, style, body) =
        (star.find("</xdr:spPr>").expect("spPr"), star.find("<xdr:style>").expect("style"), star.find("<xdr:txBody>").expect("txBody"));
    assert!(sp_pr < style && style < body, "nvSpPr, spPr, style, txBody: {star}");
}

#[test]
fn shape_style_refs_kept_without_baking_colours_into_sp_pr() {
    let xml = saved();
    let star = block(&xml, "Star 1");
    let sp_pr = &star[star.find("<xdr:spPr>").expect("spPr")..star.find("</xdr:spPr>").expect("spPr closed")];
    assert!(!sp_pr.contains("<a:noFill/>"), "a theme-styled shape is not saved as no-fill: {sp_pr}");
    assert!(!sp_pr.contains("srgbClr") && !sp_pr.contains("<a:solidFill>"), "the fill stays with the theme style: {sp_pr}");
}

#[test]
fn shape_style_refs_kept_next_to_an_explicit_fill() {
    let xml = saved();
    let rect = block(&xml, "Rectangle 2");
    assert!(rect.contains(RED), "the shape's own fill is still written: {rect}");
    let style = style_of(rect);
    assert!(style.contains(FILL_REF) && style.contains(LN_REF), "{style}");
}

#[test]
fn shape_style_refs_kept_absent_stays_absent() {
    let xml = saved();
    let diamond = block(&xml, "Diamond 3");
    assert!(!diamond.contains("<xdr:style>"), "no style invented: {diamond}");
    assert!(diamond.contains(r#"<a:srgbClr val="00B050"/>"#), "{diamond}");
}
