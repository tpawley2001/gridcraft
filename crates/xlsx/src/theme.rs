//! `theme1.xml`: editable colours and fonts, with source format styles and extensions.
//!
//! In the file the colour scheme is ordered dk1, lt1, dk2, lt2, accent1–6, hlink, folHlink,
//! while cell colour `theme` indices (and the model's `Theme.colors`) use lt1, dk1, lt2, dk2, ….

use std::fmt::Write as _;

use gridcraft_model::Theme;

use crate::xml::{El, esc_attr};

const MAX_SCRIPT_FONTS: usize = 64;
const MAX_SCRIPT_FONT_CHARS: usize = 256;

const SLOTS: [(&str, usize); 12] = [
    ("dk1", 1),
    ("lt1", 0),
    ("dk2", 3),
    ("lt2", 2),
    ("accent1", 4),
    ("accent2", 5),
    ("accent3", 6),
    ("accent4", 7),
    ("accent5", 8),
    ("accent6", 9),
    ("hlink", 10),
    ("folHlink", 11),
];

fn scheme_rgb(e: &El) -> Option<u32> {
    if let Some(c) = e.child("srgbClr").and_then(|c| c.attr("val")) {
        return u32::from_str_radix(c.trim(), 16).ok().map(|v| v & 0xFFFFFF);
    }
    if let Some(c) = e.child("sysClr") {
        if let Some(v) = c.attr("lastClr").and_then(|v| u32::from_str_radix(v.trim(), 16).ok()) {
            return Some(v & 0xFFFFFF);
        }
        return match c.attr("val") {
            Some("windowText") => Some(0x000000),
            Some("window") => Some(0xFFFFFF),
            _ => None,
        };
    }
    None
}

/// The typeface of `<fontScheme>/<font>/<kind>` (`latin`, `ea` or `cs`), `None` when absent or
/// empty (an empty typeface means "not set" and must not overwrite the default).
fn typeface<'a>(fs: &'a El, font: &str, kind: &str) -> Option<&'a str> {
    fs.path(&[font, kind]).and_then(|e| e.attr("typeface")).filter(|f| !f.is_empty())
}

pub fn read_theme(root: &El) -> Theme {
    let mut t = Theme::default();
    if let Some(n) = root.attr("name") {
        t.name = n.to_string();
    }
    let Some(el) = root.child("themeElements") else { return t };
    if let Some(cs) = el.child("clrScheme") {
        for (name, idx) in SLOTS {
            if let Some(v) = cs.child(name).and_then(scheme_rgb)
                && let Some(slot) = t.colors.get_mut(idx)
            {
                *slot = v;
            }
        }
    }
    if let Some(fs) = el.child("fontScheme") {
        if let Some(f) = typeface(fs, "majorFont", "latin") {
            t.major_font = f.to_string();
        }
        if let Some(f) = typeface(fs, "minorFont", "latin") {
            t.minor_font = f.to_string();
        }
        if let Some(f) = typeface(fs, "majorFont", "ea") {
            t.major_font_ea = f.to_string();
        }
        if let Some(f) = typeface(fs, "minorFont", "ea") {
            t.minor_font_ea = f.to_string();
        }
        if let Some(f) = typeface(fs, "majorFont", "cs") {
            t.major_font_cs = f.to_string();
        }
        if let Some(f) = typeface(fs, "minorFont", "cs") {
            t.minor_font_cs = f.to_string();
        }
        t.major_font_scripts = fs.child("majorFont").map(script_fonts).unwrap_or_default();
        t.minor_font_scripts = fs.child("minorFont").map(script_fonts).unwrap_or_default();
    }
    t
}

/// Keep only the unmodelled theme elements; colours and fonts remain editable.
/// Offsets refer to the UTF-8 source so whitespace, prefixes and attribute spelling survive.
pub fn keep_source_xml(t: &mut Theme, bytes: &[u8]) -> Result<(), crate::IoError> {
    use quick_xml::{Reader, XmlVersion, events::Event};
    let data = crate::xml::to_utf8(bytes);
    let text = std::str::from_utf8(&data).map_err(|e| crate::IoError::Xml(e.to_string()))?;
    let mut reader = Reader::from_str(text);
    let mut path: Vec<String> = Vec::new();
    let mut capture: Option<(String, usize, usize)> = None;
    let mut element_decls = Vec::new();
    loop {
        let start = usize::try_from(reader.buffer_position()).map_err(|e| crate::IoError::Xml(e.to_string()))?;
        let event = reader.read_event().map_err(|e| crate::IoError::Xml(e.to_string()))?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if path.len() >= crate::xml::MAX_DEPTH {
                    return Err(crate::IoError::Xml("theme XML nested too deeply".into()));
                }
                if path.is_empty() || (path.len() == 1 && name == "themeElements") {
                    for a in e.attributes().flatten() {
                        let key = a.key.as_ref();
                        let prefix = if key == b"xmlns" { Some("") } else { key.strip_prefix(b"xmlns:").and_then(|p| std::str::from_utf8(p).ok()) };
                        if let Some(prefix) = prefix {
                            let uri = a.normalized_value(XmlVersion::Explicit1_0).map_err(|e| crate::IoError::Xml(e.to_string()))?;
                            let decl = (prefix.to_string(), uri.into_owned());
                            if path.is_empty() {
                                t.xml_namespaces.push(decl);
                            } else {
                                element_decls.push((decl.0, esc_attr(&decl.1)));
                            }
                        }
                    }
                }
                let slot = (path.len() == 1 && matches!(name.as_str(), "objectDefaults" | "extraClrSchemeLst" | "custClrLst" | "extLst"))
                    || (path.len() == 2 && path.last().is_some_and(|p| p == "themeElements") && name == "fmtScheme");
                if capture.is_none() && slot {
                    capture = Some((name.clone(), start, path.len()));
                }
                if matches!(event, Event::Start(_)) {
                    path.push(name);
                }
            }
            Event::End(_) => {
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        if capture.as_ref().is_some_and(|(_, _, depth)| path.len() == *depth) {
            let end = usize::try_from(reader.buffer_position()).map_err(|e| crate::IoError::Xml(e.to_string()))?;
            if let Some((slot, start, _)) = capture.take()
                && let Some(fragment) = text.get(start..end)
            {
                let xml = if slot == "fmtScheme" && !element_decls.is_empty() {
                    crate::passthrough::with_decls(fragment, &element_decls)
                } else {
                    fragment.to_string()
                };
                match slot.as_str() {
                    "fmtScheme" => t.fmt_scheme_xml = Some(xml),
                    "objectDefaults" => t.object_defaults_xml = Some(xml),
                    "extraClrSchemeLst" => t.extra_clr_scheme_lst_xml = Some(xml),
                    "custClrLst" => t.cust_clr_lst_xml = Some(xml),
                    "extLst" => t.ext_lst_xml = Some(xml),
                    _ => {}
                }
            }
        }
    }
    // Retaining a byte-identical fallback adds no information. Keep a new workbook's theme
    // equal after saving and reopening, including themes imported from other file formats.
    if t.fmt_scheme_xml.as_deref() == Some(generated_format_scheme().as_str()) {
        t.fmt_scheme_xml = None;
    }
    if t.object_defaults_xml.as_deref() == Some("<a:objectDefaults/>") {
        t.object_defaults_xml = None;
    }
    if t.extra_clr_scheme_lst_xml.as_deref() == Some("<a:extraClrSchemeLst/>") {
        t.extra_clr_scheme_lst_xml = None;
    }
    if [&t.fmt_scheme_xml, &t.object_defaults_xml, &t.extra_clr_scheme_lst_xml, &t.cust_clr_lst_xml, &t.ext_lst_xml].iter().all(|v| v.is_none()) {
        t.xml_namespaces.clear();
    }
    Ok(())
}

/// The `<a:font script typeface>` entries of a font collection, in file order. A hostile file is
/// held to [`MAX_SCRIPT_FONTS`] entries of at most [`MAX_SCRIPT_FONT_CHARS`] characters each.
fn script_fonts(collection: &El) -> Vec<(String, String)> {
    let capped = |v: Option<&str>| v.unwrap_or_default().chars().take(MAX_SCRIPT_FONT_CHARS).collect::<String>();
    collection.kids("font").take(MAX_SCRIPT_FONTS).map(|e| (capped(e.attr("script")), capped(e.attr("typeface")))).collect()
}

fn script_fonts_xml(fonts: &[(String, String)]) -> String {
    fonts.iter().map(|(script, face)| format!("<a:font script=\"{}\" typeface=\"{}\"/>", esc_attr(script), esc_attr(face))).collect()
}

/// A minimal, schema-complete theme part for `t`.
pub fn write_theme(t: &Theme) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    let name = if t.name.is_empty() { "Craft" } else { &t.name };
    let _ = write!(s, "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"{}\"", esc_attr(name));
    for (prefix, uri) in &t.xml_namespaces {
        if prefix == "a" {
            continue;
        }
        if prefix.is_empty() {
            let _ = write!(s, " xmlns=\"{}\"", esc_attr(uri));
        } else {
            let _ = write!(s, " xmlns:{}=\"{}\"", prefix, esc_attr(uri));
        }
    }
    let fragments = [&t.fmt_scheme_xml, &t.object_defaults_xml, &t.extra_clr_scheme_lst_xml, &t.cust_clr_lst_xml, &t.ext_lst_xml];
    if fragments.iter().filter_map(|x| x.as_deref()).any(|x| x.contains("r:")) && !t.xml_namespaces.iter().any(|(p, _)| p == "r") {
        let _ = write!(s, " xmlns:r=\"{}\"", crate::write::NS_REL);
    }
    s.push_str("><a:themeElements>");
    let _ = write!(s, "<a:clrScheme name=\"{}\">", esc_attr(name));
    for (slot, idx) in SLOTS {
        let rgb = t.colors.get(idx).copied().unwrap_or(0) & 0xFFFFFF;
        let _ = write!(s, "<a:{slot}><a:srgbClr val=\"{rgb:06X}\"/></a:{slot}>");
    }
    s.push_str("</a:clrScheme>");
    let _ = write!(
        s,
        "<a:fontScheme name=\"{}\"><a:majorFont><a:latin typeface=\"{}\"/><a:ea typeface=\"{}\"/><a:cs typeface=\"{}\"/>{}</a:majorFont><a:minorFont><a:latin typeface=\"{}\"/><a:ea typeface=\"{}\"/><a:cs typeface=\"{}\"/>{}</a:minorFont></a:fontScheme>",
        esc_attr(name),
        esc_attr(&t.major_font),
        esc_attr(&t.major_font_ea),
        esc_attr(&t.major_font_cs),
        script_fonts_xml(&t.major_font_scripts),
        esc_attr(&t.minor_font),
        esc_attr(&t.minor_font_ea),
        esc_attr(&t.minor_font_cs),
        script_fonts_xml(&t.minor_font_scripts)
    );
    if let Some(xml) = &t.fmt_scheme_xml {
        s.push_str(xml);
    } else {
        s.push_str(&generated_format_scheme());
    }
    s.push_str("</a:themeElements>");
    s.push_str(t.object_defaults_xml.as_deref().unwrap_or("<a:objectDefaults/>"));
    s.push_str(t.extra_clr_scheme_lst_xml.as_deref().unwrap_or("<a:extraClrSchemeLst/>"));
    if let Some(xml) = &t.cust_clr_lst_xml {
        s.push_str(xml);
    }
    if let Some(xml) = &t.ext_lst_xml {
        s.push_str(xml);
    }
    s.push_str("</a:theme>");
    s
}

/// Original fallback format scheme, also used to recognize our own generated defaults on import.
fn generated_format_scheme() -> String {
    let mut s = String::new();
    // Format scheme: three entries per list, as the schema requires.
    s.push_str("<a:fmtScheme name=\"Craft\"><a:fillStyleLst>");
    for _ in 0..3 {
        s.push_str("<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>");
    }
    s.push_str("</a:fillStyleLst><a:lnStyleLst>");
    for w in [6350, 12700, 19050] {
        let _ = write!(
            s,
            "<a:ln w=\"{w}\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>"
        );
    }
    s.push_str("</a:lnStyleLst><a:effectStyleLst>");
    for _ in 0..3 {
        s.push_str("<a:effectStyle><a:effectLst/></a:effectStyle>");
    }
    s.push_str("</a:effectStyleLst><a:bgFillStyleLst>");
    for _ in 0..3 {
        s.push_str("<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>");
    }
    s.push_str("</a:bgFillStyleLst></a:fmtScheme>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_fragments_keep_prefixes_whitespace_and_schema_order() -> Result<(), crate::IoError> {
        let source = r#"<d:theme xmlns:d="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:x="urn:theme:test" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><d:themeElements xmlns:y="urn:theme:local?x=1&amp;y=2"><d:clrScheme/><d:fontScheme/><d:fmtScheme name='Original'>
<!-- retained --> <d:fillStyleLst><y:fill x:flag='yes'/></d:fillStyleLst></d:fmtScheme></d:themeElements><d:objectDefaults/><d:extraClrSchemeLst><d:extraClrScheme/></d:extraClrSchemeLst><d:custClrLst/><d:extLst><d:ext uri='test'><x:payload r:link='externalLink'/></d:ext></d:extLst></d:theme>"#;
        let mut theme = read_theme(&crate::xml::parse(source.as_bytes())?);
        keep_source_xml(&mut theme, source.as_bytes())?;
        theme.minor_font = "Test Body".into();
        let saved = write_theme(&theme);
        let parsed = crate::xml::parse(saved.as_bytes())?;
        assert_eq!(
            parsed.children.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["themeElements", "objectDefaults", "extraClrSchemeLst", "custClrLst", "extLst"]
        );
        assert!(saved.contains("<!-- retained --> <d:fillStyleLst><y:fill x:flag='yes'/>"));
        assert!(saved.contains(r#"xmlns:d="http://schemas.openxmlformats.org/drawingml/2006/main""#));
        assert!(saved.contains(r#"xmlns:x="urn:theme:test""#));
        assert!(saved.contains(r#"xmlns:y="urn:theme:local?x=1&amp;y=2""#));
        assert!(saved.contains(r#"<a:latin typeface="Test Body"/>"#));
        let mut again = read_theme(&parsed);
        keep_source_xml(&mut again, saved.as_bytes())?;
        assert_eq!(again.fmt_scheme_xml, theme.fmt_scheme_xml);
        assert_eq!(again.ext_lst_xml, theme.ext_lst_xml);
        Ok(())
    }

    #[test]
    fn external_theme_relationship_keeps_its_id_across_two_saves() -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Read as _;
        let original = crate::write_xlsx(&gridcraft_model::Workbook::new())?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&original))?;
        let mut package = crate::package::ZipOut::new();
        for i in 0..zip.len() {
            let mut part = zip.by_index(i)?;
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes)?;
            if part.name() == "xl/theme/theme1.xml" {
                let xml = String::from_utf8(bytes)?;
                bytes = xml
                    .replace("<a:objectDefaults/>", r#"<a:objectDefaults><a:spDef><a:blip r:link="externalLink"/></a:spDef></a:objectDefaults>"#)
                    .replace("<a:theme ", r#"<a:theme xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#)
                    .into_bytes();
            }
            package.add(part.name(), &bytes)?;
        }
        package.add("xl/theme/_rels/theme1.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="externalLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://example.invalid/picture?a=1&amp;b=2" TargetMode="External"/></Relationships>"#)?;
        let mut bytes = package.finish()?;
        for _ in 0..2 {
            let (wb, _) = crate::read_xlsx(&bytes)?;
            bytes = crate::write_xlsx(&wb)?;
            let mut pkg = crate::package::Package::open(&bytes)?;
            let rels = pkg.rels("xl/theme/theme1.xml")?;
            assert_eq!(rels.len(), 1);
            let rel = rels.first().ok_or("missing theme relationship")?;
            assert_eq!(rel.id, "externalLink");
            assert!(rel.external);
            assert_eq!(rel.target, "https://example.invalid/picture?a=1&b=2");
            let theme = pkg.read("xl/theme/theme1.xml")?.ok_or("missing theme")?;
            assert!(String::from_utf8(theme)?.contains(r#"r:link="externalLink""#));
        }
        Ok(())
    }

    /// A Japanese Office theme: the heading font sets latin, ea and cs; the body font only ea.
    const EA_THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"/><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface="游ゴシック"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:ea typeface="ＭＳ Ｐゴシック"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;

    #[test]
    fn roundtrip() {
        let mut t = Theme::default();
        t.colors[0] = 0xFEFEFE;
        t.colors[1] = 0x101010;
        t.colors[9] = 0x123456;
        t.major_font = "Cambria".into();
        t.name = "Mine".into();
        let x = write_theme(&t);
        let back = read_theme(&crate::xml::parse(x.as_bytes()).unwrap());
        assert_eq!(back, t);
    }

    #[test]
    fn ea_and_cs_typefaces_roundtrip() {
        let t = read_theme(&crate::xml::parse(EA_THEME.as_bytes()).unwrap());
        assert_eq!(t.major_font, "Calibri");
        assert_eq!(t.major_font_ea, "游ゴシック");
        assert_eq!(t.major_font_cs, "Arial");
        // No `<a:latin>` under the minor font: the default stays.
        assert_eq!(t.minor_font, Theme::default().minor_font);
        assert_eq!(t.minor_font_ea, "ＭＳ Ｐゴシック");
        assert!(t.minor_font_cs.is_empty());

        let x = write_theme(&t);
        assert!(
            x.contains(r#"<a:majorFont><a:latin typeface="Calibri"/><a:ea typeface="游ゴシック"/><a:cs typeface="Arial"/></a:majorFont>"#),
            "{x}"
        );
        assert!(
            x.contains(r#"<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface="ＭＳ Ｐゴシック"/><a:cs typeface=""/></a:minorFont>"#),
            "{x}"
        );
        assert_eq!(read_theme(&crate::xml::parse(x.as_bytes()).unwrap()), t);
    }

    #[test]
    fn theme_without_ea_or_cs_writes_empty_attributes() {
        let t = Theme::default();
        let x = write_theme(&t);
        assert!(x.contains(r#"<a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>"#), "{x}");
        assert!(x.contains(r#"<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>"#), "{x}");
    }

    #[test]
    fn explicit_empty_ea_and_cs_read_as_unset() {
        let xml = EA_THEME
            .replace(r#"<a:ea typeface="游ゴシック"/>"#, r#"<a:ea typeface=""/>"#)
            .replace(r#"<a:ea typeface="ＭＳ Ｐゴシック"/>"#, r#"<a:ea typeface=""/>"#);
        let t = read_theme(&crate::xml::parse(xml.as_bytes()).unwrap());
        assert!(t.major_font_ea.is_empty() && t.minor_font_ea.is_empty());
        assert_eq!(t.major_font_cs, "Arial");
        let x = write_theme(&t);
        assert!(x.contains(r#"<a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface="Arial"/></a:majorFont>"#), "{x}");
        assert_eq!(read_theme(&crate::xml::parse(x.as_bytes()).unwrap()), t);
    }

    #[test]
    fn ea_and_cs_typefaces_are_escaped() {
        let t = Theme { major_font_ea: "游&ゴシック".into(), minor_font_cs: "A<B>\"C\"".into(), ..Theme::default() };
        let x = write_theme(&t);
        assert!(x.contains(r#"<a:ea typeface="游&amp;ゴシック"/>"#), "{x}");
        assert!(x.contains(r#"<a:cs typeface="A&lt;B&gt;&quot;C&quot;"/>"#), "{x}");
        assert_eq!(read_theme(&crate::xml::parse(x.as_bytes()).unwrap()), t);
    }

    /// An Arabic Office theme: Arabic, Hebrew and Japanese faces in the per-script lists.
    const SCRIPT_THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface="Times New Roman"/><a:font script="Arab" typeface="Times New Roman"/><a:font script="Hebr" typeface="Times New Roman"/><a:font script="Jpan" typeface="游ゴシック Light"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface="Arial"/><a:font script="Arab" typeface="Arial"/><a:font script="Hebr" typeface="Arial"/><a:font script="Jpan" typeface="游ゴシック"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;

    #[test]
    fn per_script_typefaces_roundtrip_in_file_order() {
        let t = read_theme(&crate::xml::parse(SCRIPT_THEME.as_bytes()).unwrap());
        let pairs = |v: &[(String, String)]| v.iter().map(|(s, f)| (s.clone(), f.clone())).collect::<Vec<_>>();
        let expect =
            |a: &str, j: &str| vec![("Arab".to_string(), a.to_string()), ("Hebr".to_string(), a.to_string()), ("Jpan".to_string(), j.to_string())];
        assert_eq!(pairs(&t.major_font_scripts), expect("Times New Roman", "游ゴシック Light"));
        assert_eq!(pairs(&t.minor_font_scripts), expect("Arial", "游ゴシック"));
        let x = write_theme(&t);
        assert!(
            x.contains(r#"<a:cs typeface="Times New Roman"/><a:font script="Arab" typeface="Times New Roman"/><a:font script="Hebr" typeface="Times New Roman"/><a:font script="Jpan" typeface="游ゴシック Light"/></a:majorFont>"#),
            "{x}"
        );
        assert_eq!(read_theme(&crate::xml::parse(x.as_bytes()).unwrap()), t);
    }

    #[test]
    fn hostile_per_script_lists_are_capped() {
        let entries: String = (0..100).map(|i| format!(r#"<a:font script="S{i}" typeface="{}"/>"#, "x".repeat(300))).collect();
        let xml = format!(
            r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fontScheme><a:majorFont><a:latin typeface="Calibri"/>{entries}</a:majorFont></a:fontScheme></a:themeElements></a:theme>"#
        );
        let t = read_theme(&crate::xml::parse(xml.as_bytes()).unwrap());
        assert_eq!(t.major_font_scripts.len(), 64);
        assert!(t.major_font_scripts.iter().all(|(script, face)| script.chars().count() <= 256 && face.chars().count() <= 256));
    }
}
