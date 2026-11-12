//! A shape's outline dash (`a:ln/a:prstDash`, ECMA-376 Part 1 §20.1.10.48), line cap and join
//! (§20.1.2.2.24) and its effect list (`a:effectLst` with an `a:outerShdw`, §20.1.8.26 and
//! §20.1.8.45) survive open + save. Packages are built in code.

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

const RED: &str = r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#;
const SHADOW: &str = r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:prstClr val="black"><a:alpha val="40000"/></a:prstClr></a:outerShdw></a:effectLst>"#;

/// A filled rectangle as Excel writes it: `ln` is its whole `a:ln` element and `effects` what
/// follows it inside `xdr:spPr`.
fn shape(id: u32, name: &str, ln: &str, effects: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="{name}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="4472C4"/></a:solidFill>{ln}{effects}</xdr:spPr></xdr:sp>"#
    )
}

/// The written `xdr:spPr` of each shape named `S0`, `S1`, ... in `xml`.
fn sp_prs(xml: &str, n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let at = xml.find(&format!("name=\"S{i}\"")).unwrap_or_else(|| panic!("shape S{i} was not written: {xml}"));
            let rest = &xml[at..];
            let start = rest.find("<xdr:spPr").unwrap_or_else(|| panic!("shape S{i} has no xdr:spPr: {rest}"));
            let end = rest[start..].find("</xdr:spPr>").unwrap_or_else(|| panic!("shape S{i} has an unclosed xdr:spPr: {rest}"));
            rest[start..start + end].to_string()
        })
        .collect()
}

/// Open + save a drawing of one shape per `(a:ln, effects)` pair; each written `xdr:spPr`.
fn saved(shapes: &[(&str, &str)]) -> Vec<String> {
    let xml: Vec<String> = shapes.iter().enumerate().map(|(i, (ln, fx))| shape(i as u32 + 2, &format!("S{i}"), ln, fx)).collect();
    sp_prs(&written_drawing(&read_drawing(&drawing(&xml))), shapes.len())
}

/// The `a:ln` element of a written `xdr:spPr`, from `<a:ln` to `</a:ln>`.
fn ln_of(sp_pr: &str) -> &str {
    let start = sp_pr.find("<a:ln").unwrap_or_else(|| panic!("no a:ln: {sp_pr}"));
    let end = sp_pr[start..].find("</a:ln>").unwrap_or_else(|| panic!("a:ln is not closed: {sp_pr}"));
    &sp_pr[start..start + end]
}

/// `needle` is in `hay`, after every occurrence of `before`.
fn after(hay: &str, needle: &str, before: &str) -> bool {
    match (hay.find(needle), hay.rfind(before)) {
        (Some(n), Some(b)) => n > b,
        _ => false,
    }
}

/// Each of the ten non-solid preset dashes is written back, after the line colour.
#[test]
fn every_preset_dash_is_kept() {
    let dashes = ["dot", "dash", "lgDash", "dashDot", "lgDashDot", "lgDashDotDot", "sysDash", "sysDot", "sysDashDot", "sysDashDotDot"];
    let lns: Vec<String> = dashes.iter().map(|d| format!(r#"<a:ln w="19050">{RED}<a:prstDash val="{d}"/></a:ln>"#)).collect();
    let shapes: Vec<(&str, &str)> = lns.iter().map(|l| (l.as_str(), "")).collect();
    let got = saved(&shapes);
    for (d, sp_pr) in dashes.iter().zip(&got) {
        let ln = ln_of(sp_pr);
        let want = format!(r#"<a:prstDash val="{d}"/>"#);
        assert_eq!(ln.matches("<a:prstDash").count(), 1, "{d}: {ln}");
        assert!(after(ln, &want, "</a:solidFill>"), "{d}: the dash follows the line fill: {ln}");
        assert!(ln.contains(r#" w="19050""#), "{d}: the width is still written: {ln}");
    }
}

/// A solid or unstated dash, and a value the standard does not list, never become a dash.
#[test]
fn solid_absent_and_unknown_dashes_write_no_dash_pattern() {
    let solid = format!(r#"<a:ln w="12700">{RED}<a:prstDash val="solid"/></a:ln>"#);
    let absent = format!(r#"<a:ln w="12700">{RED}</a:ln>"#);
    let unknown = format!(r#"<a:ln w="12700">{RED}<a:prstDash val="wavy"/></a:ln>"#);
    let got = saved(&[(&solid, ""), (&absent, ""), (&unknown, "")]);
    for (i, sp_pr) in got.iter().enumerate() {
        let ln = ln_of(sp_pr).replace(r#"<a:prstDash val="solid"/>"#, "");
        assert!(!ln.contains("prstDash"), "S{i}: {ln}");
        assert!(ln.contains("<a:solidFill>"), "S{i}: the line colour is still written: {ln}");
    }
    assert!(!got[1].contains("prstDash"), "an unstated dash stays unstated: {}", got[1]);
}

/// Line cap and join are written back: the cap on `a:ln`, the join after the dash.
#[test]
fn cap_and_join_are_kept() {
    let round = format!(r#"<a:ln w="38100" cap="rnd">{RED}<a:prstDash val="sysDot"/><a:round/></a:ln>"#);
    let bevel = format!(r#"<a:ln w="38100" cap="sq">{RED}<a:bevel/></a:ln>"#);
    let miter = format!(r#"<a:ln w="38100" cap="flat">{RED}<a:prstDash val="dash"/><a:miter lim="800000"/></a:ln>"#);
    let plain = format!(r#"<a:ln w="38100">{RED}</a:ln>"#);
    let bad_cap = format!(r#"<a:ln w="38100" cap="pointy">{RED}</a:ln>"#);
    let got = saved(&[(&round, ""), (&bevel, ""), (&miter, ""), (&plain, ""), (&bad_cap, "")]);
    let ln: Vec<&str> = got.iter().map(|s| ln_of(s)).collect();

    assert!(ln[0].contains(r#" cap="rnd""#), "{}", ln[0]);
    assert!(after(ln[0], "<a:round/>", r#"<a:prstDash val="sysDot"/>"#), "the join follows the dash: {}", ln[0]);
    assert!(ln[1].contains(r#" cap="sq""#), "{}", ln[1]);
    assert!(after(ln[1], "<a:bevel/>", "</a:solidFill>"), "the join follows the line fill: {}", ln[1]);
    assert!(ln[2].contains(r#" cap="flat""#), "{}", ln[2]);
    assert!(after(ln[2], r#"<a:miter lim="800000"/>"#, r#"<a:prstDash val="dash"/>"#), "{}", ln[2]);
    for l in &ln[..3] {
        assert!(l.contains(r#" w="38100""#), "the width is still written: {l}");
    }
    for l in &ln[3..] {
        assert!(!l.contains(" cap="), "an unstated or unknown cap is not written: {l}");
        assert!(!l.contains("<a:round") && !l.contains("<a:bevel") && !l.contains("<a:miter"), "no join was stated: {l}");
    }
}

/// A shape without an outline stays without one; its dash is not invented.
#[test]
fn a_shape_without_a_line_gets_no_dash() {
    let got = saved(&[(r#"<a:ln><a:noFill/></a:ln>"#, "")]);
    assert!(got[0].contains("<a:ln><a:noFill/></a:ln>"), "{}", got[0]);
    assert!(!got[0].contains("prstDash"), "{}", got[0]);
}

/// An outer shadow keeps every attribute and its colour (with the alpha), after `a:ln`.
#[test]
fn outer_shadow_is_kept() {
    let ln = format!(r#"<a:ln w="12700">{RED}</a:ln>"#);
    let scaled = r#"<a:effectLst><a:outerShdw blurRad="76200" dist="50800" dir="5400000" sx="102000" sy="102000" kx="-800400" ky="0" algn="bl" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="43137"/></a:srgbClr></a:outerShdw></a:effectLst>"#;
    let scheme = r#"<a:effectLst><a:outerShdw dist="25400" dir="2700000"><a:schemeClr val="accent2"><a:lumMod val="50000"/></a:schemeClr></a:outerShdw></a:effectLst>"#;
    let got = saved(&[(&ln, SHADOW), (&ln, scaled), (&ln, scheme)]);

    for attr in [r#" blurRad="50800""#, r#" dist="38100""#, r#" dir="2700000""#, r#" algn="tl""#, r#" rotWithShape="0""#] {
        assert!(got[0].contains(attr), "{attr} missing: {}", got[0]);
    }
    assert!(got[0].contains(r#"<a:prstClr val="black"><a:alpha val="40000"/></a:prstClr>"#), "{}", got[0]);
    for attr in [r#" sx="102000""#, r#" sy="102000""#, r#" kx="-800400""#, r#" ky="0""#, r#" algn="bl""#, r#" blurRad="76200""#] {
        assert!(got[1].contains(attr), "{attr} missing: {}", got[1]);
    }
    assert!(got[1].contains(r#"<a:srgbClr val="000000"><a:alpha val="43137"/></a:srgbClr>"#), "{}", got[1]);
    assert!(got[2].contains(r#"<a:schemeClr val="accent2"><a:lumMod val="50000"/></a:schemeClr>"#), "{}", got[2]);
    let tag = got[2].find("<a:outerShdw").map(|at| &got[2][at..]).and_then(|rest| rest.find('>').map(|end| &rest[..end])).unwrap_or_default();
    assert!(!tag.contains("blurRad") && !tag.contains("algn") && !tag.contains("rotWithShape"), "unstated attributes stay unstated: {tag}");
    for sp_pr in &got {
        assert_eq!(sp_pr.matches("<a:outerShdw").count(), 1, "{sp_pr}");
        assert!(after(sp_pr, "<a:effectLst>", "</a:ln>"), "the effect list follows a:ln: {sp_pr}");
    }
}

/// An empty effect list ("no effects") is kept; a shape that stated none gets none.
#[test]
fn an_empty_effect_list_is_kept_and_none_is_not_invented() {
    let ln = format!(r#"<a:ln w="12700">{RED}</a:ln>"#);
    let got = saved(&[(&ln, "<a:effectLst/>"), (&ln, ""), (r#"<a:ln><a:noFill/></a:ln>"#, "<a:effectLst/>")]);
    assert!(after(&got[0], "<a:effectLst/>", "</a:ln>"), "{}", got[0]);
    assert!(!got[1].contains("effectLst"), "{}", got[1]);
    assert!(after(&got[2], "<a:effectLst/>", "<a:ln><a:noFill/></a:ln>"), "{}", got[2]);
}

/// Effects this change does not model are not written back half-read.
#[test]
fn other_effects_are_not_written_as_a_shadow() {
    let ln = format!(r#"<a:ln w="12700">{RED}</a:ln>"#);
    let glow = r#"<a:effectLst><a:glow rad="63500"><a:srgbClr val="FF0000"/></a:glow></a:effectLst>"#;
    let got = saved(&[(&ln, glow)]);
    assert!(!got[0].contains("outerShdw"), "{}", got[0]);
    assert!(!got[0].contains("<a:effectLst/>"), "a glow is not the same as 'no effects': {}", got[0]);
}

/// Dash, cap, join and shadow survive a second open + save unchanged.
#[test]
fn dash_and_shadow_survive_a_second_save() {
    let ln = format!(r#"<a:ln w="19050" cap="rnd">{RED}<a:prstDash val="lgDashDot"/><a:round/></a:ln>"#);
    let wb = read_drawing(&drawing(&[shape(2, "S0", &ln, SHADOW)]));
    let first = written_drawing(&wb);
    let second = written_drawing(&read_drawing(&first));
    let (a, b) = (sp_prs(&first, 1).remove(0), sp_prs(&second, 1).remove(0));
    assert_eq!(a, b);
    assert!(b.contains(r#"<a:prstDash val="lgDashDot"/>"#) && b.contains("<a:round/>") && b.contains(r#" cap="rnd""#), "{b}");
    assert!(b.contains("<a:outerShdw "), "{b}");
    assert!(b.contains(r#"<a:prstClr val="black"><a:alpha val="40000"/></a:prstClr>"#), "{b}");
}
