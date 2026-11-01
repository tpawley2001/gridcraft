//! Strict Open XML (ISO/IEC 29500 Strict) → Transitional for kept content: GridCraft saves
//! Transitional, so the relationship types and namespace URIs of what a Strict package keeps are
//! translated on read. Binary parts are kept byte for byte.

use super::*;

/// Strict relationship types share a prefix; the last segment is the same in Transitional except
/// for these kinds (ISO/IEC 29500-1 §15 vs. ECMA-376 Part 1 §15).
const STRICT_REL_PREFIX: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const TRANSITIONAL_REL_PREFIX: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const RENAMED_REL_KINDS: &[(&str, &str)] =
    &[("extendedProperties", "extended-properties"), ("customProperties", "custom-properties"), ("customXmlProperties", "customXmlProps")];

/// Strict namespace URIs and their Transitional spelling (ISO/IEC 29500-1 Annex A).
const NAMESPACES: &[(&str, &str)] = &[
    ("http://purl.oclc.org/ooxml/spreadsheetml/main", "http://schemas.openxmlformats.org/spreadsheetml/2006/main"),
    ("http://purl.oclc.org/ooxml/officeDocument/relationships", "http://schemas.openxmlformats.org/officeDocument/2006/relationships"),
    ("http://purl.oclc.org/ooxml/drawingml/main", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    ("http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing", "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"),
    ("http://purl.oclc.org/ooxml/drawingml/chart", "http://schemas.openxmlformats.org/drawingml/2006/chart"),
    ("http://purl.oclc.org/ooxml/drawingml/picture", "http://schemas.openxmlformats.org/drawingml/2006/picture"),
    ("http://purl.oclc.org/ooxml/officeDocument/customXml", "http://schemas.openxmlformats.org/officeDocument/2006/customXml"),
    ("http://purl.oclc.org/ooxml/officeDocument/extendedProperties", "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"),
    ("http://purl.oclc.org/ooxml/officeDocument/customProperties", "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"),
    ("http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes", "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"),
];

const STRICT_HOST: &str = "http://purl.oclc.org/";

/// The Transitional relationship type for a Strict one; any other type is returned unchanged.
pub fn strict_rel_type_to_transitional(rel_type: &str) -> Cow<'_, str> {
    let Some(kind) = rel_type.strip_prefix(STRICT_REL_PREFIX) else { return Cow::Borrowed(rel_type) };
    let kind = RENAMED_REL_KINDS.iter().find(|(s, _)| *s == kind).map_or(kind, |(_, t)| t);
    Cow::Owned(format!("{TRANSITIONAL_REL_PREFIX}{kind}"))
}

/// Why kept Strict XML can't be saved as Transitional.
#[derive(Debug, PartialEq, Eq)]
pub enum Untranslatable {
    /// A `purl.oclc.org` namespace with no Transitional spelling known here.
    UnknownNamespace(String),
    /// A value only Strict allows (a universal measure such as `2.5in` for a coordinate).
    StrictValue,
    /// The XML could not be read.
    Damaged,
}

/// `xml` with the Strict namespace URIs of its `xmlns`/`xmlns:*` declarations replaced by their
/// Transitional spelling. Element text and other attributes are untouched.
pub fn translate_namespaces(xml: &str) -> Result<Cow<'_, str>, Untranslatable> {
    if !xml.contains(STRICT_HOST) {
        return Ok(Cow::Borrowed(xml));
    }
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        out.push_str(rest.get(..lt).unwrap_or(""));
        rest = rest.get(lt..).unwrap_or("");
        // Markup that holds no attributes is copied through its terminator.
        let skip = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>"), ("<!", ">")].into_iter().find(|(open, _)| rest.starts_with(open));
        if let Some((_, close)) = skip {
            let end = rest.find(close).map(|i| i + close.len()).ok_or(Untranslatable::Damaged)?;
            out.push_str(rest.get(..end).unwrap_or(""));
            rest = rest.get(end..).unwrap_or("");
            continue;
        }
        let end = tag_end(rest).ok_or(Untranslatable::Damaged)?;
        translate_tag(rest.get(..end).unwrap_or(""), &mut out)?;
        rest = rest.get(end..).unwrap_or("");
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

/// The length of the tag `s` starts with (through its `>`, outside quoted values).
fn tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Copies one tag to `out`, translating its namespace declarations.
fn translate_tag(tag: &str, out: &mut String) -> Result<(), Untranslatable> {
    let mut i = 0;
    while let Some(eq) = tag.get(i..).and_then(|s| s.find('=')).map(|j| i + j) {
        let name = tag.get(i..eq).unwrap_or("").trim_end();
        let name = name.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
        let after = tag.get(eq + 1..).unwrap_or("");
        let open = eq + 1 + (after.len() - after.trim_start().len());
        let q = tag.get(open..).and_then(|s| s.chars().next()).ok_or(Untranslatable::Damaged)?;
        if q != '"' && q != '\'' {
            return Err(Untranslatable::Damaged);
        }
        let close = tag.get(open + 1..).and_then(|s| s.find(q)).map(|j| open + 1 + j).ok_or(Untranslatable::Damaged)?;
        out.push_str(tag.get(i..open + 1).unwrap_or(""));
        let value = tag.get(open + 1..close).unwrap_or("");
        if name == "xmlns" || name.starts_with("xmlns:") {
            out.push_str(translate_namespace(value)?);
        } else {
            out.push_str(value);
        }
        i = close;
    }
    out.push_str(tag.get(i..).unwrap_or(""));
    Ok(())
}

fn translate_namespace(uri: &str) -> Result<&str, Untranslatable> {
    if !uri.starts_with(STRICT_HOST) {
        return Ok(uri);
    }
    NAMESPACES.iter().find(|(s, _)| *s == uri).map(|(_, t)| *t).ok_or_else(|| Untranslatable::UnknownNamespace(uri.to_string()))
}

/// Whether kept DrawingML holds Strict-only coordinate values: Strict `ST_Coordinate` allows a
/// universal measure (`2.5in`, `3mm`) where Transitional needs a number of EMUs.
fn has_strict_values(xml: &str) -> Result<bool, Untranslatable> {
    if !xml.contains("drawingml/2006") {
        return Ok(false);
    }
    let not_a_number = |v: &[u8]| std::str::from_utf8(v).map_or(true, |v| v.trim().parse::<i64>().is_err());
    let strict_point = |e: &BytesStart<'_>| {
        matches!(e.local_name().as_ref(), b"off" | b"ext" | b"chOff" | b"chExt")
            && e.attributes().with_checks(false).flatten().any(|a| matches!(a.key.as_ref(), b"x" | b"y" | b"cx" | b"cy") && not_a_number(&a.value))
    };
    let mut reader = Reader::from_str(xml);
    let mut in_offset = false;
    loop {
        match reader.read_event().map_err(|_| Untranslatable::Damaged)? {
            Event::Eof => return Ok(false),
            Event::Start(e) => {
                in_offset = matches!(e.local_name().as_ref(), b"colOff" | b"rowOff");
                if strict_point(&e) {
                    return Ok(true);
                }
            }
            Event::Empty(e) => {
                in_offset = false;
                if strict_point(&e) {
                    return Ok(true);
                }
            }
            Event::Text(t) if in_offset => {
                if not_a_number(t.as_ref()) {
                    return Ok(true);
                }
                in_offset = false;
            }
            Event::End(_) => in_offset = false,
            _ => {}
        }
    }
}

/// Kept XML from a Strict package, ready to be saved as Transitional.
pub fn to_transitional(xml: &str) -> Result<Cow<'_, str>, Untranslatable> {
    let out = translate_namespaces(xml)?;
    if has_strict_values(&out)? {
        return Err(Untranslatable::StrictValue);
    }
    Ok(out)
}

/// Whether a kept part holds XML (translated) rather than binary data (kept byte for byte).
pub fn is_xml_part(name: &str, content_type: &str) -> bool {
    let ct = content_type.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    ct.ends_with("xml") || ct.contains("vmldrawing") || name.ends_with(".xml") || name.ends_with(".vml") || name.ends_with(".rels")
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

    #[test]
    fn strict_relationship_types_get_their_transitional_names() {
        let rows = [
            ("officeDocument", "officeDocument"),
            ("worksheet", "worksheet"),
            ("ctrlProp", "ctrlProp"),
            ("control", "control"),
            ("oleObject", "oleObject"),
            ("package", "package"),
            ("image", "image"),
            ("printerSettings", "printerSettings"),
            ("customXml", "customXml"),
            ("connections", "connections"),
            ("xmlMaps", "xmlMaps"),
            ("queryTable", "queryTable"),
            ("externalLink", "externalLink"),
            ("vmlDrawing", "vmlDrawing"),
            ("extendedProperties", "extended-properties"),
            ("customProperties", "custom-properties"),
            ("customXmlProperties", "customXmlProps"),
        ];
        for (strict, transitional) in rows {
            assert_eq!(strict_rel_type_to_transitional(&format!("{STRICT_REL_PREFIX}{strict}")), format!("{T}{transitional}"), "{strict}");
        }
    }

    #[test]
    fn other_relationship_types_are_unchanged() {
        for ty in [
            "http://schemas.microsoft.com/office/2006/relationships/activeXControlBinary",
            "http://schemas.microsoft.com/office/2007/relationships/ui/extensibility",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps",
            "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
        ] {
            assert!(matches!(strict_rel_type_to_transitional(ty), Cow::Borrowed(t) if t == ty), "{ty}");
        }
    }

    #[test]
    fn every_namespace_row_is_translated_in_declarations_only() {
        for (strict, transitional) in NAMESPACES {
            let x = format!(r#"<?xml version="1.0"?><a:r xmlns:a="{strict}" xmlns = '{strict}' v="{strict}"><!-- {strict} --><b>{strict}</b></a:r>"#);
            let want = format!(
                r#"<?xml version="1.0"?><a:r xmlns:a="{transitional}" xmlns = '{transitional}' v="{strict}"><!-- {strict} --><b>{strict}</b></a:r>"#
            );
            assert_eq!(translate_namespaces(&x).as_deref(), Ok(want.as_str()));
        }
    }

    #[test]
    fn transitional_xml_is_borrowed_unchanged() {
        let x = r#"<a xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"/>"#;
        assert!(matches!(translate_namespaces(x), Ok(Cow::Borrowed(_))));
    }

    #[test]
    fn an_unknown_strict_namespace_is_reported() {
        let x = r#"<a xmlns:z="http://purl.oclc.org/ooxml/somethingNew/main"/>"#;
        assert_eq!(translate_namespaces(x), Err(Untranslatable::UnknownNamespace("http://purl.oclc.org/ooxml/somethingNew/main".into())));
    }

    #[test]
    fn damaged_xml_is_reported() {
        assert_eq!(translate_namespaces(r#"<a xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main"#), Err(Untranslatable::Damaged));
    }

    #[test]
    fn universal_measures_are_untranslatable() {
        let ns = r#"xmlns:a="http://purl.oclc.org/ooxml/drawingml/main" xmlns:xdr="http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing""#;
        let ok = format!(r#"<xdr:wsDr {ns}><xdr:from><xdr:colOff>0</xdr:colOff></xdr:from><a:off x="10" y="-3"/></xdr:wsDr>"#);
        assert!(to_transitional(&ok).is_ok_and(|x| !x.contains("purl.oclc.org")));
        let attr = format!(r#"<xdr:wsDr {ns}><a:ext cx="2.5in" cy="1"/></xdr:wsDr>"#);
        assert_eq!(to_transitional(&attr), Err(Untranslatable::StrictValue));
        let text = format!(r#"<xdr:wsDr {ns}><xdr:from><xdr:rowOff>3mm</xdr:rowOff></xdr:from></xdr:wsDr>"#);
        assert_eq!(to_transitional(&text), Err(Untranslatable::StrictValue));
    }
}
