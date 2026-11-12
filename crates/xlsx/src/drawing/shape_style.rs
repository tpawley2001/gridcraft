//! A shape's style element (`xdr:style`, CT_ShapeStyle, ECMA-376 Part 1 §20.5.2.30): references
//! into the theme's format scheme (§20.1.4.1.14) for the fill (`a:fillRef`, §20.1.4.2.10), outline
//! (`a:lnRef`, §20.1.4.2.19), effects (`a:effectRef`, §20.1.4.2.8) and font (`a:fontRef`,
//! §20.1.4.1.17). Read into the model with what the theme's entries draw, written back as read.

use std::fmt::Write as _;

use gridcraft_model::{Color, KeptDmlColor, ShapeFontIdx, ShapeFontRef, ShapeStyle, ShapeStyleRef};

use super::{DML_COLOR_TRANSFORMS, SCHEME_COLOR_VALS, dml_color_resolved};
use crate::xml::El;

/// What one entry of a format scheme list draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SchemeEntry {
    /// False for an `a:noFill` entry.
    pub draws: bool,
    /// The entry's own colour, when it isn't the placeholder colour (`phClr`).
    pub color: Option<Color>,
    /// An outline entry's width in EMU.
    pub width: Option<i32>,
}

/// The parts of a theme's format scheme a shape style draws with. Entries beyond what a theme
/// can reasonably hold are not read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FormatScheme {
    pub fills: Vec<SchemeEntry>,
    pub lines: Vec<SchemeEntry>,
    pub effects: usize,
    pub bg_fills: Vec<SchemeEntry>,
}

/// More entries than any theme needs (the schema asks for at least three per list).
const MAX_ENTRIES: usize = 64;

const FILLS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

impl Default for FormatScheme {
    /// The built-in theme's: solid placeholder fills and outlines of 6350, 12700 and 19050 EMU.
    fn default() -> Self {
        let solid = SchemeEntry { draws: true, color: None, width: None };
        FormatScheme {
            fills: vec![solid; 3],
            lines: [6350, 12700, 19050].into_iter().map(|w| SchemeEntry { width: Some(w), ..solid }).collect(),
            effects: 3,
            bg_fills: vec![solid; 3],
        }
    }
}

fn fill_entry(fill: &El, theme: &[u32; 12]) -> SchemeEntry {
    let color = if fill.name == "solidFill" { dml_color_resolved(fill, theme) } else { None };
    SchemeEntry { draws: fill.name != "noFill", color, width: None }
}

impl FormatScheme {
    /// The format scheme `a:fmtScheme`, resolved against the theme colours.
    pub fn read(fs: &El, theme: &[u32; 12]) -> Self {
        let fills = |list: &str| -> Vec<SchemeEntry> {
            fs.child(list)
                .map(|l| l.children.iter().filter(|f| FILLS.contains(&f.name.as_str())).take(MAX_ENTRIES).map(|f| fill_entry(f, theme)).collect())
                .unwrap_or_default()
        };
        let lines = fs
            .child("lnStyleLst")
            .map(|l| {
                l.kids("ln")
                    .take(MAX_ENTRIES)
                    .map(|ln| {
                        let fill = ln.children.iter().find(|f| FILLS.contains(&f.name.as_str()));
                        let mut e = fill.map_or(SchemeEntry { draws: true, color: None, width: None }, |f| fill_entry(f, theme));
                        e.width = line_width(ln);
                        e
                    })
                    .collect()
            })
            .unwrap_or_default();
        let effects = fs.child("effectStyleLst").map_or(0, |l| l.kids("effectStyle").take(MAX_ENTRIES).count());
        FormatScheme { fills: fills("fillStyleLst"), lines, effects, bg_fills: fills("bgFillStyleLst") }
    }

    /// The fill entry `idx` picks: 1.. in `fillStyleLst`, 1001.. in `bgFillStyleLst`.
    fn fill(&self, idx: u32) -> Option<&SchemeEntry> {
        match idx {
            0 => None,
            1..=999 => self.fills.get(idx as usize - 1),
            _ => self.bg_fills.get(idx.checked_sub(1001)? as usize),
        }
    }

    fn line(&self, idx: u32) -> Option<&SchemeEntry> {
        self.lines.get(idx.checked_sub(1)? as usize)
    }
}

/// ST_LineWidth: 0..=20116800 EMU.
pub(crate) fn line_width(ln: &El) -> Option<i32> {
    ln.attr("w").and_then(|w| w.trim().parse::<i32>().ok()).filter(|w| (0..=20_116_800).contains(w))
}

/// Whether `e` (`xdr:spPr` or `a:ln`) states a fill of its own.
fn has_fill(e: &El) -> bool {
    e.children.iter().any(|c| FILLS.contains(&c.name.as_str()))
}

/// A short token attribute (`sysClr`/`prstClr` names): ASCII letters and digits only.
fn token(v: &str) -> Option<&str> {
    let v = v.trim();
    (!v.is_empty() && v.len() <= 32 && v.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(v)
}

/// The colour child of `e` (any EG_ColorChoice element) rebuilt from parsed values, with its
/// colour transforms in document order; `None` when it has none or its value is malformed.
pub(crate) fn kept_color_choice(e: &El) -> Option<String> {
    let el = e.children.iter().find(|c| matches!(c.name.as_str(), "scrgbClr" | "srgbClr" | "hslClr" | "sysClr" | "schemeClr" | "prstClr"))?;
    let int = |k: &str| el.attr(k).and_then(|v| v.trim().parse::<i64>().ok());
    let attrs = match el.name.as_str() {
        "srgbClr" => {
            let Some(Color::Rgb(v)) = el.attr("val").and_then(Color::from_hex) else { return None };
            format!(" val=\"{:06X}\"", v & 0xFF_FFFF)
        }
        "schemeClr" => {
            let v = el.attr("val")?;
            format!(" val=\"{}\"", SCHEME_COLOR_VALS.iter().find(|s| **s == v)?)
        }
        "sysClr" => {
            let mut a = format!(" val=\"{}\"", token(el.attr("val")?)?);
            if let Some(Color::Rgb(v)) = el.attr("lastClr").and_then(Color::from_hex) {
                let _ = write!(a, " lastClr=\"{:06X}\"", v & 0xFF_FFFF);
            }
            a
        }
        "prstClr" => format!(" val=\"{}\"", token(el.attr("val")?)?),
        "scrgbClr" => format!(" r=\"{}\" g=\"{}\" b=\"{}\"", int("r")?, int("g")?, int("b")?),
        _ => format!(" hue=\"{}\" sat=\"{}\" lum=\"{}\"", int("hue")?, int("sat")?, int("lum")?),
    };
    let mut kids = String::new();
    for k in &el.children {
        let Some(&(name, has_val)) = DML_COLOR_TRANSFORMS.iter().find(|(n, _)| *n == k.name) else { continue };
        if has_val {
            let Some(v) = k.attr("val").and_then(|v| v.trim().parse::<i64>().ok()) else { continue };
            let _ = write!(kids, "<a:{name} val=\"{v}\"/>");
        } else {
            let _ = write!(kids, "<a:{name}/>");
        }
    }
    let tag = &el.name;
    Some(if kids.is_empty() { format!("<a:{tag}{attrs}/>") } else { format!("<a:{tag}{attrs}>{kids}</a:{tag}>") })
}

/// The colour child of a reference, kept and resolved.
fn ref_color(e: &El, theme: &[u32; 12]) -> Option<KeptDmlColor> {
    let xml = kept_color_choice(e)?;
    Some(KeptDmlColor { color: dml_color_resolved(e, theme).unwrap_or_default(), xml })
}

fn style_ref(e: &El, theme: &[u32; 12], entry: impl Fn(u32) -> Option<SchemeEntry>) -> ShapeStyleRef {
    let idx = e.attr_u32("idx").unwrap_or(0);
    let entry = entry(idx);
    ShapeStyleRef {
        idx,
        color: ref_color(e, theme).unwrap_or_default(),
        draws: idx != 0 && entry.is_some_and(|e| e.draws),
        theme_color: entry.and_then(|e| e.color),
        width_emu: entry.and_then(|e| e.width),
    }
}

/// The shape's style `st`, with whether its properties `sp` (and outline `ln`) state their own
/// fill and outline.
pub(crate) fn read(st: &El, sp: &El, ln: Option<&El>, theme: &[u32; 12], scheme: &FormatScheme) -> ShapeStyle {
    ShapeStyle {
        line: st.child("lnRef").map(|e| style_ref(e, theme, |i| scheme.line(i).copied())),
        fill: st.child("fillRef").map(|e| style_ref(e, theme, |i| scheme.fill(i).copied())),
        effect: st.child("effectRef").map(|e| {
            style_ref(e, theme, |i| (i >= 1 && (i as usize) <= scheme.effects).then_some(SchemeEntry { draws: true, color: None, width: None }))
        }),
        font: st
            .child("fontRef")
            .map(|e| ShapeFontRef { idx: e.attr("idx").and_then(ShapeFontIdx::from_xml).unwrap_or_default(), color: ref_color(e, theme) }),
        own_fill: has_fill(sp),
        own_line: ln.is_some_and(has_fill),
    }
}

/// The `xdr:style` element: the references in schema order, a missing one written as the
/// reference to nothing so the element stays valid.
pub(crate) fn write(s: &mut String, st: &ShapeStyle) {
    s.push_str("<xdr:style>");
    for (tag, r) in [("lnRef", &st.line), ("fillRef", &st.fill), ("effectRef", &st.effect)] {
        match r {
            Some(r) if !r.color.xml.is_empty() => {
                let _ = write!(s, "<a:{tag} idx=\"{}\">{}</a:{tag}>", r.idx, r.color.xml);
            }
            Some(r) => {
                let _ = write!(s, "<a:{tag} idx=\"{}\"/>", r.idx);
            }
            None => {
                let _ = write!(s, "<a:{tag} idx=\"0\"/>");
            }
        }
    }
    let font = st.font.as_ref();
    let idx = font.map_or(ShapeFontIdx::Minor, |f| f.idx).xml();
    match font.and_then(|f| f.color.as_ref()) {
        Some(c) => {
            let _ = write!(s, "<a:fontRef idx=\"{idx}\">{}</a:fontRef>", c.xml);
        }
        None => {
            let _ = write!(s, "<a:fontRef idx=\"{idx}\"/>");
        }
    }
    s.push_str("</xdr:style>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(xml: &str) -> El {
        crate::xml::parse(xml.as_bytes()).unwrap_or_default()
    }

    fn theme() -> [u32; 12] {
        gridcraft_model::Theme::default().colors
    }

    fn style(xml: &str) -> ShapeStyle {
        let sp = el("<spPr/>");
        read(&el(xml), &sp, None, &theme(), &FormatScheme::default())
    }

    #[test]
    fn every_colour_choice_is_kept_from_parsed_values() {
        for (xml, want) in [
            (r#"<r><a:scrgbClr r="1" g="2" b=" 3"/></r>"#, Some(r#"<a:scrgbClr r="1" g="2" b="3"/>"#)),
            (
                r#"<r><a:sysClr val="windowText" lastClr="000000"><a:alpha val="5"/></a:sysClr></r>"#,
                Some(r#"<a:sysClr val="windowText" lastClr="000000"><a:alpha val="5"/></a:sysClr>"#),
            ),
            (r#"<r><a:prstClr val="black"/></r>"#, Some(r#"<a:prstClr val="black"/>"#)),
            (r#"<r><a:hslClr hue="1" sat="2" lum="3"/></r>"#, Some(r#"<a:hslClr hue="1" sat="2" lum="3"/>"#)),
            (r#"<r><a:prstClr val="a&quot;b"/></r>"#, None),
            (r#"<r><a:schemeClr val="nope"/></r>"#, None),
            (r#"<r/>"#, None),
        ] {
            assert_eq!(kept_color_choice(&el(xml)).as_deref(), want, "{xml}");
        }
    }

    #[test]
    fn style_refs_resolve_against_the_default_scheme() {
        let st = style(
            r#"<s><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></s>"#,
        );
        let fill = st.fill.clone().unwrap_or_default();
        assert_eq!(fill.drawn(), Some(Color::Theme(4, 0)));
        let line = st.line.clone().unwrap_or_default();
        assert!(matches!(line.drawn(), Some(Color::Rgb(_))), "shade applied: {line:?}");
        assert_eq!(line.width_emu, Some(12700));
        assert_eq!(st.effect.clone().unwrap_or_default().drawn(), None);
        assert_eq!(st.font.as_ref().and_then(|f| f.color.as_ref()).map(|c| c.color), Some(Color::Theme(0, 0)));
        let mut out = String::new();
        write(&mut out, &st);
        assert!(out.starts_with("<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"><a:shade val=\"50000\"/>"), "{out}");
    }

    #[test]
    fn a_no_fill_theme_entry_and_index_zero_draw_nothing() {
        let fs = el(
            r#"<a:fmtScheme><a:fillStyleLst><a:noFill/><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill><a:gradFill/></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst></a:fmtScheme>"#,
        );
        let scheme = FormatScheme::read(&fs, &theme());
        let sp = el("<spPr/>");
        let st = |xml: &str| read(&el(xml), &sp, None, &theme(), &scheme);
        let c = r#"<a:schemeClr val="accent2"/>"#;
        let drawn = |i: u32| st(&format!(r#"<s><a:fillRef idx="{i}">{c}</a:fillRef></s>"#)).fill.and_then(|f| f.drawn());
        assert_eq!(drawn(0), None);
        assert_eq!(drawn(1), None);
        assert_eq!(drawn(2), Some(Color::Rgb(0x00FF00)));
        assert_eq!(drawn(3), Some(Color::Theme(5, 0)), "a gradient entry draws the colour flat");
        assert_eq!(drawn(4), None, "no such entry");
        assert_eq!(drawn(u32::MAX), None);
        let line = st(&format!(r#"<s><a:lnRef idx="1">{c}</a:lnRef></s>"#)).line.unwrap_or_default();
        assert_eq!((line.drawn(), line.width_emu), (Some(Color::Theme(5, 0)), Some(9525)));
    }

    #[test]
    fn a_missing_reference_is_written_as_none() {
        let mut out = String::new();
        write(&mut out, &ShapeStyle::default());
        assert_eq!(out, r#"<xdr:style><a:lnRef idx="0"/><a:fillRef idx="0"/><a:effectRef idx="0"/><a:fontRef idx="minor"/></xdr:style>"#);
    }
}
