//! A shape's formatted text survives open + save: several runs with their own properties
//! (`a:r/a:rPr`, ECMA-376 Part 1 §21.1.2.3.9), paragraph properties (`a:pPr`, §21.1.2.2.7), line
//! breaks (`a:br`), the end-of-paragraph run properties (`a:endParaRPr`) and the list styles
//! (`a:lstStyle`, §21.1.2.4.12). The kept body is written back only while the shape's text and
//! text format are the ones read with it. Packages are built in code.

use super::shape_preset_geometry::{drawing, read_drawing, written_drawing};

/// A filled rectangle whose `xdr:txBody` holds `body` after its `a:bodyPr`.
fn shape(id: u32, name: &str, body: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="{name}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFF00"/></a:solidFill></xdr:spPr><xdr:txBody><a:bodyPr wrap="square" rtlCol="0" anchor="t"/>{body}</xdr:txBody></xdr:sp>"#
    )
}

/// A drawing of one shape per body in `bodies`, named `S0`, `S1`, ...
fn drawing_of(bodies: &[&str]) -> String {
    let shapes: Vec<String> = bodies.iter().enumerate().map(|(i, b)| shape(i as u32 + 2, &format!("S{i}"), b)).collect();
    drawing(&shapes)
}

/// The written `xdr:txBody` (tags included) of the shape named `S{i}` in a saved drawing.
fn text_body(xml: &str, i: usize) -> String {
    let at = xml.find(&format!("name=\"S{i}\"")).unwrap_or_else(|| panic!("shape S{i} was not written: {xml}"));
    let rest = &xml[at..];
    let rest = &rest[..rest.find("</xdr:sp>").unwrap_or_else(|| panic!("shape S{i} is not closed: {rest}"))];
    let start = rest.find("<xdr:txBody>").unwrap_or_else(|| panic!("shape S{i} has no text body: {rest}"));
    let end = rest.find("</xdr:txBody>").unwrap_or_else(|| panic!("shape S{i}'s text body is not closed: {rest}"));
    rest[start..end + "</xdr:txBody>".len()].to_string()
}

/// The `n`th (0-based) `<a:r>...</a:r>` of a text body.
fn run(body: &str, n: usize) -> String {
    let (at, _) = body.match_indices("<a:r>").nth(n).unwrap_or_else(|| panic!("the text body has no run {n}: {body}"));
    let rest = &body[at..];
    rest[..rest.find("</a:r>").unwrap_or_else(|| panic!("run {n} is not closed: {rest}"))].to_string()
}

const TWO_RUNS: &str = r#"<a:lstStyle/><a:p><a:r><a:rPr lang="fr-FR" sz="1100" b="1"/><a:t>Total: </a:t></a:r><a:r><a:rPr lang="fr-FR" sz="1400" i="1" u="sng" strike="sngStrike"><a:solidFill><a:schemeClr val="accent2"><a:lumMod val="40000"/><a:lumOff val="60000"/></a:schemeClr></a:solidFill><a:latin typeface="Arial"/><a:cs typeface="Arial"/></a:rPr><a:t>42</a:t></a:r></a:p>"#;

fn assert_two_runs(body: &str) {
    assert_eq!(body.matches("<a:r>").count(), 2, "both runs are written: {body}");
    assert_eq!(body.matches(r#"lang="fr-FR""#).count(), 2, "each run keeps its language: {body}");
    let first = run(body, 0);
    for want in [r#" sz="1100""#, r#" b="1""#, "<a:t>Total: </a:t>"] {
        assert!(first.contains(want), "the first run keeps {want}: {first}");
    }
    assert!(!first.contains(r#" i="1""#), "the first run is not italic: {first}");
    let second = run(body, 1);
    for want in [
        r#" sz="1400""#,
        r#" i="1""#,
        r#" u="sng""#,
        r#" strike="sngStrike""#,
        r#"<a:schemeClr val="accent2""#,
        r#"<a:lumMod val="40000""#,
        r#"<a:lumOff val="60000""#,
        r#"<a:latin typeface="Arial""#,
        r#"<a:cs typeface="Arial""#,
        "<a:t>42</a:t>",
    ] {
        assert!(second.contains(want), "the second run keeps {want}: {second}");
    }
    assert!(!second.contains(r#" b="1""#), "the second run is not bold: {second}");
}

/// Two runs with different formats stay two runs, each with its own properties.
#[test]
fn runs_keep_their_own_properties() {
    let wb = read_drawing(&drawing_of(&[TWO_RUNS]));
    assert_eq!(wb.sheet(0).unwrap().shapes[0].text, "Total: 42");
    let xml = written_drawing(&wb);
    assert_two_runs(&text_body(&xml, 0));
}

/// The saved file, opened and saved again, still has both runs.
#[test]
fn runs_survive_a_second_open_and_save() {
    let once = written_drawing(&read_drawing(&drawing_of(&[TWO_RUNS])));
    let again = read_drawing(&once);
    assert_eq!(again.sheet(0).unwrap().shapes[0].text, "Total: 42");
    assert_two_runs(&text_body(&written_drawing(&again), 0));
}

/// Paragraph indents, line spacing, bullets, line breaks and the end-of-paragraph properties are kept.
#[test]
fn paragraph_properties_breaks_and_end_properties_are_kept() {
    let body = r#"<a:lstStyle/><a:p><a:pPr marL="171450" indent="-171450" lvl="1"><a:lnSpc><a:spcPct val="150000"/></a:lnSpc><a:spcBef><a:spcPts val="600"/></a:spcBef><a:buFont typeface="Arial"/><a:buChar char="•"/></a:pPr><a:r><a:rPr lang="en-US" sz="1100"/><a:t>one</a:t></a:r><a:br><a:rPr lang="en-US" sz="1100"/></a:br><a:r><a:rPr lang="en-US" sz="1100"/><a:t>two</a:t></a:r><a:endParaRPr lang="en-ZA" sz="1100" b="1"/></a:p><a:p><a:pPr algn="r"/><a:r><a:rPr lang="en-US" sz="1100" baseline="30000" spc="300"/><a:t>three</a:t></a:r></a:p>"#;
    let got = text_body(&written_drawing(&read_drawing(&drawing_of(&[body]))), 0);
    for want in [
        r#" marL="171450""#,
        r#" indent="-171450""#,
        r#" lvl="1""#,
        r#"<a:spcPct val="150000""#,
        r#"<a:spcPts val="600""#,
        r#"<a:buFont typeface="Arial""#,
        r#"<a:buChar char="•""#,
        "<a:br>",
        "<a:endParaRPr",
        r#"lang="en-ZA""#,
        r#" algn="r""#,
        r#" baseline="30000""#,
        r#" spc="300""#,
        "<a:t>one</a:t>",
        "<a:t>two</a:t>",
        "<a:t>three</a:t>",
    ] {
        assert!(got.contains(want), "the text body keeps {want}: {got}");
    }
    assert_eq!(got.matches("<a:p>").count(), 2, "two paragraphs: {got}");
    assert_eq!(got.matches("<a:r>").count(), 3, "three runs: {got}");
}

/// The list styles of a text body are kept.
#[test]
fn list_styles_are_kept() {
    let body = r#"<a:lstStyle><a:lvl1pPr marL="0" indent="0"><a:defRPr sz="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></a:defRPr></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr lang="en-US"/><a:t>Styled</a:t></a:r></a:p>"#;
    let got = text_body(&written_drawing(&read_drawing(&drawing_of(&[body]))), 0);
    for want in ["<a:lvl1pPr", r#" marL="0""#, r#"<a:defRPr sz="1200""#, r#"<a:schemeClr val="tx1""#, "<a:t>Styled</a:t>"] {
        assert!(got.contains(want), "the text body keeps {want}: {got}");
    }
    assert!(!got.contains("<a:lstStyle/>"), "the list styles are not written empty: {got}");
}

/// Text and attribute values are escaped when the kept body is written back.
#[test]
fn kept_text_is_escaped() {
    let body = r#"<a:lstStyle/><a:p><a:r><a:rPr lang="en-US" b="1"><a:latin typeface="A&amp;B"/></a:rPr><a:t>R&amp;D </a:t></a:r><a:r><a:rPr lang="en-US"/><a:t>&lt;5 "q"</a:t></a:r></a:p>"#;
    let wb = read_drawing(&drawing_of(&[body]));
    let text = r#"R&D <5 "q""#;
    assert_eq!(wb.sheet(0).unwrap().shapes[0].text, text);
    let xml = written_drawing(&wb);
    assert!(crate::xml::parse(xml.as_bytes()).is_ok(), "the saved drawing parses: {xml}");
    let got = text_body(&xml, 0);
    assert_eq!(got.matches("<a:r>").count(), 2, "both runs are written: {got}");
    assert!(got.contains("R&amp;D "), "{got}");
    assert!(got.contains("&lt;5 "), "{got}");
    assert!(got.contains(r#"typeface="A&amp;B""#), "{got}");
    assert_eq!(read_drawing(&xml).sheet(0).unwrap().shapes[0].text, text);
}

/// Changed text is written the way it was before this change: the old runs are gone.
#[test]
fn edited_text_drops_the_kept_runs() {
    let mut wb = read_drawing(&drawing_of(&[TWO_RUNS, TWO_RUNS]));
    wb.sheet_mut(0).unwrap().shapes[0].text = "New\ntext".into();
    let xml = written_drawing(&wb);
    let got = text_body(&xml, 0);
    assert!(got.contains("<a:t>New</a:t>") && got.contains("<a:t>text</a:t>"), "{got}");
    assert_eq!(got.matches("<a:r>").count(), 2, "one run per line: {got}");
    for gone in ["Total", "<a:t>42</a:t>", r#" u="sng""#, r#" sz="1400""#, "schemeClr"] {
        assert!(!got.contains(gone), "{gone} belongs to the replaced text: {got}");
    }
    // The shape that was not edited keeps its runs.
    assert_two_runs(&text_body(&xml, 1));
}

/// A changed text format (here the size) is written for the whole text: the old runs are gone.
#[test]
fn edited_format_drops_the_kept_runs() {
    let mut wb = read_drawing(&drawing_of(&[TWO_RUNS]));
    wb.sheet_mut(0).unwrap().shapes[0].format.font_size = Some(20.0);
    let got = text_body(&written_drawing(&wb), 0);
    assert_eq!(got.matches("<a:r>").count(), 1, "one run for the one line: {got}");
    assert!(got.contains(r#" sz="2000""#), "the new size is written: {got}");
    assert!(got.contains("<a:t>Total: 42</a:t>"), "{got}");
    for gone in [r#" sz="1400""#, r#" sz="1100""#, r#" u="sng""#] {
        assert!(!got.contains(gone), "{gone} belongs to the replaced format: {got}");
    }
}

/// A body that points at a relationship is not kept (the id would dangle and Excel would ask to
/// repair the file); its text still is.
#[test]
fn a_body_with_a_relationship_id_is_not_kept() {
    let body = r#"<a:lstStyle/><a:p><a:r><a:rPr lang="en-US" u="sng"><a:hlinkClick xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId9"/></a:rPr><a:t>link</a:t></a:r><a:r><a:rPr lang="en-US" b="1"/><a:t> text</a:t></a:r></a:p>"#;
    let wb = read_drawing(&drawing_of(&[body]));
    assert_eq!(wb.sheet(0).unwrap().shapes[0].text, "link text");
    let xml = written_drawing(&wb);
    assert!(crate::xml::parse(xml.as_bytes()).is_ok(), "the saved drawing parses: {xml}");
    let got = text_body(&xml, 0);
    assert!(!got.contains("hlinkClick"), "{got}");
    assert!(!got.contains("r:id"), "{got}");
    assert!(got.contains("<a:t>link text</a:t>"), "the text is written the way it was before this change: {got}");
}

/// Extension content in another namespace never makes the saved drawing unreadable.
#[test]
fn a_body_with_foreign_extension_content_still_saves_a_readable_drawing() {
    let body = r#"<a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"><a:extLst><a:ext uri="{FF2B5EF4-FFF2-40B4-BE49-F238E27FC236}"><a16:creationId xmlns:a16="http://schemas.microsoft.com/office/drawing/2014/main" id="{00000000-0008-0000-0000-000002000000}"/></a:ext></a:extLst></a:rPr><a:t>ext</a:t></a:r></a:p>"#;
    let xml = written_drawing(&read_drawing(&drawing_of(&[body])));
    assert!(crate::xml::parse(xml.as_bytes()).is_ok(), "the saved drawing parses: {xml}");
    assert!(text_body(&xml, 0).contains("<a:t>ext</a:t>"), "{xml}");
    assert_eq!(read_drawing(&xml).sheet(0).unwrap().shapes[0].text, "ext");
}
