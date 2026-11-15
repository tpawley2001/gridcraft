//! ODS page setup: each table's master page (ODF 1.2 Part 1 §16.9 `style:master-page`) and its
//! page layout (§16.5 `style:page-layout`, §17.2 `style:page-layout-properties`, §17.5
//! `style:header-footer-properties`) become the sheet's print settings, header and footer.
//! Everything here is optional: a missing or unreadable `styles.xml` leaves the defaults.

use std::collections::HashMap;
use std::sync::Arc;

use gridcraft_model::{Orientation, PageOrder, PrintSettings, Workbook};
use quick_xml::NsReader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

use crate::ReadReport;

const OFFICE: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const STYLE: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const TEXT: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const FO: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const SVG: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";

/// Largest `styles.xml` read for page setup.
pub(crate) const MAX_STYLES_PART: u64 = 32 * 1024 * 1024;
/// Elements kept from `styles.xml` (a hostile file can't make the tree unbounded).
const MAX_NODES: usize = 2_000_000;
/// Longest header or footer section built from a master page.
const MAX_SECTION: usize = 64 * 1024;
/// Largest `text:s text:c` honoured in a header or footer.
const MAX_SPACES: usize = 1024;

const WARNING: &str = "ODS import: the page styles (styles.xml) couldn't be read, so page setup, headers and footers were not imported.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ns {
    Office,
    Style,
    Text,
    Fo,
    Svg,
    Other,
}

fn ns_of(ns: ResolveResult<'_>) -> Ns {
    match ns {
        ResolveResult::Bound(n) => match n.as_ref() {
            OFFICE => Ns::Office,
            STYLE => Ns::Style,
            TEXT => Ns::Text,
            FO => Ns::Fo,
            SVG => Ns::Svg,
            _ => Ns::Other,
        },
        _ => Ns::Other,
    }
}

enum Node {
    El(El),
    Text(String),
}

struct El {
    ns: Ns,
    name: Vec<u8>,
    attrs: Vec<(Ns, Vec<u8>, String)>,
    children: Vec<Node>,
}

impl El {
    fn is(&self, ns: Ns, name: &[u8]) -> bool {
        self.ns == ns && self.name == name
    }
    fn attr(&self, ns: Ns, name: &[u8]) -> Option<&str> {
        self.attrs.iter().find(|(n, l, _)| *n == ns && l == name).map(|(_, _, v)| v.as_str())
    }
    fn elements(&self) -> impl Iterator<Item = &El> {
        self.children.iter().filter_map(|c| match c {
            Node::El(e) => Some(e),
            Node::Text(_) => None,
        })
    }
    fn child(&self, ns: Ns, name: &[u8]) -> Option<&El> {
        self.elements().find(|e| e.is(ns, name))
    }
}

fn element(reader: &mut NsReader<&[u8]>, e: &BytesStart<'_>) -> Option<El> {
    let (ns, name) = reader.resolver_mut().resolve_element(e.name());
    let ns = ns_of(ns);
    let name = name.as_ref().to_vec();
    let mut attrs = Vec::new();
    for a in e.attributes() {
        let a = a.ok()?;
        let (ans, local) = reader.resolver_mut().resolve_attribute(a.key);
        let ans = ns_of(ans);
        if ans == Ns::Other {
            continue;
        }
        let value = a.normalized_value(XmlVersion::Explicit1_0).ok()?;
        if value.len() > MAX_SECTION {
            return None;
        }
        attrs.push((ans, local.as_ref().to_vec(), value.into_owned()));
    }
    Some(El { ns, name, attrs, children: Vec::new() })
}

fn push_text(stack: &mut [El], text: &str) {
    let Some(top) = stack.last_mut() else { return };
    if let Some(Node::Text(t)) = top.children.last_mut() {
        if t.len() < MAX_SECTION {
            t.push_str(text);
        }
    } else {
        top.children.push(Node::Text(text.to_string()));
    }
}

fn close(stack: &mut Vec<El>, root: &mut Option<El>) -> Option<()> {
    let el = stack.pop()?;
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::El(el)),
        None => *root = Some(el),
    }
    Some(())
}

/// Parses `styles.xml` into a small element tree, or `None` when it is malformed or too large.
fn parse_tree(bytes: &[u8]) -> Option<El> {
    let mut reader = NsReader::from_reader(bytes);
    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;
    let mut nodes = 0usize;
    loop {
        match reader.read_event().ok()? {
            ev @ (Event::Start(_) | Event::Empty(_)) => {
                let (Event::Start(e) | Event::Empty(e)) = &ev else { return None };
                nodes += 1;
                if nodes > MAX_NODES || stack.len() >= crate::xml::MAX_DEPTH || root.is_some() {
                    return None;
                }
                stack.push(element(&mut reader, e)?);
                if matches!(ev, Event::Empty(_)) {
                    close(&mut stack, &mut root)?;
                }
            }
            Event::End(_) => close(&mut stack, &mut root)?,
            Event::Text(t) => {
                let text = t.xml_content(XmlVersion::Explicit1_0).ok()?;
                if stack.is_empty() {
                    if !text.trim().is_empty() {
                        return None;
                    }
                } else {
                    push_text(&mut stack, &text);
                }
            }
            Event::CData(t) => push_text(&mut stack, &t.decode().ok()?),
            Event::GeneralRef(r) => {
                let ch = match r.resolve_char_ref().ok()? {
                    Some(c) => c,
                    None => match r.as_ref() {
                        b"amp" => '&',
                        b"lt" => '<',
                        b"gt" => '>',
                        b"apos" => '\'',
                        b"quot" => '"',
                        _ => return None,
                    },
                };
                push_text(&mut stack, ch.encode_utf8(&mut [0; 4]));
            }
            Event::DocType(_) => return None,
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return None;
    }
    root.filter(|r| r.is(Ns::Office, b"document-styles"))
}

pub(crate) fn warn(report: &mut ReadReport) {
    report.warn(WARNING);
}

/// `style:style style:family="table"` names -> their `style:master-page-name`, read from the
/// automatic styles of `content.xml` (everything before `office:body`).
pub(crate) fn content_table_styles(content: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut reader = NsReader::from_reader(content);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let Some(el) = element(&mut reader, &e) else { continue };
                if el.is(Ns::Office, b"body") {
                    break;
                }
                table_style(&el, &mut out);
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Column widths and row heights of the automatic `table-column` / `table-row` styles in
/// `content.xml` (ODF 1.2 Part 1 §17.16 `style:table-column-properties`, §17.17
/// `style:table-row-properties`), in whole pixels at 96 dpi.
#[derive(Debug, Default)]
pub(crate) struct LineStyles {
    /// Column style name -> width in px.
    pub(crate) cols: HashMap<String, f32>,
    /// Row style name -> (height in px, `true` when the height is fixed rather than optimal).
    pub(crate) rows: HashMap<String, (f32, bool)>,
}

/// Most line styles kept from one file.
const MAX_LINE_STYLES: usize = 100_000;

/// Reads [`LineStyles`] from the automatic styles of `content.xml` (everything before
/// `office:body`). Unknown units, missing properties and zero or negative lengths are skipped.
pub(crate) fn content_line_styles(content: &[u8]) -> LineStyles {
    let mut out = LineStyles::default();
    let mut reader = NsReader::from_reader(content);
    // The enclosing `style:style`: (family, name).
    let mut current: Option<(String, String)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let Some(el) = element(&mut reader, &e) else { continue };
                if el.is(Ns::Office, b"body") {
                    break;
                }
                if el.is(Ns::Style, b"style") {
                    current = match (el.attr(Ns::Style, b"family"), el.attr(Ns::Style, b"name")) {
                        (Some(family), Some(name)) => Some((family.to_string(), name.to_string())),
                        _ => None,
                    };
                } else {
                    line_style(&el, current.as_ref(), &mut out);
                }
            }
            Ok(Event::Empty(e)) => {
                let Some(el) = element(&mut reader, &e) else { continue };
                line_style(&el, current.as_ref(), &mut out);
            }
            Ok(Event::End(e)) => {
                let (ns, name) = reader.resolver_mut().resolve_element(e.name());
                if ns_of(ns) == Ns::Style && name.as_ref() == b"style" {
                    current = None;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn line_style(el: &El, current: Option<&(String, String)>, out: &mut LineStyles) {
    let Some((family, name)) = current else { return };
    if out.cols.len().saturating_add(out.rows.len()) >= MAX_LINE_STYLES {
        return;
    }
    if family == "table-column" && el.is(Ns::Style, b"table-column-properties") {
        if let Some(px) = el.attr(Ns::Style, b"column-width").and_then(length_px) {
            out.cols.entry(name.clone()).or_insert(px);
        }
    } else if family == "table-row"
        && el.is(Ns::Style, b"table-row-properties")
        && let Some(px) = el.attr(Ns::Style, b"row-height").and_then(length_px)
    {
        let fixed = el.attr(Ns::Style, b"use-optimal-row-height").map(str::trim) == Some("false");
        out.rows.entry(name.clone()).or_insert((px, fixed));
    }
}

/// An ODF length in whole pixels at 96 dpi, rounded to the nearest pixel as Excel stores
/// widths and heights; `None` when unparsable or not at least half a pixel.
pub(crate) fn length_px(v: &str) -> Option<f32> {
    let px = (length(v)? * 96.0).round();
    (px >= 1.0).then_some(px as f32)
}

fn table_style(el: &El, out: &mut HashMap<String, String>) {
    if el.is(Ns::Style, b"style")
        && el.attr(Ns::Style, b"family") == Some("table")
        && let (Some(name), Some(master)) = (el.attr(Ns::Style, b"name"), el.attr(Ns::Style, b"master-page-name"))
        && out.len() < 100_000
    {
        out.entry(name.to_string()).or_insert_with(|| master.to_string());
    }
}

/// Applies each sheet's master page from `styles.xml`. `table_styles[i]` is sheet `i`'s
/// `table:style-name`; `content_styles` comes from [`content_table_styles`].
pub(crate) fn apply(
    styles: &[u8],
    content_styles: &HashMap<String, String>,
    table_styles: &[Option<String>],
    wb: &mut Workbook,
    report: &mut ReadReport,
) {
    let Some(root) = parse_tree(styles) else {
        warn(report);
        return;
    };
    let mut table_masters: HashMap<String, String> = HashMap::new();
    let mut layouts: HashMap<&str, &El> = HashMap::new();
    let mut masters: HashMap<&str, &El> = HashMap::new();
    for section in root.elements() {
        for el in section.elements() {
            match (section.ns, section.name.as_slice()) {
                (Ns::Office, b"styles" | b"automatic-styles") => {
                    table_style(el, &mut table_masters);
                    if el.is(Ns::Style, b"page-layout")
                        && let Some(name) = el.attr(Ns::Style, b"name")
                    {
                        layouts.entry(name).or_insert(el);
                    }
                }
                (Ns::Office, b"master-styles") => {
                    if el.is(Ns::Style, b"master-page")
                        && let Some(name) = el.attr(Ns::Style, b"name")
                    {
                        masters.entry(name).or_insert(el);
                    }
                }
                _ => {}
            }
        }
    }
    let mut cache: HashMap<&str, PrintSettings> = HashMap::new();
    for (sheet, style) in wb.sheets.iter_mut().zip(table_styles) {
        let master = style
            .as_deref()
            .and_then(|s| content_styles.get(s).or_else(|| table_masters.get(s)))
            .and_then(|m| masters.get_key_value(m.as_str()))
            .or_else(|| masters.get_key_value("Default"));
        let Some((&name, &master)) = master else { continue };
        let print = cache.entry(name).or_insert_with(|| {
            let mut p = sheet.print.clone();
            fill(&mut p, master, &layouts);
            p
        });
        if sheet.print != *print {
            Arc::make_mut(sheet).print = print.clone();
        }
    }
}

fn fill(p: &mut PrintSettings, master: &El, layouts: &HashMap<&str, &El>) {
    let header = master.child(Ns::Style, b"header");
    let footer = master.child(Ns::Style, b"footer");
    p.header = header.map(header_footer).unwrap_or_default();
    p.footer = footer.map(header_footer).unwrap_or_default();
    let Some(layout) = master.attr(Ns::Style, b"page-layout-name").and_then(|n| layouts.get(n)) else { return };
    let empty = El { ns: Ns::Other, name: Vec::new(), attrs: Vec::new(), children: Vec::new() };
    let props = layout.child(Ns::Style, b"page-layout-properties").unwrap_or(&empty);
    let hf_props = |style: &[u8]| layout.child(Ns::Style, style).and_then(|s| s.child(Ns::Style, b"header-footer-properties"));

    match props.attr(Ns::Style, b"print-orientation") {
        Some("landscape") => p.orientation = Orientation::Landscape,
        Some("portrait") => p.orientation = Orientation::Portrait,
        _ => {}
    }
    for (slot, name) in [(0, b"margin-left".as_slice()), (1, b"margin-right")] {
        if let Some(v) = margin(props, name)
            && let Some(m) = p.margins.get_mut(slot)
        {
            *m = v as f32;
        }
    }
    let shown = |el: Option<&El>| el.is_some_and(|e| e.attr(Ns::Style, b"display") != Some("false"));
    for (edge, content, edge_slot, style, spacing, el) in [
        (b"margin-top".as_slice(), 2usize, 4usize, b"header-style".as_slice(), b"margin-bottom".as_slice(), header),
        (b"margin-bottom", 3, 5, b"footer-style", b"margin-top", footer),
    ] {
        let Some(edge) = margin(props, edge) else { continue };
        let extra = match (shown(el), hf_props(style)) {
            (true, Some(hp)) => {
                let height = hp.attr(Ns::Fo, b"min-height").or_else(|| hp.attr(Ns::Svg, b"height")).and_then(length).unwrap_or(0.0);
                height + hp.attr(Ns::Fo, spacing).and_then(length).unwrap_or(0.0)
            }
            _ => 0.0,
        };
        if let Some(m) = p.margins.get_mut(content) {
            *m = (edge + extra) as f32;
        }
        if let Some(m) = p.margins.get_mut(edge_slot) {
            *m = edge as f32;
        }
    }

    if let Some(scale) = props.attr(Ns::Style, b"scale-to") {
        if let Ok(n) = scale.trim().trim_end_matches('%').trim().parse::<f64>()
            && n.is_finite()
        {
            p.scale = n.round().clamp(10.0, 400.0) as u16;
            p.fit_to = None;
        }
    } else {
        let x = props.attr(Ns::Style, b"scale-to-X").and_then(|v| v.trim().parse::<u16>().ok());
        let y = props.attr(Ns::Style, b"scale-to-Y").and_then(|v| v.trim().parse::<u16>().ok());
        if x.is_some() || y.is_some() {
            p.fit_to = Some((x.unwrap_or(0), y.unwrap_or(0)));
        } else if props.attr(Ns::Style, b"scale-to-pages").map(str::trim) == Some("1") {
            p.fit_to = Some((1, 1));
        }
    }
    match props.attr(Ns::Style, b"table-centering") {
        Some("horizontal") => (p.center_h, p.center_v) = (true, false),
        Some("vertical") => (p.center_h, p.center_v) = (false, true),
        Some("both") => (p.center_h, p.center_v) = (true, true),
        Some("none") => (p.center_h, p.center_v) = (false, false),
        _ => {}
    }
    match props.attr(Ns::Style, b"print-page-order") {
        Some("ttb") => p.page_order = PageOrder::DownThenOver,
        Some("ltr") => p.page_order = PageOrder::OverThenDown,
        _ => {}
    }
    if let Some(print) = props.attr(Ns::Style, b"print") {
        p.gridlines = print.split_whitespace().any(|t| t == "grid");
        p.headings = print.split_whitespace().any(|t| t == "headers");
    }
    if let Some(n) = props.attr(Ns::Style, b"first-page-number").and_then(|v| v.trim().parse::<u32>().ok()) {
        p.output.first_page_number = Some(n);
        p.output.use_first_page_number = Some(true);
    }
    if let (Some(w), Some(h)) = (props.attr(Ns::Fo, b"page-width").and_then(length), props.attr(Ns::Fo, b"page-height").and_then(length))
        && let Some(name) = paper(w, h)
    {
        p.paper = name.to_string();
    }
}

/// Paper sizes in inches (portrait) that a page size maps to.
const PAPERS: [(&str, f64, f64); 5] = [
    ("Letter", 8.5, 11.0),
    ("Legal", 8.5, 14.0),
    ("A3", 297.0 / 25.4, 420.0 / 25.4),
    ("A4", 210.0 / 25.4, 297.0 / 25.4),
    ("A5", 148.0 / 25.4, 210.0 / 25.4),
];

fn paper(w: f64, h: f64) -> Option<&'static str> {
    let tol = 1.0 / 25.4;
    let near = |a: f64, b: f64| (a - b).abs() <= tol;
    PAPERS.iter().find(|(_, pw, ph)| (near(w, *pw) && near(h, *ph)) || (near(w, *ph) && near(h, *pw))).map(|(n, _, _)| *n)
}

/// A page margin: absent is 0, unparsable is `None` (the default stays).
fn margin(props: &El, name: &[u8]) -> Option<f64> {
    match props.attr(Ns::Fo, name) {
        None => Some(0.0),
        Some(v) => length(v),
    }
}

/// An ODF length in inches (`in`, `cm`, `mm`, `pt`, `pc`, `px`); `None` when unparsable,
/// negative or implausibly large.
fn length(v: &str) -> Option<f64> {
    let v = v.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(v.len());
    let (number, unit) = v.split_at(split);
    let n = number.trim().parse::<f64>().ok()?;
    let per_inch = match unit.trim() {
        "in" | "inch" => 1.0,
        "cm" => 2.54,
        "mm" => 25.4,
        "pt" => 72.0,
        "pc" => 6.0,
        "px" => 96.0,
        _ => return None,
    };
    let inches = n / per_inch;
    (inches.is_finite() && (0.0..=1000.0).contains(&inches)).then_some(inches)
}

/// A `style:header`/`style:footer` as an Excel header/footer string (`&L…&C…&R…`).
fn header_footer(el: &El) -> String {
    if el.attr(Ns::Style, b"display") == Some("false") {
        return String::new();
    }
    let region = |name: &[u8]| el.child(Ns::Style, name).map(section).unwrap_or_default();
    let has_regions = el.elements().any(|e| e.ns == Ns::Style && e.name.starts_with(b"region-"));
    let (left, center, right) = if has_regions {
        (region(b"region-left"), region(b"region-center"), region(b"region-right"))
    } else {
        (String::new(), section(el), String::new())
    };
    let mut out = String::new();
    for (code, text) in [("&L", left), ("&C", center), ("&R", right)] {
        if !text.is_empty() {
            out.push_str(code);
            out.push_str(&text);
        }
    }
    out
}

/// The paragraphs of one header/footer section, separated by line breaks.
fn section(el: &El) -> String {
    let mut out = Section { text: String::new(), after_space: true };
    let mut first = true;
    for p in el.elements().filter(|e| e.ns == Ns::Text && matches!(e.name.as_slice(), b"p" | b"h")) {
        if !first {
            out.push("\n");
        }
        first = false;
        out.after_space = true;
        out.walk(p, 0);
    }
    out.text
}

struct Section {
    text: String,
    /// The last thing written was whitespace (XML whitespace collapses into one space).
    after_space: bool,
}

impl Section {
    fn push(&mut self, s: &str) {
        if self.text.len().saturating_add(s.len()) <= MAX_SECTION {
            self.text.push_str(s);
        }
    }

    fn walk(&mut self, el: &El, depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        for child in &el.children {
            match child {
                Node::Text(t) => {
                    for ch in t.chars() {
                        if ch.is_whitespace() {
                            if !self.after_space {
                                self.push(" ");
                                self.after_space = true;
                            }
                        } else {
                            let mut buf = [0; 4];
                            self.push(if ch == '&' { "&&" } else { ch.encode_utf8(&mut buf) });
                            self.after_space = false;
                        }
                    }
                }
                Node::El(e) => {
                    let code = if e.ns == Ns::Text {
                        match e.name.as_slice() {
                            b"sheet-name" => Some("&A"),
                            b"page-number" => Some("&P"),
                            b"page-count" => Some("&N"),
                            b"date" => Some("&D"),
                            b"time" => Some("&T"),
                            b"file-name" => Some("&F"),
                            b"tab" => Some(" "),
                            b"line-break" => Some("\n"),
                            b"s" => {
                                let n = e.attr(Ns::Text, b"c").and_then(|c| c.trim().parse::<usize>().ok()).unwrap_or(1).min(MAX_SPACES);
                                self.push(&" ".repeat(n));
                                self.after_space = false;
                                continue;
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    match code {
                        Some(code) => {
                            self.push(code);
                            self.after_space = false;
                        }
                        None => self.walk(e, depth + 1),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_ods;
    use crate::tests::make_zip;

    const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/></manifest:manifest>"#;
    const CONTENT: &[u8] = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:spreadsheet><table:table table:name="A"><table:table-row><table:table-cell office:value-type="float" office:value="2"/></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#;

    #[test]
    fn malformed_styles_open_with_defaults_and_one_warning() {
        let zip = make_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.spreadsheet"),
            ("META-INF/manifest.xml", MANIFEST),
            ("content.xml", CONTENT),
            ("styles.xml", b"<office:document-styles><unclosed>"),
        ]);
        let (wb, report) = read_ods(&zip).unwrap();
        assert_eq!(wb.sheets[0].print, PrintSettings::default());
        assert_eq!(report.warnings.iter().filter(|w| w.as_str() == WARNING).count(), 1);
    }

    #[test]
    fn hostile_lengths_and_spaces_are_bounded() {
        assert_eq!(length("1e308in"), None);
        assert_eq!(length("-1in"), None);
        assert_eq!(length("NaNin"), None);
        assert_eq!(length("12furlongs"), None);
        assert_eq!(length("96px"), Some(1.0));
        assert_eq!(length_px("2.868cm"), Some(108.0));
        assert_eq!(length_px("0.4px"), None);
        assert_eq!(length_px("-3mm"), None);
        let s = r#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:master-styles><style:master-page style:name="Default"><style:header><text:p><text:s text:c="4000000000"/></text:p></style:header></style:master-page></office:master-styles></office:document-styles>"#;
        let root = parse_tree(s.as_bytes()).unwrap();
        let header = root.elements().next().and_then(|m| m.elements().next()).and_then(|m| m.child(Ns::Style, b"header")).unwrap();
        assert_eq!(header_footer(header).len(), 2 + MAX_SPACES);
    }
}
