use super::read::{first_free_block, merge_vml};
use super::write::{NameMap, attach, attach_kept, part_rels_xml, remap, write_parts};
use super::*;

/// A sheet's kept content being written.
pub struct SheetKept<'a> {
    p: Option<&'a Preserved>,
    names: NameMap,
    ids: HashMap<String, String>,
}

impl<'a> SheetKept<'a> {
    /// Writes the sheet's kept parts and adds their relationships to the sheet's (`part` is the
    /// sheet's part name, whose folder the relationship targets are relative to).
    pub fn write(sheet: &'a Sheet, part: &str, rels: &mut Rels, out: &mut Out) -> SheetKept<'a> {
        let Some(p) = sheet.preserved.as_deref() else { return SheetKept { p: None, names: NameMap::new(), ids: HashMap::new() } };
        // A kept PivotTable whose cache wasn't kept with the workbook is left out.
        let (wb_names, cache_ids) = (&out.kept.wb_names, &out.kept.cache_ids);
        let orphan: HashSet<String> = p
            .parts
            .iter()
            .filter(|q| {
                let direct = q.rels.iter().filter(|r| !r.external && is_kind(r, &["pivotCacheDefinition"]));
                // A table with no relationship to its cache names it by `cacheId` instead.
                let by_id = || {
                    q.content_type == CT_PIVOT_TABLE
                        && crate::xml::parse(&q.data).ok().and_then(|x| x.attr_u32("cacheId")).is_some_and(|id| !cache_ids.contains(&id))
                };
                let direct_is_orphan = direct.into_iter().any(|r| !wb_names.contains_key(&normalize(&r.target)));
                direct_is_orphan || by_id()
            })
            .map(|q| normalize(&q.name))
            .collect();
        let (parts, sheet_rels): (Cow<'_, [PreservedPart]>, Cow<'_, [PreservedRel]>) = if orphan.is_empty() {
            (Cow::Borrowed(&p.parts), Cow::Borrowed(&p.rels))
        } else {
            (
                p.parts.iter().filter(|q| !orphan.contains(&normalize(&q.name))).cloned().collect(),
                p.rels.iter().filter(|r| r.external || !orphan.contains(&normalize(&r.target))).cloned().collect(),
            )
        };
        let names = write_parts(out, &parts);
        let (from, _) = package::split_dir(part);
        let ids = attach(rels, &sheet_rels, &names, from);
        SheetKept { p: Some(p), names, ids }
    }

    fn vml(&self) -> Option<&'a PreservedPart> {
        self.p?.vml.as_ref()
    }

    fn write_vml_rels(&self, vml: &PreservedPart, name: &str, out: &mut Out) {
        if !vml.rels.is_empty() {
            out.part(&rels_part_name(name), None, part_rels_xml(&vml.rels, &self.names, name).into_bytes());
        }
    }

    /// The relationship ids of the kept VML drawing.
    pub fn vml_rel_ids(&self) -> Vec<&str> {
        self.vml().map(|v| v.rels.iter().map(|r| r.id.as_str()).collect()).unwrap_or_default()
    }

    /// The number GridCraft's own VML shapes start after when they join the kept drawing
    /// ([`SheetKept::notes_vml`] numbers them `base + 1`, `base + 2`…); `None` without one.
    pub fn shape_base(&self) -> Option<u64> {
        let vml = self.vml()?;
        Some(first_free_block(&String::from_utf8_lossy(&vml.data)).saturating_mul(1024))
    }

    /// GridCraft's notes and form controls VML for part `name`, merged with the kept control and
    /// object shapes; `pictures` are the notes' fill pictures as (relationship id, `xl/media` file
    /// name), written to the part's relationships with the kept ones.
    pub fn notes_vml(&self, ours: String, name: &str, pictures: &[(String, String)], out: &mut Out) -> String {
        let vml = self.vml();
        let mut rels = vml.filter(|v| !v.rels.is_empty()).map(|v| part_rels_xml(&v.rels, &self.names, name)).unwrap_or_default();
        if !pictures.is_empty() {
            let mut add = String::new();
            for (id, file) in pictures {
                let _ = write!(add, "<Relationship Id=\"{}\" Type=\"{NS_REL}/image\" Target=\"../media/{}\"/>", esc_attr(id), esc_attr(file));
            }
            match rels.rfind("</Relationships>") {
                Some(i) => rels.insert_str(i, &add),
                None => {
                    rels = format!(
                        "{XML_DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{add}</Relationships>"
                    )
                }
            }
        }
        if !rels.is_empty() {
            out.part(&rels_part_name(name), None, rels.into_bytes());
        }
        match vml {
            Some(vml) => merge_vml(&String::from_utf8_lossy(&vml.data), &ours),
            None => ours,
        }
    }

    /// The kept VML drawing of a sheet without notes; its new relationship id.
    pub fn lone_vml(&self, rels: &mut Rels, out: &mut Out) -> Option<String> {
        let vml = self.vml()?;
        let name = out.kept.claim(&vml.name);
        out.part(&name, Some(CT_VML), vml.data.to_vec());
        self.write_vml_rels(vml, &name, out);
        Some(rels.add("vmlDrawing", &format!("/{name}")))
    }

    /// The kept anchors of the sheet's drawing part as (position, XML), their relationships added
    /// to the drawing's `drels` under their source ids (kept diagram parts name them).
    pub fn drawing_anchors(&self, drels: &mut Rels) -> Vec<(usize, String)> {
        let Some(p) = self.p else { return vec![] };
        if p.anchors.is_empty() {
            return vec![];
        }
        let ids = attach_kept(drels, &p.drawing_rels, &self.names, "xl/drawings");
        p.anchors.iter().filter_map(|a| Some((a.index, remap(&a.xml, &ids)?))).collect()
    }

    /// The new relationship id for `<pageSetup r:id>` (printer settings).
    pub fn page_setup_id(&self) -> Option<&str> {
        let old = &self.p?.elements.iter().find(|e| e.slot == "pageSetup")?.xml;
        self.ids.get(old).map(String::as_str)
    }

    /// The kept sheet elements that come before `<drawing>`, in schema order.
    pub fn early_elements(&self) -> String {
        self.slots(SHEET_SLOTS_EARLY).replace(&format!(" xmlns:r=\"{NS_REL}\""), "")
    }

    /// The kept `<dataConsolidate>` (after `<sortState>`, before `<customSheetViews>`), its
    /// relationship ids rewritten; empty without one.
    pub fn data_consolidate(&self) -> String {
        self.slots(&[SHEET_SLOT_CONSOLIDATE]).replace(&format!(" xmlns:r=\"{NS_REL}\""), "")
    }

    /// The kept `<customSheetViews>` (after `<autoFilter>`, before `<mergeCells>`) with only the
    /// views whose guid is in `guids` (lower case); empty when none remains.
    pub fn custom_sheet_views(&self, guids: &HashSet<String>) -> String {
        if guids.is_empty() {
            return String::new();
        }
        let x = self.slots(SHEET_SLOTS_MID);
        if x.is_empty() {
            return String::new();
        }
        let keep = |g: &str| guids.contains(&g.trim().to_ascii_lowercase());
        filter_custom_sheet_views(&x, &keep).map(|x| x.replace(&format!(" xmlns:r=\"{NS_REL}\""), "")).unwrap_or_default()
    }

    /// The kept sheet elements, in schema order, with GridCraft's own form control entries
    /// (`controls`, see `controls::write`) in the sheet's one `<controls>` element.
    pub fn elements(&self, controls: &str) -> String {
        let mut s = String::new();
        let mut placed = controls.is_empty();
        if let Some(p) = self.p {
            for slot in SHEET_SLOTS {
                for e in p.elements.iter().filter(|e| e.slot == *slot) {
                    if let Some(mut x) = remap(&e.xml, &self.ids) {
                        if *slot == "controls"
                            && !placed
                            && let Some(i) = controls_insert_at(&x)
                        {
                            x.insert_str(i, controls);
                            placed = true;
                        }
                        s.push_str(&x);
                    }
                }
            }
        }
        // `<controls>` comes last of the kept elements.
        if !placed {
            s.push_str(&crate::controls::controls_element(controls));
        }
        s
    }

    fn slots(&self, slots: &[&str]) -> String {
        let Some(p) = self.p else { return String::new() };
        let mut s = String::new();
        for slot in slots {
            for e in p.elements.iter().filter(|e| e.slot == *slot) {
                if let Some(x) = remap(&e.xml, &self.ids) {
                    s.push_str(&x);
                }
            }
        }
        s
    }
}

/// `<customSheetViews>` with only the `<customSheetView>`s whose guid passes `keep`. `None` when
/// none passes or the XML is damaged.
fn filter_custom_sheet_views(x: &str, keep: &dyn Fn(&str) -> bool) -> Option<String> {
    let guid = |e: &BytesStart<'_>| {
        e.attributes()
            .with_checks(false)
            .flatten()
            .find(|a| a.key.as_ref() == b"guid")
            .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
            .unwrap_or_default()
    };
    let mut reader = Reader::from_str(x);
    let mut out = String::with_capacity(x.len());
    let mut last = 0usize;
    let mut depth = 0usize;
    let mut any = false;
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        let (view, end) = match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(e) if depth == 1 => {
                let g = guid(&e);
                reader.read_to_end(e.name()).ok()?;
                (Some(g), usize::try_from(reader.buffer_position()).ok()?)
            }
            Event::Start(_) => {
                depth += 1;
                (None, 0)
            }
            Event::Empty(e) if depth == 1 => (Some(guid(&e)), usize::try_from(reader.buffer_position()).ok()?),
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                (None, 0)
            }
            _ => (None, 0),
        };
        match view {
            Some(g) if keep(&g) => any = true,
            Some(_) => {
                out.push_str(x.get(last..start)?);
                last = end;
            }
            None => {}
        }
    }
    out.push_str(x.get(last..)?);
    any.then_some(out)
}

/// Where GridCraft's control entries go in a kept `<controls>` fragment: just before the close
/// tag of the `<controls>` element the reader reads (`controls::controls_of`), whatever its
/// namespace prefix. In an `mc:AlternateContent` with separate `<controls>` elements in its
/// Choice and Fallback branches that is the Choice's, the one an application that knows the
/// x14 extensions reads; the Fallback's is left as the file had it. With no Choice, or a Choice
/// without a `<controls>` element, it is the first `<controls>` element (a plain fragment, or a
/// Fallback alone).
fn controls_insert_at(x: &str) -> Option<usize> {
    let from = open_tag(x, "Choice").unwrap_or(0);
    close_tag_from(x, from, "controls")
}

/// The local name of the tag whose text starts at `name` (`x:controls` → `controls`).
fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The byte offset of the first open tag of `local` in `x`, with any namespace prefix.
fn open_tag(x: &str, local: &str) -> Option<usize> {
    x.match_indices('<').find_map(|(i, _)| {
        let rest = x.get(i + 1..)?;
        let end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?;
        (local_name(rest.get(..end)?) == local).then_some(i)
    })
}

/// The byte offset of the first close tag of `local` at or after `from` in `x`, with any
/// namespace prefix (`</controls>`, `</x:controls>`).
fn close_tag_from(x: &str, from: usize, local: &str) -> Option<usize> {
    x.get(from..)?.match_indices("</").find_map(|(i, _)| {
        let rest = x.get(from + i + 2..)?;
        let name = rest.get(..rest.find('>')?)?.trim_end();
        (local_name(name) == local).then_some(from + i)
    })
}

// ---------------------------------------------------------------- linked workbooks

/// `wb` as it is saved into the folder `to` (absolute): the relative paths of its linked
/// workbooks, relative to the folder it was read from, are rewritten relative to `to`, as Excel
/// does on Save As, or made absolute when no relative path exists (another drive). `None` when
/// nothing changes.
pub fn rebase_links(wb: &Workbook, to: &Path) -> Option<Workbook> {
    let p = wb.preserved.as_deref()?;
    let from = Folder::of(wb.source_dir.as_deref()?)?;
    let to = Folder::of(to)?;
    if from == to {
        return None;
    }
    let mut parts = p.parts.clone();
    let mut changed = false;
    for r in parts.iter_mut().flat_map(|part| part.rels.iter_mut()) {
        if r.external
            && is_kind(r, &["externalLinkPath"])
            && let Some(target) = rebase(&r.target, &from, &to)
        {
            r.target = target;
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    let mut out = wb.clone();
    out.preserved = Some(Arc::new(Preserved { parts, ..p.clone() }));
    Some(out)
}

/// An absolute folder, lexically normalized.
#[derive(PartialEq)]
struct Folder {
    /// Drive or share and root separator (`/`, `C:\`, `\\server\share\`).
    root: String,
    names: Vec<String>,
}

impl Folder {
    fn of(dir: &Path) -> Option<Folder> {
        if !dir.is_absolute() {
            return None;
        }
        let mut f = Folder { root: String::new(), names: vec![] };
        for c in dir.components() {
            match c {
                Component::Prefix(_) | Component::RootDir => f.root.push_str(&c.as_os_str().to_string_lossy()),
                Component::CurDir => {}
                Component::ParentDir => {
                    f.names.pop();
                }
                Component::Normal(n) => f.names.push(n.to_string_lossy().into_owned()),
            }
        }
        Some(f)
    }
}

/// A relative link target read from `from`, rewritten for `to`. `None` keeps it as it is: an
/// absolute path or URL, or a path that climbs above the root.
fn rebase(target: &str, from: &Folder, to: &Folder) -> Option<String> {
    if !is_relative_path(target) {
        return None;
    }
    // Each name as the file system knows it, and as it is written in a target. Names taken from
    // the original target keep their spelling.
    let mut path: Vec<(String, String)> = from.names.iter().map(|n| (n.clone(), encode_name(n))).collect();
    for seg in target.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                path.pop()?;
            }
            _ => path.push((decode_name(seg), seg.to_string())),
        }
    }
    let folders = path.len().checked_sub(1)?;
    if !from.root.eq_ignore_ascii_case(&to.root) {
        let sep = if from.root.contains('\\') { "\\" } else { "/" };
        let names: Vec<&str> = path.iter().map(|(name, _)| name.as_str()).collect();
        return Some(format!("file:///{}{}", from.root, names.join(sep)));
    }
    let common = to.names.iter().zip(&path).take(folders).take_while(|(a, (b, _))| *a == b).count();
    let up = "../".repeat(to.names.len().saturating_sub(common));
    let down: Vec<&str> = path.iter().skip(common).map(|(_, written)| written.as_str()).collect();
    Some(format!("{up}{}", down.join("/")))
}

/// Not empty, no root, no scheme or drive (`file:`, `http:`, `C:`).
fn is_relative_path(target: &str) -> bool {
    let scheme = target.split_once(':').is_some_and(|(s, _)| {
        s.starts_with(|c: char| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    !target.is_empty() && !target.starts_with(['/', '\\']) && !scheme
}

/// A folder name as a segment of a relationship target (a URI): ASCII characters a URI path
/// can't hold are percent-encoded, other characters are written as they are.
fn encode_name(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for c in name.chars() {
        if !c.is_ascii() || c.is_ascii_alphanumeric() || "-._~!$&'()+,;=@".contains(c) {
            s.push(c);
        } else {
            let _ = write!(s, "%{:02X}", c as u32);
        }
    }
    s
}

/// A target segment with its percent-escapes decoded; as written when they don't decode to UTF-8.
fn decode_name(seg: &str) -> String {
    let b = seg.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) if c == b'%' => {
                out.push(byte);
                i += 3;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| seg.to_string())
}
