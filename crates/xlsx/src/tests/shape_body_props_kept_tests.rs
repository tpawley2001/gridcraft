//! A shape's text-box settings on `a:bodyPr` (ECMA-376 Part 1 §21.1.2.1.1) survive open + save:
//! `vertOverflow`, `horzOverflow`, `upright` and the autofit child (`a:noAutofit`,
//! `a:normAutofit`, `a:spAutoFit`; §21.1.2.1.2 to §21.1.2.1.4). Packages are built in code.

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

/// A filled rectangle with text, as Excel writes it, with `body_pr` as its `a:bodyPr` element.
fn shape(id: u32, name: &str, body_pr: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="{name}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFF00"/></a:solidFill></xdr:spPr><xdr:txBody>{body_pr}<a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>Hi</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#
    )
}

/// Open + save a drawing of one shape per `a:bodyPr` in `bodies` (named `S0`, `S1`, ...); the
/// written `a:bodyPr` of each, from `<a:bodyPr` up to the `<a:lstStyle` after it.
fn saved(bodies: &[&str]) -> Vec<String> {
    let shapes: Vec<String> = bodies.iter().enumerate().map(|(i, b)| shape(i as u32 + 2, &format!("S{i}"), b)).collect();
    let xml = written_drawing(&read_drawing(&drawing(&shapes)));
    (0..bodies.len())
        .map(|i| {
            let at = xml.find(&format!("name=\"S{i}\"")).unwrap_or_else(|| panic!("shape S{i} was not written: {xml}"));
            let rest = &xml[at..];
            let start = rest.find("<a:bodyPr").unwrap_or_else(|| panic!("shape S{i} has no a:bodyPr: {rest}"));
            let end = rest[start..].find("<a:lstStyle").unwrap_or_else(|| panic!("shape S{i} has no a:lstStyle: {rest}"));
            rest[start..start + end].to_string()
        })
        .collect()
}

/// Text that is clipped at the shape's edge stays clipped.
#[test]
fn overflow_clipping_is_kept() {
    let got = saved(&[
        r#"<a:bodyPr vertOverflow="clip" horzOverflow="clip" wrap="square" rtlCol="0" anchor="t"/>"#,
        r#"<a:bodyPr vertOverflow="ellipsis" wrap="square" rtlCol="0" anchor="ctr"/>"#,
        r#"<a:bodyPr vertOverflow="overflow" horzOverflow="overflow" rtlCol="0" anchor="t"/>"#,
    ]);
    assert!(got[0].contains(r#" vertOverflow="clip""#), "{}", got[0]);
    assert!(got[0].contains(r#" horzOverflow="clip""#), "{}", got[0]);
    assert!(got[1].contains(r#" vertOverflow="ellipsis""#), "{}", got[1]);
    assert!(!got[1].contains("horzOverflow"), "an absent horzOverflow stays absent: {}", got[1]);
    assert!(got[2].contains(r#" vertOverflow="overflow""#), "{}", got[2]);
    assert!(got[2].contains(r#" horzOverflow="overflow""#), "{}", got[2]);
}

/// The attributes written before this change are still written next to the kept ones.
#[test]
fn wrap_insets_and_anchor_are_still_written() {
    let got = saved(&[
        r#"<a:bodyPr vertOverflow="clip" horzOverflow="clip" wrap="none" lIns="27432" tIns="18288" rIns="0" bIns="0" rtlCol="0" anchor="b" upright="1"/>"#,
    ]);
    for attr in [
        r#" vertOverflow="clip""#,
        r#" horzOverflow="clip""#,
        r#" upright="1""#,
        r#" wrap="none""#,
        r#" lIns="27432""#,
        r#" tIns="18288""#,
        r#" rIns="0""#,
        r#" bIns="0""#,
        r#" anchor="b""#,
    ] {
        assert!(got[0].contains(attr), "{attr} missing: {}", got[0]);
    }
}

/// Upright text (kept horizontal when the shape is rotated) stays upright.
#[test]
fn upright_is_kept() {
    let got = saved(&[r#"<a:bodyPr wrap="square" anchor="t" upright="1"/>"#, r#"<a:bodyPr wrap="square" anchor="t"/>"#]);
    assert!(got[0].contains(r#" upright="1""#), "{}", got[0]);
    assert!(!got[1].contains("upright"), "{}", got[1]);
}

/// "Resize shape to fit text", "Shrink text on overflow" and "Do not Autofit" are kept.
#[test]
fn autofit_is_kept() {
    let got = saved(&[
        r#"<a:bodyPr wrap="none" lIns="91440" tIns="45720" rIns="91440" bIns="45720"><a:spAutoFit/></a:bodyPr>"#,
        r#"<a:bodyPr wrap="square"><a:normAutofit fontScale="92500" lnSpcReduction="10000"/></a:bodyPr>"#,
        r#"<a:bodyPr wrap="square"><a:normAutofit/></a:bodyPr>"#,
        r#"<a:bodyPr wrap="square"><a:noAutofit/></a:bodyPr>"#,
    ]);
    assert!(got[0].ends_with("<a:spAutoFit/></a:bodyPr>"), "{}", got[0]);
    assert!(got[0].contains(r#" wrap="none""#) && got[0].contains(r#" lIns="91440""#), "{}", got[0]);
    assert!(got[1].contains("<a:normAutofit ") && got[1].ends_with("/></a:bodyPr>"), "{}", got[1]);
    assert!(got[1].contains(r#" fontScale="92500""#), "{}", got[1]);
    assert!(got[1].contains(r#" lnSpcReduction="10000""#), "{}", got[1]);
    assert!(got[2].ends_with("<a:normAutofit/></a:bodyPr>"), "{}", got[2]);
    assert!(got[3].ends_with("<a:noAutofit/></a:bodyPr>"), "{}", got[3]);
}

/// A shape without these settings is written without them, self-closing as before.
#[test]
fn a_plain_body_gains_nothing() {
    let got = saved(&[r#"<a:bodyPr wrap="square" rtlCol="0" anchor="t"/>"#, "<a:bodyPr/>"]);
    for body in &got {
        for word in ["vertOverflow", "horzOverflow", "upright", "utofit", "AutoFit"] {
            assert!(!body.contains(word), "{word} appeared: {body}");
        }
        assert!(body.starts_with("<a:bodyPr") && body.ends_with("/>"), "{body}");
        assert_eq!(body.matches('<').count(), 1, "one self-closing element: {body}");
    }
}

/// Values outside the schema are dropped; the shape still opens and keeps its valid settings.
#[test]
fn unknown_values_are_dropped() {
    let got = saved(&[
        r#"<a:bodyPr vertOverflow="bogus" horzOverflow="clip" upright="maybe" wrap="square"/>"#,
        r#"<a:bodyPr wrap="square"><a:normAutofit fontScale="abc" lnSpcReduction="20000"/></a:bodyPr>"#,
    ]);
    assert!(!got[0].contains("vertOverflow") && !got[0].contains("bogus"), "{}", got[0]);
    assert!(got[0].contains(r#" horzOverflow="clip""#), "{}", got[0]);
    assert!(!got[0].contains("upright"), "{}", got[0]);
    assert!(!got[1].contains("fontScale") && !got[1].contains("abc"), "{}", got[1]);
    assert!(got[1].contains(r#"<a:normAutofit lnSpcReduction="20000"/>"#), "{}", got[1]);
}
