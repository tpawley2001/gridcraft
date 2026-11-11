//! A shape's outline width (`a:ln w`, EMU, ECMA-376 Part 1 §20.1.2.2.24) and shaded outline colour
//! are read, and the width is written back on save.

use gridcraft_model::Color;

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

fn outlined(ln: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Rectangle 1"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{ln}</xdr:spPr></xdr:sp>"#
    )
}

#[test]
fn outline_width_is_read_and_written_back() {
    let wb = read_drawing(&drawing(&[outlined(
        r#"<a:ln w="28575"><a:solidFill><a:srgbClr val="4F81BD"><a:shade val="50000"/></a:srgbClr></a:solidFill></a:ln>"#,
    )]));
    let sh = &wb.sheets[0].shapes[0];
    assert_eq!(sh.format.line_width_emu, Some(28575));
    assert_eq!(sh.line, Color::Rgb(0x385D8A));
    let xml = written_drawing(&wb);
    assert!(xml.contains(r#"<a:ln w="28575">"#), "{xml}");
    assert_eq!(read_drawing(&xml).sheets[0].shapes[0].format.line_width_emu, Some(28575));
}

#[test]
fn missing_or_hostile_width_keeps_the_default() {
    for ln in [
        r#"<a:ln><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#,
        r#"<a:ln w="-5"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#,
        r#"<a:ln w="99999999999"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#,
    ] {
        let wb = read_drawing(&drawing(&[outlined(ln)]));
        assert_eq!(wb.sheets[0].shapes[0].format.line_width_emu, None, "{ln}");
    }
}
