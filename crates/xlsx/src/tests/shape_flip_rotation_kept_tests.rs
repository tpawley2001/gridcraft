//! A shape's or connector's `a:xfrm` flips and rotation (`flipH`, `flipV`, `rot`; ECMA-376 Part 1
//! §20.1.7.6) are read and written back on save, so a rising line or a turned shape stays as drawn.

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

fn line(xfrm_attrs: &str) -> String {
    format!(
        r#"<xdr:cxnSp macro=""><xdr:nvCxnSpPr><xdr:cNvPr id="2" name="Straight Connector 1"/><xdr:cNvCxnSpPr/></xdr:nvCxnSpPr><xdr:spPr><a:xfrm{xfrm_attrs}><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></xdr:spPr></xdr:cxnSp>"#
    )
}

fn rect(xfrm_attrs: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="3" name="Rectangle 2"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm{xfrm_attrs}><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:sp>"#
    )
}

/// The attributes of the n-th `<a:xfrm ...>` opening tag in a written drawing (shapes only here).
fn xfrm_tags(xml: &str) -> Vec<String> {
    xml.match_indices("<a:xfrm").map(|(i, _)| xml[i..i + xml[i..].find('>').unwrap()].to_string()).collect()
}

#[test]
fn line_flips_are_kept_on_save() {
    for (attrs, h, v) in [
        (r#" flipV="1""#, false, true),
        (r#" flipH="1""#, true, false),
        (r#" flipH="1" flipV="1""#, true, true),
        (r#" flipH="true" flipV="false""#, true, false),
    ] {
        let wb = read_drawing(&drawing(&[line(attrs)]));
        let xml = written_drawing(&wb);
        let tags = xfrm_tags(&xml);
        assert_eq!(tags.len(), 1, "{xml}");
        assert_eq!(tags[0].contains(r#"flipH="1""#), h, "{attrs}: {}", tags[0]);
        assert_eq!(tags[0].contains(r#"flipV="1""#), v, "{attrs}: {}", tags[0]);
        // Saved twice: still the same.
        let again = written_drawing(&read_drawing(&xml));
        assert_eq!(xfrm_tags(&again), tags, "{attrs}");
    }
}

#[test]
fn rotation_is_kept_on_save() {
    let wb = read_drawing(&drawing(&[rect(r#" rot="5400000""#), line(r#" rot="16200000" flipV="1""#)]));
    let tags = xfrm_tags(&written_drawing(&wb));
    assert_eq!(tags.len(), 2, "{tags:?}");
    assert!(tags[0].contains(r#"rot="5400000""#), "{}", tags[0]);
    assert!(!tags[0].contains("flip"), "{}", tags[0]);
    assert!(tags[1].contains(r#"rot="16200000""#) && tags[1].contains(r#"flipV="1""#), "{}", tags[1]);
}

#[test]
fn plain_and_hostile_values_write_no_attributes() {
    for attrs in ["", r#" rot="0""#, r#" flipH="0" flipV="false""#, r#" flipH="yes" rot="abc""#] {
        let wb = read_drawing(&drawing(&[rect(attrs)]));
        let tags = xfrm_tags(&written_drawing(&wb));
        assert_eq!(tags, vec!["<a:xfrm".to_string()], "{attrs}");
    }
}
