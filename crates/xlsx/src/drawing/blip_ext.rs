//! Extensions of a picture's `a:blip` (`a:extLst`, ECMA-376 Part 1 §20.1.2.2.15) that GridCraft
//! does not model: artistic effects and background removal (`a14:imgProps`, whose `a14:imgLayer`
//! names an HD Photo part by relationship id) and the local-dpi flag (`a14:useLocalDpi`). Each
//! `a:ext` is kept verbatim with the parts its relationship ids name and written back on save with
//! the ids rewritten.

use std::collections::HashMap;
use std::sync::Arc;

use gridcraft_model::{KeptExtension, KeptExtensionPart};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::IoError;
use crate::read::Ctx;
use crate::write::{Out, Rels};
use crate::xml::{self, El};

/// Largest kept part of one extension.
const MAX_PART: u64 = 64 * 1024 * 1024;
/// Largest kept extension XML.
const MAX_XML: usize = 1024 * 1024;

/// Open elements while scanning: local name and the namespace declarations each makes.
type Scope = Vec<(Vec<u8>, Vec<(String, String)>)>;

/// The `a:ext` children of each `a:blip`'s `a:extLst` of a drawing part, one list per `a:blip` in
/// document order, each with the namespace declarations in scope added. `None` when the XML is
/// damaged.
pub fn scan(bytes: &[u8]) -> Option<Vec<Vec<String>>> {
    let data = xml::to_utf8(bytes);
    let text = std::str::from_utf8(&data).ok()?;
    let mut reader = Reader::from_str(text);
    let mut stack: Scope = vec![];
    let mut out: Vec<Vec<String>> = vec![];
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(e) => {
                let local = e.local_name().as_ref().to_vec();
                let in_ext_list = stack.len() >= 2
                    && stack.last().is_some_and(|(n, _)| n == b"extLst")
                    && stack.get(stack.len() - 2).is_some_and(|(n, _)| n == b"blip");
                if in_ext_list && local == b"ext" {
                    reader.read_to_end(e.name()).ok()?;
                    let end = usize::try_from(reader.buffer_position()).ok()?;
                    push_ext(&mut out, text.get(start..end)?, &stack);
                    continue;
                }
                if local == b"blip" {
                    out.push(vec![]);
                }
                if stack.len() >= xml::MAX_DEPTH {
                    return None;
                }
                stack.push((local, decls(&e)));
            }
            Event::Empty(e) => {
                let local = e.local_name().as_ref().to_vec();
                if local == b"blip" {
                    out.push(vec![]);
                } else if local == b"ext"
                    && stack.len() >= 2
                    && stack.last().is_some_and(|(n, _)| n == b"extLst")
                    && stack.get(stack.len() - 2).is_some_and(|(n, _)| n == b"blip")
                {
                    let end = usize::try_from(reader.buffer_position()).ok()?;
                    push_ext(&mut out, text.get(start..end)?, &stack);
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            _ => {}
        }
    }
    Some(out)
}

fn decls(e: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    let mut out = vec![];
    for a in e.attributes().with_checks(false).flatten() {
        let key = a.key.as_ref();
        let prefix = if key == b"xmlns" { Some(&b""[..]) } else { key.strip_prefix(b"xmlns:") };
        if let Some(p) = prefix
            && let (Ok(p), Ok(v)) = (std::str::from_utf8(p), std::str::from_utf8(&a.value))
            && !v.contains('"')
        {
            out.push((p.to_string(), v.to_string()));
        }
    }
    out
}

fn push_ext(out: &mut [Vec<String>], frag: &str, stack: &Scope) {
    if frag.len() > MAX_XML {
        return;
    }
    // In-scope declarations, the innermost one of each prefix.
    let mut scope: Vec<(String, String)> = vec![];
    for (_, d) in stack.iter().rev() {
        for (p, v) in d {
            if !scope.iter().any(|(q, _)| q == p) {
                scope.push((p.clone(), v.clone()));
            }
        }
    }
    if let Some(list) = out.last_mut() {
        list.push(crate::passthrough::with_decls(frag, &scope));
    }
}

/// The `a:blip` elements of a parsed drawing part, in document order.
pub fn blips(root: &El) -> Vec<&El> {
    fn walk<'a>(e: &'a El, out: &mut Vec<&'a El>) {
        if e.name == "blip" {
            out.push(e);
        }
        for c in &e.children {
            walk(c, out);
        }
    }
    let mut out = vec![];
    walk(root, &mut out);
    out
}

/// Whether `x` is one well-formed element and nothing else.
fn single_element(x: &str) -> bool {
    let mut reader = Reader::from_str(x);
    let mut depth = 0usize;
    let mut roots = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => return depth == 0 && roots == 1,
            Ok(Event::Start(_)) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                if depth > xml::MAX_DEPTH {
                    return false;
                }
            }
            Ok(Event::Empty(_)) => {
                if depth == 0 {
                    roots += 1;
                }
            }
            Ok(Event::End(_)) => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            Ok(Event::Text(t)) if depth == 0 && !t.iter().all(u8::is_ascii_whitespace) => return false,
            Ok(Event::CData(_)) if depth == 0 => return false,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

/// The relationship ids an extension's XML uses, once each. `None` when it is damaged.
fn rel_ids(x: &str) -> Option<Vec<String>> {
    let mut ids: Vec<String> = vec![];
    crate::passthrough::map_rel_ids(x, &mut |id| {
        if !ids.iter().any(|i| i == id) {
            ids.push(id.to_string());
        }
        Some(id.to_string())
    })?;
    Some(ids)
}

/// A drawing part's relationships (full type URIs) and its package's content types, read once
/// when a picture extension first needs them.
#[derive(Default)]
pub struct Lookup {
    loaded: bool,
    rels: Vec<gridcraft_model::PreservedRel>,
    defaults: HashMap<String, String>,
    overrides: HashMap<String, String>,
}

impl Lookup {
    fn load(&mut self, cx: &mut Ctx<'_>, part: &str) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.rels = crate::passthrough::raw_rels(cx, part).unwrap_or_default();
        if let Some(types) = cx.pkg.read_limited("[Content_Types].xml", 16 * 1024 * 1024).ok().flatten().and_then(|b| xml::parse(&b).ok()) {
            for d in types.kids("Default") {
                if let (Some(e), Some(t)) = (d.attr("Extension"), d.attr("ContentType")) {
                    self.defaults.insert(e.to_ascii_lowercase(), t.to_string());
                }
            }
            for o in types.kids("Override") {
                if let (Some(p), Some(t)) = (o.attr("PartName"), o.attr("ContentType")) {
                    self.overrides.insert(crate::package::normalize(p), t.to_string());
                }
            }
        }
    }

    fn content_type(&self, part: &str) -> String {
        let key = crate::package::normalize(part);
        if let Some(t) = self.overrides.get(&key) {
            return t.clone();
        }
        let ext = key.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        self.defaults.get(ext).cloned().unwrap_or_else(|| "application/octet-stream".to_string())
    }
}

/// The picture extensions worth keeping, with the parts they name read from the package. An
/// extension whose part is missing, external or damaged is dropped with a warning; the picture
/// stays.
pub fn read(cx: &mut Ctx<'_>, part: &str, lookup: &mut Lookup, exts: &[String]) -> Result<Vec<KeptExtension>, IoError> {
    let mut kept = vec![];
    for x in exts {
        let Some(ids) = rel_ids(x).filter(|_| single_element(x)) else {
            cx.warn(format!("a picture extension with damaged XML was dropped ({part})"));
            continue;
        };
        if !ids.is_empty() {
            lookup.load(cx, part);
        }
        let mut parts = vec![];
        let mut why = None;
        for id in &ids {
            let Some(rel) = lookup.rels.iter().find(|r| r.id == *id) else {
                why = Some(format!("relationship {id} is missing"));
                break;
            };
            if rel.external {
                why = Some(format!("relationship {id} is external"));
                break;
            }
            let target = rel.target.clone();
            let has_rels = {
                let (dir, file) = crate::package::split_dir(&target);
                let name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
                cx.pkg.has(&name)
            };
            if has_rels {
                why = Some(format!("{target} has relationships of its own"));
                break;
            }
            let data = match cx.pkg.read_limited(&target, MAX_PART) {
                Ok(Some(d)) => d,
                Ok(None) => {
                    why = Some(format!("{target} is missing"));
                    break;
                }
                Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
                Err(e) => {
                    why = Some(format!("{target}: {e}"));
                    break;
                }
            };
            let ext = target.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
            parts.push(KeptExtensionPart {
                id: id.clone(),
                rel_type: rel.rel_type.clone(),
                content_type: lookup.content_type(&target),
                ext,
                data: Arc::new(data),
            });
        }
        match why {
            Some(why) => cx.warn(format!("a picture effect was dropped ({part}): {why}")),
            None => kept.push(KeptExtension { xml: x.clone(), parts }),
        }
    }
    Ok(kept)
}

/// A file extension safe in a part name.
fn safe_ext(ext: &str) -> &str {
    if !ext.is_empty() && ext.len() <= 8 && ext.bytes().all(|b| b.is_ascii_alphanumeric()) { ext } else { "bin" }
}

/// The picture's kept `a:extLst` (empty when it has none): each part written under `xl/media/`
/// with its content type, a new drawing relationship of the same type to it, and the extension's
/// relationship ids rewritten. An extension that can't be written faithfully is left out.
pub fn write(exts: &[KeptExtension], drels: &mut Rels, out: &mut Out) -> String {
    let mut s = String::new();
    for e in exts {
        if e.xml.len() > MAX_XML || !single_element(&e.xml) {
            continue;
        }
        let mut ids: HashMap<String, String> = HashMap::new();
        for p in &e.parts {
            if ids.contains_key(&p.id) {
                continue;
            }
            let stem = if p.rel_type.rsplit('/').next().is_some_and(|k| k.eq_ignore_ascii_case("hdphoto")) { "hdphoto" } else { "media" };
            let file = out.media_part(stem, safe_ext(&p.ext), &p.content_type, &p.data);
            ids.insert(p.id.clone(), drels.add_raw(&p.rel_type, &format!("../media/{file}"), false));
        }
        if let Some(x) = crate::passthrough::map_rel_ids(&e.xml, &mut |id| ids.get(id).cloned()) {
            s.push_str(&x);
        }
    }
    if s.is_empty() { s } else { format!("<a:extLst>{s}</a:extLst>") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_xml_must_be_one_element() {
        assert!(single_element(r#"<a:ext xmlns:a="urn:a" uri="y"><a:x/></a:ext>"#));
        assert!(single_element(r#"<a:ext uri="y"/>"#));
        assert!(!single_element(r#"<a:ext uri="x"/></a:blip><evil/>"#));
        assert!(!single_element(r#"<a:ext uri="x"/><a:ext uri="y"/>"#));
        assert!(!single_element(r#"<a:ext uri="x">"#));
        assert!(!single_element("text"));
    }
}
