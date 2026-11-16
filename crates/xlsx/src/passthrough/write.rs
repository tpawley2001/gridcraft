use super::read::{custom_workbook_views_xml, map_rel_ids, vml_block_max};
use super::*;

// ---------------------------------------------------------------- writing

/// Names given to kept parts while a package is written.
#[derive(Default)]
pub struct KeptOut {
    used: HashSet<String>,
    n: u32,
    /// The written names of the kept parts, by their original name (normalized). A part kept by
    /// several owners (an Office add-in's web extension part, reached from the drawing and from
    /// the task panes) is written once, and all of them point at it; two different parts that
    /// happen to have the same name each get their own.
    names: HashMap<String, Vec<(PreservedPart, String)>>,
    /// Normalized names of the kept parts (and their rels) already written.
    written: HashSet<String>,
    /// The highest VML shape block a kept header and footer drawing uses: notes go after it.
    vml_blocks: u32,
    /// Package-root relationships (type, target, external), added when `_rels/.rels` is written.
    root: Vec<(String, String, bool)>,
    /// Names of the workbook's kept parts, which sheets' kept parts can refer to (the cache of a
    /// kept PivotTable).
    pub(super) wb_names: NameMap,
    /// The `cacheId`s of the workbook's kept pivot caches: a kept PivotTable that names its cache
    /// only by id is written only when its cache is among them.
    pub(super) cache_ids: HashSet<u32>,
    patches: StylePatches,
}

impl KeptOut {
    /// Reserves a part name GridCraft writes itself, so kept parts don't take it. False when it is
    /// taken.
    pub fn reserve(&mut self, name: &str) -> bool {
        self.used.insert(name.to_ascii_lowercase())
    }

    /// Reserves the VML shape blocks the kept header and footer drawings of `wb` use, so that the
    /// notes drawings GridCraft writes don't reuse their shape ids.
    pub fn reserve_vml_blocks(&mut self, wb: &Workbook) {
        let vmls =
            wb.sheets.iter().filter_map(|s| s.preserved.as_deref()).flat_map(|p| &p.parts).filter(|q| q.content_type.eq_ignore_ascii_case(CT_VML));
        let max = vmls.map(|q| vml_block_max(&String::from_utf8_lossy(&q.data))).max().unwrap_or(0);
        self.vml_blocks = u32::try_from(max).unwrap_or(u32::MAX);
    }

    /// The shape block of the `n`th notes drawing.
    pub fn notes_block(&self, n: u32) -> u32 {
        n.saturating_add(self.vml_blocks)
    }

    /// A part name for a kept part: its original name when that can't collide with anything
    /// written, otherwise a fresh one in the same folder.
    pub(crate) fn claim(&mut self, original: &str) -> String {
        self.claim_with(original, false)
    }

    #[cfg(test)]
    pub(super) fn claim_third_party(&mut self, original: &str) -> String {
        self.claim_with(original, true)
    }

    /// [`Self::claim`]; `third_party`: the part is linked with a third-party relationship type and
    /// keeps its name anywhere GridCraft doesn't write (see [`free_for_third_party`]).
    fn claim_with(&mut self, original: &str, third_party: bool) -> String {
        let lower = original.to_ascii_lowercase();
        let own = OWN_DIRS.iter().any(|d| lower.starts_with(d)) || (third_party && free_for_third_party(original));
        if own && self.used.insert(lower) {
            return original.to_string();
        }
        let (dir, file) = package::split_dir(original);
        let (stem, ext) = match file.rsplit_once('.') {
            Some((s, e)) => (s, format!(".{e}")),
            None => (file, String::new()),
        };
        let stem = if own {
            format!("{stem}_")
        } else if lower.starts_with("xl/pivotcache/pivotcachedefinition") {
            // A distinct suffix leaves the native writer's numbered cache names available.
            "pivotCacheDefinition_kept".to_string()
        } else {
            "kept".to_string()
        };
        loop {
            self.n = self.n.saturating_add(1);
            let file = format!("{stem}{}{ext}", self.n);
            let cand = if dir.is_empty() { file } else { format!("{dir}/{file}") };
            if self.used.insert(cand.to_ascii_lowercase()) || self.n == u32::MAX {
                return cand;
            }
        }
    }
}

/// Rewrites the values of `dxfId` and `numFmtId` attributes through the maps (ids not in a map
/// are kept). `None` when the XML is damaged.
fn map_style_ids(data: &[u8], dxfs: &HashMap<u32, u32>, fmts: &HashMap<u32, u32>) -> Option<Vec<u8>> {
    let text = xml::to_utf8(data);
    let mut reader = Reader::from_reader(text.as_ref());
    let mut writer = Writer::new(Vec::with_capacity(data.len()));
    let mut buf = vec![];
    let map = |e: &BytesStart<'_>| -> Option<BytesStart<'static>> {
        let name = String::from_utf8(e.name().as_ref().to_vec()).ok()?;
        let mut out = BytesStart::new(name);
        for a in e.attributes().with_checks(false) {
            let a = a.ok()?;
            let k = a.key.local_name();
            let table = match k.as_ref() {
                b"dxfId" => Some(dxfs),
                b"numFmtId" => Some(fmts),
                _ => None,
            };
            let new = table.and_then(|t| std::str::from_utf8(&a.value).ok()?.trim().parse::<u32>().ok().and_then(|id| t.get(&id)));
            match new {
                Some(n) => out.push_attribute(Attribute { key: a.key, value: Cow::Owned(n.to_string().into_bytes()) }),
                None => out.push_attribute(Attribute { key: a.key, value: Cow::Owned(a.value.to_vec()) }),
            }
        }
        Some(out)
    };
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Eof => break,
            Event::Start(e) => writer.write_event(Event::Start(map(&e)?)).ok()?,
            Event::Empty(e) => writer.write_event(Event::Empty(map(&e)?)).ok()?,
            ev => writer.write_event(ev).ok()?,
        }
        buf.clear();
    }
    Some(writer.into_inner())
}

/// Kept parts whose style ids are renumbered once the styles part is written.
#[derive(Default)]
pub struct StylePatches {
    /// (written part name, source bytes, dxf id map, source number format id → code).
    pub(super) list: Vec<StylePatch>,
}

pub(super) type StylePatch = (String, Arc<[u8]>, HashMap<u32, u32>, Vec<(u32, String)>);

impl KeptOut {
    /// Number format codes kept parts refer to (to be registered in the styles part).
    pub fn num_fmt_codes(&self) -> Vec<String> {
        let mut v: Vec<String> = vec![];
        for (_, _, _, fmts) in &self.patches.list {
            for (_, code) in fmts {
                if !v.contains(code) {
                    v.push(code.clone());
                }
            }
        }
        v
    }
}

/// Renumbers the `dxfId` and `numFmtId` attributes of kept PivotTables and pivot caches to the
/// written styles part. `fmt_id` maps a number format code to its id there.
pub fn finish_style_ids(out: &mut Out, fmt_id: &dyn Fn(&str) -> Option<u32>) {
    for (name, data, dxfs, fmts) in std::mem::take(&mut out.kept.patches.list) {
        let fmts: HashMap<u32, u32> = fmts.iter().filter_map(|(old, code)| Some((*old, fmt_id(code)?))).collect();
        if let Some(x) = map_style_ids(&data, &dxfs, &fmts) {
            out.replace(&name, x);
        }
    }
}

/// Original part name (normalized) → written part name.
pub(super) type NameMap = HashMap<String, String>;

/// The target of a kept relationship as written from the part in `from` (its folder): the written
/// name of the part it points at, relative to that folder, as Excel writes it.
fn rel_target(r: &PreservedRel, names: &NameMap, from: &str) -> Option<String> {
    if r.external {
        return Some(r.target.clone());
    }
    names.get(&normalize(&r.target)).map(|n| relative_target(from, n))
}

/// `target` (a part name) relative to the folder `from`.
fn relative_target(from: &str, target: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = "../".repeat(from.len().saturating_sub(common));
    out.push_str(&to.get(common..).unwrap_or_default().join("/"));
    out
}

/// A kept part's relationships, ids unchanged, targets renamed and made relative to the part.
pub(super) fn part_rels_xml(rels: &[PreservedRel], names: &NameMap, part: &str) -> String {
    let (from, _) = package::split_dir(part);
    let mut s = format!("{XML_DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for r in rels {
        let Some(target) = rel_target(r, names, from) else { continue };
        let _ = write!(
            s,
            "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>",
            esc_attr(&r.id),
            esc_attr(&r.rel_type),
            esc_attr(&target),
            if r.external { " TargetMode=\"External\"" } else { "" }
        );
    }
    s.push_str("</Relationships>");
    s
}

pub(super) fn write_parts(out: &mut Out, parts: &[PreservedPart]) -> NameMap {
    write_parts_with_media(out, parts, false, &HashSet::new())
}

/// Writes kept parts; `third_party` (normalized names) are the parts reached through third-party
/// relationships, which keep their source names where they can.
fn write_parts_with_media(out: &mut Out, parts: &[PreservedPart], theme_media: bool, third_party: &HashSet<String>) -> NameMap {
    let mut names = NameMap::new();
    for p in parts {
        let key = normalize(&p.name);
        let known = out.kept.names.get(&key).and_then(|v| v.iter().find(|(part, _)| part == p)).map(|(_, name)| name.clone());
        let name = match known {
            Some(name) => name,
            None => {
                let name = if theme_media && p.content_type.starts_with("image/") {
                    let ext = p.name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("bin");
                    let file = out.media_part("themeImage", ext, &p.content_type, &p.data);
                    let name = format!("xl/media/{file}");
                    out.kept.written.insert(normalize(&name));
                    name
                } else {
                    out.kept.claim_with(&p.name, third_party.contains(&key))
                };
                out.kept.names.entry(key.clone()).or_default().push((p.clone(), name.clone()));
                name
            }
        };
        names.entry(key).or_insert(name);
    }
    let own: HashSet<String> = names.values().cloned().collect();
    for (k, v) in &out.kept.wb_names {
        names.entry(k.clone()).or_insert_with(|| v.clone());
    }
    let mut written = HashSet::new();
    for p in parts {
        let Some(new) = names.get(&normalize(&p.name)).cloned() else { continue };
        if !own.contains(&new) || !written.insert(new.clone()) {
            continue;
        }
        let key = normalize(&new);
        if out.kept.written.contains(&key) {
            // Media bytes may have been emitted by the deduplicating writer above.
            if theme_media && !p.rels.is_empty() {
                out.part(&rels_part_name(&new), None, part_rels_xml(&p.rels, &names, &new).into_bytes());
            }
            continue;
        }
        out.kept.written.insert(key);
        out.part(&new, Some(&p.content_type), p.data.to_vec());
        if !p.dxfs.is_empty() || !p.num_fmts.is_empty() {
            let dxfs = p.dxfs.iter().map(|(old, s)| (*old, crate::styles::dxf_id(&mut out.dxfs, s))).collect();
            out.kept.patches.list.push((new.clone(), p.data.clone(), dxfs, p.num_fmts.clone()));
        }
        if !p.rels.is_empty() {
            out.part(&rels_part_name(&new), None, part_rels_xml(&p.rels, &names, &new).into_bytes());
        }
    }
    names
}

/// Theme XML retains its source relationship ids; only package target names may change.
pub fn write_theme_relationships(theme: &gridcraft_model::Theme, out: &mut Out) {
    let Some(p) = &theme.preserved else { return };
    let names = write_parts_with_media(out, &p.parts, true, &HashSet::new());
    if !p.rels.is_empty() {
        out.part("xl/theme/_rels/theme1.xml.rels", None, part_rels_xml(&p.rels, &names, "xl/theme/theme1.xml").into_bytes());
    }
}

/// Adds the owner's relationships (the owner part is in `from`); returns old id → new id.
pub(super) fn attach(rels: &mut Rels, list: &[PreservedRel], names: &NameMap, from: &str) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    for r in list {
        if let Some(target) = rel_target(r, names, from) {
            let id = rels.add_raw(&r.rel_type, &target, r.external);
            ids.insert(r.id.clone(), id);
        }
    }
    ids
}

/// [`attach`] keeping the source ids (see [`Rels::add_kept`]).
pub(super) fn attach_kept(rels: &mut Rels, list: &[PreservedRel], names: &NameMap, from: &str) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    for r in list {
        if let Some(target) = rel_target(r, names, from) {
            let id = rels.add_kept(&r.id, &r.rel_type, &target, r.external);
            ids.insert(r.id.clone(), id);
        }
    }
    ids
}

pub(super) fn remap(x: &str, ids: &HashMap<String, String>) -> Option<String> {
    map_rel_ids(x, &mut |id| ids.get(id).cloned())
}

/// A kept chart, dialog or macro sheet being written: its `<sheet>` entry's fields and its new relationship id.
pub struct KeptSheet {
    before: usize,
    name: String,
    visibility: Visibility,
    rid: String,
    sheet_id: u32,
}

/// The `sheetId`s of the written sheets: each keeps the one the file gave it, unless another
/// sheet took it first (a copied sheet) or it has none (a new sheet), and those get the next id
/// that no sheet asks for.
pub struct SheetIds {
    wanted: HashSet<u32>,
    given: HashSet<u32>,
    next: u32,
    /// The ids given to the sheets that asked for them (they kept the id the file gave them).
    own: HashSet<u32>,
    /// The id of the first sheet written.
    first: Option<u32>,
}

impl SheetIds {
    pub fn new(wb: &Workbook, kept: &WorkbookKept) -> SheetIds {
        let wanted: HashSet<u32> = wb.sheets.iter().map(|s| s.file_id).chain(kept.sheets.iter().map(|s| s.sheet_id)).filter(|&id| id != 0).collect();
        SheetIds { wanted, given: HashSet::new(), next: 1, own: HashSet::new(), first: None }
    }

    pub fn take(&mut self, wanted: u32) -> u32 {
        if wanted != 0 && self.given.insert(wanted) {
            self.own.insert(wanted);
            self.first.get_or_insert(wanted);
            return wanted;
        }
        while self.wanted.contains(&self.next) || self.given.contains(&self.next) {
            self.next += 1;
        }
        self.given.insert(self.next);
        self.first.get_or_insert(self.next);
        self.next
    }
}

/// The workbook's kept content as written.
#[derive(Default)]
pub struct WorkbookKept {
    /// Workbook XML elements that go right after `<sheets>` (`<externalReferences>`).
    pub elements: String,
    /// `<customWorkbookViews>`, which goes after `<calcPr>` (see [`custom_workbook_views`]).
    pub custom_views: String,
    sheets: Vec<KeptSheet>,
    /// Kept pivot caches: (`cacheId`, new relationship id).
    pivot_caches: Vec<(u32, String)>,
}

impl WorkbookKept {
    /// The `cacheId`s of the kept pivot caches (written PivotTables must not reuse them).
    pub fn pivot_cache_ids(&self) -> Vec<u32> {
        self.pivot_caches.iter().map(|(id, _)| *id).collect()
    }

    /// The `<pivotCache>` entries of the kept pivot caches.
    pub fn pivot_cache_entries(&self) -> String {
        let mut x = String::new();
        for (id, rid) in &self.pivot_caches {
            let _ = write!(x, "<pivotCache cacheId=\"{id}\" r:id=\"{}\"/>", esc_attr(rid));
        }
        x
    }

    /// Where the model's sheet `i` is in the file's sheet list: kept sheets come between worksheets.
    pub fn file_index(&self, i: usize) -> usize {
        i + self.sheets.iter().filter(|s| s.before <= i).count()
    }

    /// The `<sheet>` entries of the kept sheets that go right before the model's sheet `i`
    /// (`i` = the number of sheets for those that come last), with their ids from `ids`.
    pub fn sheet_entries(&self, i: usize, last: bool, ids: &mut SheetIds) -> String {
        let mut x = String::new();
        for s in self.sheets.iter().filter(|s| if last { s.before >= i } else { s.before == i }) {
            let state = match s.visibility {
                Visibility::Visible => "",
                Visibility::Hidden => " state=\"hidden\"",
                Visibility::VeryHidden => " state=\"veryHidden\"",
            };
            let name: String = s.name.chars().take(31).collect();
            let id = ids.take(s.sheet_id);
            let _ = write!(x, "<sheet name=\"{}\" sheetId=\"{id}\"{state} r:id=\"{}\"/>", esc_attr(&name), esc_attr(&s.rid));
        }
        x
    }
}

/// The part names reachable from `root` through kept parts' relationships (normalized).
fn reachable(p: &Preserved, root: &PreservedRel) -> HashSet<String> {
    let by_name: HashMap<String, &PreservedPart> = p.parts.iter().map(|q| (normalize(&q.name), q)).collect();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([normalize(&root.target)]);
    while let Some(n) = queue.pop_front() {
        if !seen.insert(n.clone()) {
            continue;
        }
        if let Some(q) = by_name.get(&n) {
            queue.extend(q.rels.iter().filter(|r| !r.external).map(|r| normalize(&r.target)));
        }
    }
    seen
}

/// Every formula (`<c:f>`) of a chart part, depth first.
fn chart_formulas(el: &xml::El, out: &mut Vec<String>) {
    if el.name == "f" && !el.text.trim().is_empty() {
        out.push(el.text.trim().to_string());
    }
    for c in &el.children {
        chart_formulas(c, out);
    }
}

/// The sheet names a formula such as `'My Sheet'!$A$1:$A$9,Data!$B$1` refers to. References to
/// other workbooks (`[1]Sheet!A1`) are not sheets of this one and are left out.
pub(crate) fn sheet_names_in(formula: &str) -> Vec<String> {
    let mut names = vec![];
    let chars: Vec<char> = formula.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let mut i = 0;
    while let Some(c) = at(i) {
        let mut name = String::new();
        let start = i;
        if c == '\'' {
            i += 1;
            while let Some(c) = at(i) {
                if c == '\'' {
                    if at(i + 1) == Some('\'') {
                        name.push('\'');
                        i += 2;
                        continue;
                    }
                    break;
                }
                name.push(c);
                i += 1;
            }
            i += 1;
        } else {
            while let Some(c) = at(i).filter(|c| !matches!(c, '!' | ',' | '(' | ')' | '\'' | ' ')) {
                name.push(c);
                i += 1;
            }
        }
        if at(i) == Some('!') {
            if !name.is_empty() && !name.starts_with('[') {
                names.push(name);
            }
            i += 1;
        } else if i == start {
            i += 1;
        }
    }
    names
}

/// Whether a kept sheet can be written back: a chart's series must still refer to sheets that
/// exist under the same names (the chart is kept as it was, so a series on a deleted or renamed
/// sheet would be left pointing at nothing), no worksheet may have taken its name, and a macro
/// sheet goes only into a macro-enabled package (`macros`), like the VBA project.
///
/// `usable` caches, by the normalized target of the sheet's relationship, whether its graph
/// refers only to sheets that exist (`false` when the relationship is missing): entries sharing one
/// graph, under the same or different relationship ids, are analysed once, and only the verdict is
/// kept, not the references it came from.
fn kept_sheet_usable(wb: &Workbook, p: &Preserved, s: &PreservedSheet, macros: bool, usable: &mut HashMap<String, bool>) -> bool {
    let same = |a: &str, b: &str| a.chars().take(31).collect::<String>().eq_ignore_ascii_case(&b.chars().take(31).collect::<String>());
    if wb.sheets.iter().any(|w| same(&w.name, &s.name)) {
        return false;
    }
    let Some(root) = p.rels.iter().find(|r| r.id == s.rel_id) else { return false };
    if !macros && is_kind(root, MACRO_SHEET_KINDS) {
        return false;
    }
    *usable.entry(normalize(&root.target)).or_insert_with(|| {
        let parts = reachable(p, root);
        let mut formulas = vec![];
        for q in p.parts.iter().filter(|q| parts.contains(&normalize(&q.name)) && q.content_type.contains("chart")) {
            if let Ok(x) = xml::parse(&q.data) {
                chart_formulas(&x, &mut formulas);
            }
        }
        formulas.iter().flat_map(|f| sheet_names_in(f)).all(|n| wb.sheets.iter().any(|w| same(&w.name, &n)))
    })
}

/// Writes the workbook's kept parts and relationships. Kept sheets that can't be written back
/// (see [`kept_sheet_usable`]; `macros`: the package is macro-enabled) are left out with the parts
/// only they reach.
pub fn write_workbook(wb: &Workbook, wb_rels: &mut Rels, out: &mut Out, macros: bool) -> WorkbookKept {
    let Some(p) = wb.preserved.as_deref() else { return WorkbookKept::default() };
    let (live, dropped): (Vec<&PreservedSheet>, Vec<&PreservedSheet>) = {
        let mut refs = HashMap::new();
        p.sheets.iter().partition(|s| kept_sheet_usable(wb, p, s, macros, &mut refs))
    };
    let root_of = |s: &PreservedSheet| p.rels.iter().find(|r| r.id == s.rel_id);
    // The revision history names sheets by id: it goes when the sheets changed.
    let stale = super::revisions::stale_roots(wb, p, dropped.is_empty());
    // Pivot caches no kept PivotTable of a sheet uses any more (their sheets were deleted).
    let sheet_parts = || wb.sheets.iter().filter_map(|s| s.preserved.as_deref()).flat_map(|q| q.parts.iter());
    let mut used_caches: HashSet<String> = sheet_parts()
        .flat_map(|part| part.rels.iter())
        .filter(|r| !r.external && is_kind(r, &["pivotCacheDefinition"]))
        .map(|r| normalize(&r.target))
        .collect();
    // A PivotTable can name its cache only by the workbook's `cacheId`.
    let cache_of_id: HashMap<u32, String> = p
        .elements
        .iter()
        .filter(|e| matches!(e.slot.as_str(), "pivotCache" | "unusedPivotCache"))
        .filter_map(|e| {
            let x = xml::parse(e.xml.as_bytes()).ok()?;
            let rel = p.rels.iter().find(|r| Some(r.id.as_str()) == x.attr("id"))?;
            Some((x.attr_u32("cacheId")?, normalize(&rel.target)))
        })
        .collect();
    used_caches.extend(
        sheet_parts()
            .filter(|part| part.content_type == CT_PIVOT_TABLE)
            .filter_map(|part| xml::parse(&part.data).ok()?.attr_u32("cacheId"))
            .filter_map(|id| cache_of_id.get(&id).cloned()),
    );
    // Caches already unused on import belong to the workbook, so deleting sheets keeps them.
    used_caches.extend(p.elements.iter().filter(|e| e.slot == "unusedPivotCache").filter_map(|e| {
        let x = xml::parse(e.xml.as_bytes()).ok()?;
        cache_of_id.get(&x.attr_u32("cacheId")?).cloned()
    }));
    let unused_caches: Vec<&PreservedRel> =
        p.rels.iter().filter(|r| !r.external && is_kind(r, &["pivotCacheDefinition"]) && !used_caches.contains(&normalize(&r.target))).collect();
    let dropped_roots: Vec<&PreservedRel> = dropped.iter().filter_map(|s| root_of(s)).chain(stale).chain(unused_caches.iter().copied()).collect();
    let dropped_ids: HashSet<&str> = dropped_roots.iter().map(|r| r.id.as_str()).collect();
    // Parts reached only by dropped sheets, stale history and unused pivot caches go with them.
    let mut gone: HashSet<String> = dropped_roots.iter().flat_map(|r| reachable(p, r)).collect();
    if !gone.is_empty() {
        let others = p.rels.iter().filter(|r| !dropped_ids.contains(r.id.as_str())).flat_map(|r| reachable(p, r));
        for n in others {
            gone.remove(&n);
        }
    }
    let parts: Vec<PreservedPart> = p.parts.iter().filter(|q| !gone.contains(&normalize(&q.name))).cloned().collect();
    let rels: Vec<PreservedRel> = p.rels.iter().filter(|r| !dropped_ids.contains(r.id.as_str())).cloned().collect();
    let third_party: HashSet<String> = rels.iter().filter(|r| !r.external && is_third_party(&r.rel_type)).flat_map(|r| reachable(p, r)).collect();
    let names = write_parts_with_media(out, &parts, false, &third_party);
    let ids = attach(wb_rels, &rels, &names, "xl");
    for r in &p.package_rels {
        if let Some(target) = rel_target(r, &names, "") {
            out.kept.root.push((r.rel_type.clone(), target, r.external));
        }
    }
    let pivot_caches: Vec<(u32, String)> = p
        .elements
        .iter()
        .filter(|e| matches!(e.slot.as_str(), "pivotCache" | "unusedPivotCache"))
        .filter_map(|e| {
            let x = xml::parse(e.xml.as_bytes()).ok()?;
            Some((x.attr_u32("cacheId")?, ids.get(x.attr("id")?)?.clone()))
        })
        .collect();
    out.kept.wb_names = names;
    out.kept.cache_ids = pivot_caches.iter().map(|(id, _)| *id).collect();
    WorkbookKept {
        pivot_caches,
        elements: p.elements.iter().filter(|e| e.slot == "externalReferences").filter_map(|e| remap(&e.xml, &ids)).collect(),
        custom_views: String::new(),
        sheets: live
            .iter()
            .filter_map(|s| {
                Some(KeptSheet {
                    before: s.before,
                    name: s.name.clone(),
                    visibility: s.visibility,
                    rid: ids.get(&s.rel_id)?.clone(),
                    sheet_id: s.sheet_id,
                })
            })
            .collect(),
    }
}

/// The workbook's kept elements of `slot` that refer to no part (`fileVersion`, `ext`).
pub fn workbook_elements(wb: &Workbook, slot: &str) -> String {
    let Some(p) = wb.preserved.as_deref() else { return String::new() };
    p.elements.iter().filter(|e| e.slot == slot).map(|e| e.xml.as_str()).collect()
}

/// The workbook's kept `<customWorkbookViews>` once its sheets are written (`ids`): a view whose
/// active sheet is no longer written under the id it had (deleted) shows the first sheet.
pub fn custom_workbook_views(wb: &Workbook, ids: &SheetIds) -> String {
    let x = workbook_elements(wb, "customWorkbookViews");
    let Ok(el) = xml::parse(x.as_bytes()) else { return String::new() };
    let first = ids.first.unwrap_or(1);
    custom_workbook_views_xml(&el, &|id| if ids.own.contains(&id) { id } else { first }).unwrap_or_default()
}

/// The guids (lower case) of the workbook's kept Custom Views: a sheet writes only its views with one of them.
pub fn custom_view_guids(wb: &Workbook) -> HashSet<String> {
    let x = workbook_elements(wb, "customWorkbookViews");
    let Ok(el) = xml::parse(x.as_bytes()) else { return HashSet::new() };
    el.kids("customWorkbookView").filter_map(|v| v.attr("guid")).map(|g| g.trim().to_ascii_lowercase()).collect()
}

/// Adds the kept package-root relationships (custom properties…) to `_rels/.rels`.
pub fn write_package_rels(root: &mut Rels, out: &mut Out) {
    for (ty, target, external) in std::mem::take(&mut out.kept.root) {
        root.add_raw(&ty, &target, external);
    }
}
