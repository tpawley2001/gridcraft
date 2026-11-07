//! A block arrow (`prst="rightArrow"`, ECMA-376 Part 1 §20.1.10.56) keeps its preset through
//! open and Save; it is not GridCraft's line-with-arrowhead.

use super::shape_preset_geometry::{drawing, guides, presets, read_drawing, sp, written_drawing};

const RIGHT_ARROW: &str = r#"<a:prstGeom prst="rightArrow"><a:avLst/></a:prstGeom>"#;
const RIGHT_ARROW_ADJ: &str =
    r#"<a:prstGeom prst="rightArrow"><a:avLst><a:gd name="adj1" fmla="val 30000"/><a:gd name="adj2" fmla="val 70000"/></a:avLst></a:prstGeom>"#;

/// A straight connector with an arrowhead at its end (GridCraft's own Arrow).
fn line_arrow(id: u32) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="Arrow {id}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="0"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:tailEnd type="triangle"/></a:ln></xdr:spPr></xdr:sp>"#
    )
}

#[test]
fn right_block_arrow_saves_as_right_arrow() {
    let wb = read_drawing(&drawing(&[sp(2, "Right Arrow 1", RIGHT_ARROW, "Next")]));
    assert_eq!(wb.sheet(0).unwrap().shapes.len(), 1);
    let xml = written_drawing(&wb);
    assert_eq!(presets(&xml), ["rightArrow"], "{xml}");
    assert!(!xml.contains("tailEnd"), "a block arrow has no line end: {xml}");
    assert!(xml.contains("<a:t>Next</a:t>"), "{xml}");
    assert!(xml.contains("4472C4"), "fill kept: {xml}");
}

#[test]
fn right_block_arrow_keeps_its_guides() {
    let wb = read_drawing(&drawing(&[sp(2, "Right Arrow 1", RIGHT_ARROW_ADJ, "")]));
    let xml = written_drawing(&wb);
    assert_eq!(presets(&xml), ["rightArrow"]);
    assert_eq!(guides(&xml, 0), r#"<a:gd name="adj1" fmla="val 30000"/><a:gd name="adj2" fmla="val 70000"/>"#);
}

#[test]
fn right_block_arrow_survives_a_second_save() {
    let wb = read_drawing(&drawing(&[sp(2, "Right Arrow 1", RIGHT_ARROW_ADJ, "")]));
    let again = read_drawing(&written_drawing(&wb));
    let xml = written_drawing(&again);
    assert_eq!(presets(&xml), ["rightArrow"]);
    assert_eq!(guides(&xml, 0), r#"<a:gd name="adj1" fmla="val 30000"/><a:gd name="adj2" fmla="val 70000"/>"#);
}

#[test]
fn line_arrow_still_saves_as_a_line_with_a_head() {
    let wb = read_drawing(&drawing(&[line_arrow(2)]));
    let xml = written_drawing(&wb);
    assert_eq!(presets(&xml), ["line"]);
    assert!(xml.contains(r#"<a:tailEnd type="triangle"/>"#), "{xml}");
}

#[test]
fn other_block_arrows_still_kept() {
    let geom = |p: &str| format!(r#"<a:prstGeom prst="{p}"><a:avLst/></a:prstGeom>"#);
    let wb = read_drawing(&drawing(&[sp(2, "L", &geom("leftArrow"), ""), sp(3, "U", &geom("upArrow"), ""), sp(4, "D", &geom("downArrow"), "")]));
    assert_eq!(presets(&written_drawing(&wb)), ["leftArrow", "upArrow", "downArrow"]);
}
