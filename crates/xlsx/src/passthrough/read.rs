use super::*;

/// A part's relationships with full type URIs (Strict types translated to Transitional, which is
/// what is saved). `Ok(vec![])` when it has none; an error when the rels part is damaged or too
/// large (its part then can't be kept faithfully).
pub(crate) fn raw_rels(cx: &mut Ctx<'_>, part: &str) -> Result<Vec<PreservedRel>, String> {
    let mut rels = rels_as_written(cx, part)?;
    for r in &mut rels {
        if let Cow::Owned(t) = strict_rel_type_to_transitional(&r.rel_type) {
            r.rel_type = t;
        }
    }
    Ok(rels)
}

/// [`raw_rels`] with the relationship types as the package spells them.
fn rels_as_written(cx: &mut Ctx<'_>, part: &str) -> Result<Vec<PreservedRel>, String> {
    let name = rels_part_name(part);
    let bytes = match cx.pkg.read_limited(&name, MAX_META) {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let root = xml::parse(&bytes).map_err(|e| format!("{name}: {e}"))?;
    let (dir, _) = package::split_dir(part);
    let mut out = vec![];
    for r in root.kids("Relationship") {
        if out.len() >= MAX_RELS {
            return Err(format!("{name}: too many relationships"));
        }
        let (Some(id), Some(target)) = (r.attr("Id"), r.attr("Target")) else { continue };
        let external = r.attr("TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("External"));
        out.push(PreservedRel {
            id: id.to_string(),
            rel_type: r.attr("Type").unwrap_or("").to_string(),
            target: if external { target.to_string() } else { package::resolve(dir, target) },
            external,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- reading

/// The `<sheet>` entries of the workbook that are kept sheets (chart, dialog and macro sheets), each
/// with the number of worksheets (model sheets) before it.
fn kept_sheet_entries<'a>(wb_xml: &'a xml::El, wb_rels: &[PreservedRel], file_to_model: &[Option<usize>]) -> Vec<(usize, &'a xml::El)> {
    let mut out = vec![];
    let mut worksheets = 0usize;
    for (i, se) in wb_xml.child("sheets").into_iter().flat_map(|s| s.kids("sheet")).enumerate() {
        if file_to_model.get(i).copied().flatten().is_some() {
            worksheets += 1;
        } else if se.attr("id").and_then(|id| wb_rels.iter().find(|r| r.id == id)).is_some_and(|r| is_kind(r, KEPT_SHEET_KINDS)) {
            out.push((worksheets, se));
        }
    }
    out
}

/// What a kept `<sheet>` entry is, for warnings.
fn sheet_kind_label(se: &xml::El, wb_rels: &[PreservedRel]) -> &'static str {
    match se.attr("id").and_then(|id| wb_rels.iter().find(|r| r.id == id)) {
        Some(r) if is_kind(r, &["dialogsheet"]) => "dialog sheet",
        Some(r) if is_kind(r, MACRO_SHEET_KINDS) => "macro sheet",
        _ => "chart sheet",
    }
}

/// Collects kept content while a package is read.
pub struct KeptReader {
    on: bool,
    /// A Strict package: kept XML is translated to Transitional.
    strict: bool,
    /// The warning about a Strict namespace with no Transitional spelling was given.
    warned_unknown_namespace: bool,
    defaults: HashMap<String, String>,
    overrides: HashMap<String, String>,
    budget: usize,
    parts_left: usize,
}

impl KeptReader {
    pub fn new(cx: &mut Ctx<'_>) -> KeptReader {
        let mut k = KeptReader {
            on: true,
            strict: false,
            warned_unknown_namespace: false,
            defaults: HashMap::new(),
            overrides: HashMap::new(),
            budget: kept_total(),
            parts_left: MAX_KEPT_PARTS,
        };
        if let Ok(root) = rels_as_written(cx, "")
            && root.iter().any(|r| r.rel_type.starts_with("http://purl.oclc.org/"))
        {
            k.strict = true;
        }
        match cx.pkg.read_limited("[Content_Types].xml", MAX_META).ok().flatten().and_then(|b| xml::parse(&b).ok()) {
            Some(types) => {
                for d in types.kids("Default") {
                    if let (Some(e), Some(t)) = (d.attr("Extension"), d.attr("ContentType")) {
                        k.defaults.insert(e.to_ascii_lowercase(), t.to_string());
                    }
                }
                for o in types.kids("Override") {
                    if let (Some(p), Some(t)) = (o.attr("PartName"), o.attr("ContentType")) {
                        k.overrides.insert(normalize(p), t.to_string());
                    }
                }
            }
            None => k.on = false,
        }
        k
    }

    /// Kept XML of a Strict package (`name` names it in warnings) as Transitional; other content
    /// unchanged. An error when it holds values Transitional can't express (the part is dropped).
    fn transitional(&mut self, cx: &mut Ctx<'_>, name: &str, xml: String) -> Result<String, String> {
        if !self.strict {
            return Ok(xml);
        }
        match to_transitional(&xml) {
            Ok(Cow::Borrowed(_)) => Ok(xml),
            Ok(Cow::Owned(x)) => Ok(x),
            Err(Untranslatable::UnknownNamespace(ns)) => {
                if !self.warned_unknown_namespace {
                    self.warned_unknown_namespace = true;
                    cx.warn(format!(
                        "{name} uses the Strict Open XML namespace {ns}, which has no Transitional equivalent known here; it is kept unchanged"
                    ));
                }
                Ok(xml)
            }
            Err(Untranslatable::StrictValue) => Err(format!("{name} holds Strict Open XML values that can't be saved as Transitional")),
            Err(Untranslatable::Damaged) => Err(format!("{name} could not be translated from Strict Open XML")),
        }
    }

    /// [`Self::transitional`] for a kept part's bytes; binary parts are kept byte for byte.
    fn transitional_part(&mut self, cx: &mut Ctx<'_>, name: &str, content_type: &str, data: Vec<u8>) -> Result<Vec<u8>, String> {
        if !self.strict || !is_xml_part(name, content_type) {
            return Ok(data);
        }
        let (bom, body) = match data.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
            Some(rest) => (true, rest),
            None => (false, data.as_slice()),
        };
        let Ok(text) = std::str::from_utf8(body) else {
            // UTF-16 (or not text): only a part with no Strict namespace can be kept as it is.
            let decoded = xml::to_utf8(&data);
            return if String::from_utf8_lossy(&decoded).contains("http://purl.oclc.org/") {
                Err(format!("{name} could not be translated from Strict Open XML"))
            } else {
                Ok(data)
            };
        };
        if !text.contains("http://purl.oclc.org/") {
            return Ok(data);
        }
        let out = self.transitional(cx, name, text.to_string())?;
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }

    fn content_type(&self, part: &str) -> Option<String> {
        let key = normalize(part);
        if let Some(t) = self.overrides.get(&key) {
            return Some(t.clone());
        }
        let (_, file) = package::split_dir(&key);
        let ext = file.rsplit_once('.').map(|(_, e)| e)?;
        self.defaults.get(ext).cloned()
    }

    /// The parts reachable from `root` that aren't in `seen` yet. Fails (and keeps nothing) when the
    /// graph reaches rewritten content, a missing or damaged part, or a size limit.
    fn closure(&mut self, cx: &mut Ctx<'_>, root: &PreservedRel, seen: &HashSet<String>, through: &[&str]) -> Result<Vec<PreservedPart>, String> {
        if root.external {
            return Ok(vec![]);
        }
        let blocked = |r: &PreservedRel| rewritten(r) && !through.iter().any(|k| k.eq_ignore_ascii_case(kind(&r.rel_type)));
        if blocked(root) {
            return Err(format!("it refers to {}", kind(&root.rel_type)));
        }
        let mut parts = vec![];
        let mut bytes = 0usize;
        let mut local: HashSet<String> = HashSet::new();
        let mut queue = VecDeque::from([(root.target.clone(), 0usize)]);
        while let Some((name, depth)) = queue.pop_front() {
            let key = normalize(&name);
            if seen.contains(&key) || !local.insert(key) {
                continue;
            }
            if depth > MAX_CHAIN {
                return Err("its relationships are nested too deeply".into());
            }
            if parts.len() >= self.parts_left {
                return Err("it has too many parts".into());
            }
            let content_type = self.content_type(&name).ok_or_else(|| format!("{name} has no content type"))?;
            let data = match cx.pkg.read_limited(&name, MAX_KEPT_PART) {
                Ok(Some(d)) => d,
                Ok(None) => return Err(format!("{name} is missing")),
                Err(e) => return Err(e.to_string()),
            };
            let data = self.transitional_part(cx, &name, &content_type, data)?;
            bytes = bytes.saturating_add(data.len());
            if bytes > self.budget {
                return Err("it is too large".into());
            }
            let rels = raw_rels(cx, &name)?;
            for r in rels.iter().filter(|r| !r.external) {
                if blocked(r) {
                    return Err(format!("it refers to {}", kind(&r.rel_type)));
                }
                queue.push_back((r.target.clone(), depth + 1));
            }
            parts.push(PreservedPart { name, content_type, data: Arc::from(data), rels });
        }
        self.budget = self.budget.saturating_sub(bytes);
        self.parts_left = self.parts_left.saturating_sub(parts.len());
        Ok(parts)
    }

    /// Keeps a set of relationships (all or nothing) and the parts behind them.
    fn keep(&mut self, cx: &mut Ctx<'_>, p: &mut Preserved, roots: &[PreservedRel], seen: &mut HashSet<String>) -> Result<(), String> {
        self.keep_through(cx, p, roots, seen, &[])
    }

    /// [`Self::keep`] for roots whose graph may reach the rewritten kinds in `through`.
    fn keep_through(
        &mut self,
        cx: &mut Ctx<'_>,
        p: &mut Preserved,
        roots: &[PreservedRel],
        seen: &mut HashSet<String>,
        through: &[&str],
    ) -> Result<(), String> {
        p.parts.extend(self.parts_of(cx, roots, seen, through)?);
        for r in roots {
            if !p.rels.iter().any(|x| x.id == r.id) {
                p.rels.push(r.clone());
            }
        }
        Ok(())
    }

    /// The parts behind a set of relationships (all or nothing), added to `seen`.
    fn parts_of(
        &mut self,
        cx: &mut Ctx<'_>,
        roots: &[PreservedRel],
        seen: &mut HashSet<String>,
        through: &[&str],
    ) -> Result<Vec<PreservedPart>, String> {
        let mut new_parts: Vec<PreservedPart> = vec![];
        let mut scratch = seen.clone();
        for r in roots {
            let parts = self.closure(cx, r, &scratch, through)?;
            scratch.extend(parts.iter().map(|q| normalize(&q.name)));
            new_parts.extend(parts);
        }
        *seen = scratch;
        Ok(new_parts)
    }

    /// Package-root and workbook-level content.
    pub fn workbook(&mut self, cx: &mut Ctx<'_>, wb_part: &str, wb_xml: &xml::El, file_to_model: &[Option<usize>]) -> Option<Arc<Preserved>> {
        if !self.on {
            let wb_rels = raw_rels(cx, wb_part).unwrap_or_default();
            for (_, se) in kept_sheet_entries(wb_xml, &wb_rels, file_to_model) {
                let what = sheet_kind_label(se, &wb_rels);
                cx.warn(format!("{what} \"{}\" was skipped and is not kept on save", se.attr("name").unwrap_or("")));
            }
            return None;
        }
        let mut p = Preserved::default();
        let mut seen = HashSet::new();
        // Package root: custom document properties, ribbon customisation, task panes, sensitivity label.
        if let Ok(root) = raw_rels(cx, "") {
            for r in root.iter().filter(|r| is_kind(r, PACKAGE_ROOTS)) {
                let mut q = Preserved::default();
                match self.keep(cx, &mut q, std::slice::from_ref(r), &mut seen) {
                    Ok(()) => {
                        p.parts.extend(q.parts);
                        p.package_rels.extend(q.rels);
                    }
                    Err(why) => cx.warn(format!("{} was dropped: {why}", kind(&r.rel_type))),
                }
            }
        }
        let wb_rels = raw_rels(cx, wb_part).unwrap_or_default();
        for r in wb_rels.iter().filter(|r| is_kind(r, WORKBOOK_ROOTS)) {
            if let Err(why) = self.keep(cx, &mut p, std::slice::from_ref(r), &mut seen) {
                cx.warn(format!("{} part {} was dropped: {why}", kind(&r.rel_type), r.target));
            }
        }
        // Office add-in task panes: the task panes part and the web extension parts it names.
        for r in wb_rels.iter().filter(|r| is_kind(r, WORKBOOK_WEB_EXTENSIONS)) {
            match self.keep(cx, &mut p, std::slice::from_ref(r), &mut seen) {
                Ok(()) => cx.warn("Office add-in task panes are not shown; they are kept for saving"),
                Err(why) => cx.warn(format!("Office add-in task panes were dropped: {why}")),
            }
        }
        // Chart, dialog and macro sheets: the sheet entry (name, state, position) plus the sheet
        // part and the drawing, chart, style, colour, VML and control parts behind it.
        for (worksheets, se) in kept_sheet_entries(wb_xml, &wb_rels, file_to_model) {
            let name = se.attr("name").unwrap_or("").to_string();
            let Some(r) = se.attr("id").and_then(|id| wb_rels.iter().find(|r| r.id == id)) else { continue };
            let what = sheet_kind_label(se, &wb_rels);
            if !is_kind(r, &["chartsheet"]) {
                let note = if is_kind(r, MACRO_SHEET_KINDS) { "; kept only when saved as a macro-enabled workbook" } else { "" };
                cx.warn(format!("{what} \"{name}\" is kept as it is but can't be shown or edited{note}"));
            }
            match self.keep_through(cx, &mut p, std::slice::from_ref(r), &mut seen, KEPT_SHEET_GRAPH) {
                Ok(()) => p.sheets.push(PreservedSheet {
                    name,
                    visibility: match se.attr("state") {
                        Some("hidden") => Visibility::Hidden,
                        Some("veryHidden") => Visibility::VeryHidden,
                        _ => Visibility::Visible,
                    },
                    rel_id: r.id.clone(),
                    before: worksheets,
                    sheet_id: se.attr_u32("sheetId").unwrap_or(0),
                }),
                Err(why) => cx.warn(format!("{what} \"{name}\" was dropped: {why}")),
            }
        }
        // Links to external workbooks: `[1]` in formulas is the position in <externalReferences>,
        // so they are kept all together or not at all.
        let ids: Vec<&str> =
            wb_xml.child("externalReferences").map(|e| e.kids("externalReference").filter_map(|x| x.attr("id")).collect()).unwrap_or_default();
        if !ids.is_empty() {
            let roots: Option<Vec<PreservedRel>> =
                ids.iter().map(|id| wb_rels.iter().find(|r| r.id == *id && is_kind(r, &["externalLink"])).cloned()).collect();
            let kept = match roots {
                Some(roots) => self.keep(cx, &mut p, &roots, &mut seen),
                None => Err("a link has no part".into()),
            };
            match kept {
                Ok(()) => {
                    let mut x = format!("<externalReferences xmlns:r=\"{NS_REL}\">");
                    for id in &ids {
                        let _ = write!(x, "<externalReference r:id=\"{}\"/>", esc_attr(id));
                    }
                    x.push_str("</externalReferences>");
                    p.elements.push(PreservedXml { slot: "externalReferences".into(), xml: x });
                    cx.warn("links to external workbooks were kept; they are not followed or refreshed");
                }
                Err(why) => cx.warn(format!("links to external workbooks were dropped: {why}")),
            }
        }
        // Excel trusts the saved cell values only when both are present; without them it
        // recalculates everything on open, and links into closed workbooks come back as errors.
        if let Some(v) = wb_xml.child("fileVersion") {
            let mut x = String::from("<fileVersion");
            for k in FILE_VERSION_ATTRS {
                if let Some(a) = v.attr(k) {
                    let _ = write!(x, " {k}=\"{}\"", esc_attr(a));
                }
            }
            x.push_str("/>");
            p.elements.push(PreservedXml { slot: "fileVersion".into(), xml: x });
        }
        let calc_features = wb_xml
            .child("extLst")
            .and_then(|l| l.kids("ext").find(|e| e.attr("uri").is_some_and(|u| u.eq_ignore_ascii_case(CALC_FEATURES_URI))))
            .and_then(|e| e.child("calcFeatures"));
        if let Some(c) = calc_features {
            let mut x = format!("<ext uri=\"{CALC_FEATURES_URI}\" xmlns:xcalcf=\"{NS_CALC_FEATURES}\"><xcalcf:calcFeatures>");
            for name in c.kids("feature").filter_map(|f| f.attr("name")) {
                let _ = write!(x, "<xcalcf:feature name=\"{}\"/>", esc_attr(name));
            }
            x.push_str("</xcalcf:calcFeatures></ext>");
            p.elements.push(PreservedXml { slot: "ext".into(), xml: x });
        }
        (!p.is_empty()).then(|| Arc::new(p))
    }

    /// Sheet-level content of the sheet part `part`.
    pub fn sheet(&mut self, cx: &mut Ctx<'_>, part: &str, name: &str) -> Option<Arc<Preserved>> {
        if !self.on {
            return None;
        }
        let rels = match raw_rels(cx, part) {
            Ok(r) => r,
            Err(e) => {
                cx.warn(format!("sheet \"{name}\": relationships could not be read ({e}); unmodelled content was dropped"));
                return None;
            }
        };
        let mut p = Preserved::default();
        let mut seen = HashSet::new();
        for r in rels.iter().filter(|r| is_kind(r, SHEET_ROOTS)) {
            if let Err(why) = self.keep(cx, &mut p, std::slice::from_ref(r), &mut seen) {
                cx.warn(format!("sheet \"{name}\": {} was dropped: {why}", kind(&r.rel_type)));
            }
        }
        if let Some(r) = rels.iter().find(|r| !r.external && is_kind(r, &["drawing"])) {
            self.drawing_anchors(cx, name, r, &mut p, &mut seen);
        }
        // Notes have one VML drawing per sheet; another one holds the header and footer pictures.
        let vml_drawings = rels.iter().filter(|r| is_kind(r, &["vmlDrawing"])).count();
        let notes = rels.iter().filter(|r| is_kind(r, &["comments"])).count();
        if rels.iter().any(|r| is_kind(r, SHEET_ELEMENT_KINDS) || is_kind(r, &["customProperty"])) || vml_drawings > notes {
            self.sheet_elements(cx, part, name, &rels, &mut p, &mut seen);
        }
        (!p.is_empty()).then(|| Arc::new(p))
    }

    fn sheet_elements(&mut self, cx: &mut Ctx<'_>, part: &str, name: &str, rels: &[PreservedRel], p: &mut Preserved, seen: &mut HashSet<String>) {
        let scan = match cx.pkg.read(part) {
            Ok(Some(bytes)) => scan_sheet(&bytes),
            _ => None,
        };
        let Some(scan) = scan else {
            cx.warn(format!("sheet \"{name}\": controls and embedded objects could not be read and were dropped"));
            return;
        };
        // Printer settings (a binary part per sheet) hang off <pageSetup r:id>.
        if let Some(id) = scan.page_setup.as_deref()
            && let Some(r) = rels.iter().find(|r| r.id == id && is_kind(r, &["printerSettings"]))
        {
            match self.keep(cx, p, std::slice::from_ref(r), seen) {
                Ok(()) => p.elements.push(PreservedXml { slot: "pageSetup".into(), xml: r.id.clone() }),
                Err(why) => cx.warn(format!("sheet \"{name}\": printer settings were dropped: {why}")),
            }
        }
        // Header and footer pictures (`&G`): a VML drawing of their own and the images it shows,
        // kept byte for byte (GridCraft writes the header text, not the pictures).
        if let Some(id) = scan.legacy_drawing_hf.as_deref()
            && scan.legacy_drawing.as_deref() != Some(id)
            && let Some(r) = rels.iter().find(|r| r.id == id && is_kind(r, &["vmlDrawing"]))
        {
            match self.keep_through(cx, p, std::slice::from_ref(r), seen, &["vmlDrawing"]) {
                Ok(()) => {
                    let xml = format!("<legacyDrawingHF xmlns:r=\"{NS_REL}\" r:id=\"{}\"/>", esc_attr(&r.id));
                    p.elements.push(PreservedXml { slot: "legacyDrawingHF".into(), xml });
                    cx.warn(format!("sheet \"{name}\": pictures in the header and footer are not shown; they are kept for saving"));
                }
                Err(why) => cx.warn(format!("sheet \"{name}\": header and footer pictures were dropped: {why}")),
            }
        }
        let mut frags = scan.fragments;
        // Worksheet custom properties: add-in tags, each naming a part of its own.
        if let Some(x) = frags.iter().position(|(slot, _)| slot == "customProperties").map(|i| frags.remove(i).1) {
            match self.custom_properties(cx, &x, rels, p, seen) {
                Ok(()) => p.elements.push(PreservedXml { slot: "customProperties".into(), xml: x }),
                Err(why) => cx.warn(format!("sheet \"{name}\": custom properties were dropped: {why}")),
            }
        }
        // Controls and OLE objects are drawn from shapes in the legacy VML drawing.
        if frags.iter().any(|(slot, _)| slot != "picture") {
            match self.vml(cx, rels, scan.legacy_drawing.as_deref(), seen) {
                Ok((vml, parts)) => {
                    p.parts.extend(parts);
                    p.vml = Some(vml);
                }
                Err(why) => {
                    cx.warn(format!("sheet \"{name}\": controls and embedded objects were dropped: {why}"));
                    frags.retain(|(slot, _)| slot == "picture");
                }
            }
        }
        for (slot, x) in frags {
            let x = match self.transitional(cx, &format!("sheet \"{name}\": {slot}"), x) {
                Ok(x) => x,
                Err(why) => {
                    cx.warn(format!("sheet \"{name}\": {slot} were dropped: {why}"));
                    continue;
                }
            };
            let mut ids = vec![];
            let ok = map_rel_ids(&x, &mut |id| {
                ids.push(id.to_string());
                Some(id.to_string())
            })
            .is_some();
            let roots: Option<Vec<PreservedRel>> = ids.iter().map(|id| rels.iter().find(|r| r.id == *id).cloned()).collect();
            let kept = match (ok, roots) {
                (true, Some(roots)) => self.keep(cx, p, &roots, seen),
                (false, _) => Err("its XML is damaged".into()),
                (_, None) => Err("it refers to a missing relationship".into()),
            };
            match kept {
                Ok(()) => p.elements.push(PreservedXml { slot, xml: x }),
                Err(why) => cx.warn(format!("sheet \"{name}\": {slot} were dropped: {why}")),
            }
        }
        if p.elements.iter().any(|e| e.slot == "oleObjects" || e.slot == "controls") {
            cx.warn(format!("sheet \"{name}\": embedded objects and form controls are not shown; they are kept for saving"));
        } else {
            p.vml = None;
        }
    }

    /// The anchors of the sheet's drawing part the drawing reader does not model (form and ActiveX
    /// control shapes wrapped in `mc:AlternateContent`, SmartArt graphics), with the parts they
    /// refer to.
    fn drawing_anchors(&mut self, cx: &mut Ctx<'_>, name: &str, drawing: &PreservedRel, p: &mut Preserved, seen: &mut HashSet<String>) {
        let anchors = match cx.pkg.read_limited(&drawing.target, MAX_META) {
            Ok(Some(bytes)) => scan_drawing(&bytes),
            _ => return,
        };
        let Some(anchors) = anchors else {
            cx.warn(format!(
                "sheet \"{name}\": drawing objects in markup-compatibility blocks and SmartArt graphics could not be read and were dropped"
            ));
            return;
        };
        if anchors.is_empty() {
            return;
        }
        let drels = match raw_rels(cx, &drawing.target) {
            Ok(r) => r,
            Err(e) => {
                cx.warn(format!("sheet \"{name}\": drawing objects in markup-compatibility blocks and SmartArt graphics were dropped ({e})"));
                return;
            }
        };
        let mut smartart = false;
        let mut group = false;
        let mut webextension = false;
        for (index, xml, kind) in anchors {
            let xml = match self.transitional(cx, &drawing.target, xml) {
                Ok(x) => x,
                Err(why) => {
                    cx.warn(format!("sheet \"{name}\": a drawing object in a markup-compatibility block or a SmartArt graphic was dropped: {why}"));
                    continue;
                }
            };
            let mut ids = vec![];
            let ok = map_rel_ids(&xml, &mut |id| {
                ids.push(id.to_string());
                Some(id.to_string())
            })
            .is_some();
            let mut roots: Option<Vec<PreservedRel>> = ids.iter().map(|id| drels.iter().find(|r| r.id == *id).cloned()).collect();
            if kind == AnchorKind::SmartArt
                && let Some(roots) = roots.as_mut()
                && let Some(r) = self.diagram_drawing(cx, roots, &drels)
                && !roots.iter().any(|x| x.id == r.id)
            {
                roots.push(r);
            }
            let unsupported = if kind == AnchorKind::Group { group_unsupported(&xml) } else { None };
            let kept = match (ok, roots) {
                _ if unsupported.is_some() => Err(unsupported.unwrap_or_default().to_string()),
                (false, _) => Err("its XML is damaged".to_string()),
                (_, None) => Err("it refers to a missing relationship".to_string()),
                (true, Some(_)) if xml.len() > self.budget => Err("it is too large".to_string()),
                (true, Some(roots)) => self.parts_of(cx, &roots, seen, &[]).map(|parts| (roots, parts)),
            };
            match kept {
                Ok((roots, parts)) => {
                    self.budget = self.budget.saturating_sub(xml.len());
                    p.parts.extend(parts);
                    for r in roots {
                        if !p.drawing_rels.iter().any(|x| x.id == r.id) {
                            p.drawing_rels.push(r);
                        }
                    }
                    p.anchors.push(PreservedAnchor { index, xml });
                    smartart |= kind == AnchorKind::SmartArt;
                    group |= kind == AnchorKind::Group;
                    webextension |= kind == AnchorKind::WebExtension;
                }
                Err(why) => match kind {
                    AnchorKind::Control => cx.warn(format!("sheet \"{name}\": a drawing object in a markup-compatibility block was dropped: {why}")),
                    AnchorKind::SmartArt => cx.warn(format!("sheet \"{name}\": a SmartArt graphic was dropped: {why}")),
                    AnchorKind::Group => cx.warn(format!("sheet \"{name}\": a group of drawing objects was dropped: {why}")),
                    AnchorKind::WebExtension => cx.warn(format!("sheet \"{name}\": an Office add-in was dropped: {why}")),
                },
            }
        }
        if smartart {
            cx.warn(format!("sheet \"{name}\": SmartArt graphics are not shown; they are kept for saving"));
        }
        if group {
            cx.warn(format!("sheet \"{name}\": grouped drawing objects are not shown; they are kept for saving"));
        }
        if webextension {
            cx.warn(format!("sheet \"{name}\": Office add-ins are not shown; they are kept for saving"));
        }
    }

    /// The drawing relationship of a SmartArt graphic's pre-rendered shapes (the diagram drawing
    /// part): its id is not in the anchor but in the data part's `dsp:dataModelExt relId`.
    fn diagram_drawing(&self, cx: &mut Ctx<'_>, roots: &[PreservedRel], drels: &[PreservedRel]) -> Option<PreservedRel> {
        let data = roots.iter().find(|r| !r.external && is_kind(r, &["diagramData"]))?;
        let bytes = cx.pkg.read_limited(&data.target, MAX_META).ok()??;
        let id = data_model_ext_rel_id(&bytes)?;
        drels.iter().find(|r| r.id == id && !r.external).cloned()
    }

    /// Keeps the parts behind a sheet's `<customProperties>`: every `<customPr>` must name a
    /// `customProperty` relationship of the sheet, and each part must have that content type.
    fn custom_properties(
        &mut self,
        cx: &mut Ctx<'_>,
        x: &str,
        rels: &[PreservedRel],
        p: &mut Preserved,
        seen: &mut HashSet<String>,
    ) -> Result<(), String> {
        let el = xml::parse(x.as_bytes()).map_err(|_| "its XML is damaged".to_string())?;
        let mut roots = vec![];
        for pr in el.kids("customPr") {
            let id = pr.attr("id").ok_or("a custom property has no part")?;
            let r = rels.iter().find(|r| r.id == id && is_kind(r, &["customProperty"])).ok_or("it refers to a missing relationship")?;
            if !roots.iter().any(|q: &PreservedRel| q.id == r.id) {
                roots.push(r.clone());
            }
        }
        if roots.is_empty() {
            return Err("it names no part".into());
        }
        let mut q = Preserved::default();
        self.keep(cx, &mut q, &roots, seen)?;
        if let Some(bad) = q.parts.iter().find(|part| !part.content_type.eq_ignore_ascii_case(CT_CUSTOM_PROPERTY)) {
            return Err(format!("{} is not a custom property part", bad.name));
        }
        p.parts.extend(q.parts);
        p.rels.extend(q.rels);
        Ok(())
    }

    /// The sheet's VML drawing without its notes (GridCraft writes those), and the parts its
    /// shapes refer to (preview images).
    fn vml(
        &mut self,
        cx: &mut Ctx<'_>,
        rels: &[PreservedRel],
        rid: Option<&str>,
        seen: &mut HashSet<String>,
    ) -> Result<(PreservedPart, Vec<PreservedPart>), String> {
        let rel = rid.and_then(|id| rels.iter().find(|r| r.id == id && !r.external)).ok_or("the sheet has no VML drawing")?;
        // Charged to the kept total like any other part: many sheets can point at one highly
        // compressed VML drawing, and each keeps its own copy.
        if self.budget == 0 {
            return Err("it is too large".into());
        }
        let limit = MAX_META.min(u64::try_from(self.budget).unwrap_or(u64::MAX));
        let data = match cx.pkg.read_limited(&rel.target, limit) {
            Ok(Some(d)) => d,
            Ok(None) => return Err(format!("{} is missing", rel.target)),
            Err(IoError::TooLarge(_)) => return Err("it is too large".into()),
            Err(e) => return Err(e.to_string()),
        };
        self.budget = self.budget.saturating_sub(data.len());
        let text = String::from_utf8(data).map_err(|_| "the VML drawing is not UTF-8".to_string())?;
        let text = strip_notes(&text);
        let text = self.transitional(cx, &rel.target, text)?;
        let vrels = raw_rels(cx, &rel.target)?;
        let mut q = Preserved::default();
        let roots: Vec<PreservedRel> = vrels.iter().filter(|r| !r.external).cloned().collect();
        self.keep(cx, &mut q, &roots, seen)?;
        let vml = PreservedPart { name: rel.target.clone(), content_type: CT_VML.into(), data: Arc::from(text.into_bytes()), rels: vrels };
        Ok((vml, q.parts))
    }
}

/// What a sheet part holds for pass-through.
struct SheetScan {
    /// (slot, XML with the namespace declarations it needs).
    fragments: Vec<(String, String)>,
    legacy_drawing: Option<String>,
    /// The relationship id on `<legacyDrawingHF>` (header and footer pictures).
    legacy_drawing_hf: Option<String>,
    /// The relationship id on `<pageSetup>` (printer settings).
    page_setup: Option<String>,
}

/// Finds the sheet's `<customProperties>`, `<picture>`, `<oleObjects>`, `<controls>` (also inside
/// `<mc:AlternateContent>`) and the `<legacyDrawing>` and `<legacyDrawingHF>` ids. `None` when the XML is damaged.
fn scan_sheet(bytes: &[u8]) -> Option<SheetScan> {
    let data = xml::to_utf8(bytes);
    let text = std::str::from_utf8(&data).ok()?;
    let mut reader = Reader::from_str(text);
    let mut decls: Vec<(String, String)> = vec![];
    let mut out = SheetScan { fragments: vec![], legacy_drawing: None, legacy_drawing_hf: None, page_setup: None };
    let mut in_root = false;
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(e) if !in_root => {
                in_root = true;
                for a in e.attributes().with_checks(false).flatten() {
                    if let Some(p) = a.key.as_ref().strip_prefix(b"xmlns:")
                        && let (Ok(p), Ok(v)) = (std::str::from_utf8(p), std::str::from_utf8(&a.value))
                        && !v.contains('"')
                    {
                        decls.push((p.to_string(), v.to_string()));
                    }
                }
            }
            Event::Start(e) => {
                let local = e.local_name().as_ref().to_vec();
                match local.as_slice() {
                    b"pageSetup" => out.page_setup = rel_id_attr(&e),
                    b"legacyDrawing" => out.legacy_drawing = rel_id_attr(&e),
                    b"legacyDrawingHF" => out.legacy_drawing_hf = rel_id_attr(&e),
                    _ => {}
                }
                reader.read_to_end(e.name()).ok()?;
                let end = usize::try_from(reader.buffer_position()).ok()?;
                let wanted = matches!(local.as_slice(), b"customProperties" | b"picture" | b"oleObjects" | b"controls" | b"AlternateContent");
                if wanted && end.saturating_sub(start) <= MAX_FRAGMENT {
                    let frag = text.get(start..end)?;
                    push_fragment(&mut out, frag, &local, &decls);
                }
            }
            Event::Empty(e) if in_root => {
                let local = e.local_name().as_ref().to_vec();
                if local == b"legacyDrawing" || local == b"legacyDrawingHF" || local == b"pageSetup" {
                    let id = rel_id_attr(&e);
                    match local.as_slice() {
                        b"pageSetup" => out.page_setup = id,
                        b"legacyDrawing" => out.legacy_drawing = id,
                        _ => out.legacy_drawing_hf = id,
                    }
                } else if local == b"picture"
                    && let Some(frag) = text.get(start..usize::try_from(reader.buffer_position()).ok()?)
                {
                    push_fragment(&mut out, frag, &local, &decls);
                }
            }
            Event::End(_) => break,
            _ => {}
        }
    }
    Some(out)
}

/// What a kept drawing anchor holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnchorKind {
    /// An `mc:AlternateContent` block (form and ActiveX control shapes).
    Control,
    /// An anchor whose graphic frame is a SmartArt graphic (ECMA-376 Part 1 §21.4).
    SmartArt,
    /// An anchor whose object is a group of drawing objects (`xdr:grpSp`, ECMA-376 Part 1 §20.5.2.17).
    Group,
    /// An anchor holding an Office add-in (a web extension, [MS-OWEXML] 2.1.1).
    WebExtension,
}

/// The `graphicData` URI of a SmartArt graphic.
const URI_DIAGRAM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
/// The `graphicData` URIs of charts.
const URI_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const URI_CHARTEX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
/// The `graphicData` URI of an Office add-in's web extension ([MS-OWEXML] 2.1.1).
const URI_WEB_EXTENSION: &str = "http://schemas.microsoft.com/office/webextensions/webextension/2010/11";

/// Whether an anchor fragment (`twoCellAnchor`, `oneCellAnchor`, `absoluteAnchor`) holds an
/// Office add-in: a `graphicFrame` whose `graphicData` is a web extension, in the anchor itself or
/// in an `mc:Choice` of it (the fallback holds the add-in's snapshot picture).
fn is_web_extension_anchor(frag: &str) -> bool {
    if !frag.contains(URI_WEB_EXTENSION) {
        return false;
    }
    let Ok(el) = xml::parse(frag.as_bytes()) else { return false };
    let frames = el
        .children
        .iter()
        .filter(|c| c.name == "graphicFrame")
        .chain(el.children.iter().filter(|c| c.name == "AlternateContent").flat_map(|c| c.kids("Choice")).flat_map(|c| c.kids("graphicFrame")));
    frames.filter_map(|c| c.path(&["graphic", "graphicData"])).any(|gd| gd.attr("uri") == Some(URI_WEB_EXTENSION))
}

/// Whether an anchor fragment (`twoCellAnchor`, `oneCellAnchor`, `absoluteAnchor`) holds a
/// SmartArt graphic frame.
fn is_smartart_anchor(frag: &str) -> bool {
    if !frag.contains(URI_DIAGRAM) {
        return false;
    }
    let Ok(el) = xml::parse(frag.as_bytes()) else { return false };
    el.children
        .iter()
        .filter(|c| c.name == "graphicFrame")
        .filter_map(|c| c.path(&["graphic", "graphicData"]))
        .any(|gd| gd.attr("uri") == Some(URI_DIAGRAM))
}

/// Whether an anchor fragment's object (the child after `from`/`to`/`pos`/`ext`) is a group.
fn is_group_anchor(frag: &str) -> bool {
    if !frag.contains("grpSp") {
        return false;
    }
    let Ok(el) = xml::parse(frag.as_bytes()) else { return false };
    el.children.iter().find(|c| !matches!(c.name.as_str(), "from" | "to" | "pos" | "ext")).is_some_and(|c| c.name == "grpSp")
}

/// Why a kept group can't be kept as it is: it holds a chart or a SmartArt graphic, whose parts
/// the writer would not know about. `None` when it holds neither.
fn group_unsupported(frag: &str) -> Option<&'static str> {
    let Ok(el) = xml::parse(frag.as_bytes()) else { return Some("its XML is damaged") };
    let mut stack = vec![&el];
    while let Some(e) = stack.pop() {
        if e.name == "graphicData" {
            match e.attr("uri") {
                Some(URI_CHART) | Some(URI_CHARTEX) => return Some("it contains a chart"),
                Some(URI_DIAGRAM) => return Some("it contains a SmartArt graphic"),
                _ => {}
            }
        }
        stack.extend(e.children.iter());
    }
    None
}

/// The `relId` of a diagram data part's `dsp:dataModelExt` (the diagram drawing relationship).
fn data_model_ext_rel_id(bytes: &[u8]) -> Option<String> {
    let data = xml::to_utf8(bytes);
    let text = std::str::from_utf8(&data).ok()?;
    let mut reader = Reader::from_str(text);
    loop {
        match reader.read_event().ok()? {
            Event::Eof => return None,
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"dataModelExt" => {
                return e
                    .attributes()
                    .with_checks(false)
                    .flatten()
                    .find(|a| a.key.as_ref() == b"relId")
                    .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
            }
            _ => {}
        }
    }
}

/// The children of a drawing part's root GridCraft keeps as they are (`mc:AlternateContent`
/// blocks and SmartArt anchors), each with its position among the root's children and the root's
/// namespace declarations added. `None` when the XML is damaged.
fn scan_drawing(bytes: &[u8]) -> Option<Vec<(usize, String, AnchorKind)>> {
    let data = xml::to_utf8(bytes);
    let text = std::str::from_utf8(&data).ok()?;
    let mut reader = Reader::from_str(text);
    let mut decls: Vec<(String, String)> = vec![];
    let mut out = vec![];
    let mut in_root = false;
    let mut index = 0usize;
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(e) if !in_root => {
                in_root = true;
                for a in e.attributes().with_checks(false).flatten() {
                    let key = a.key.as_ref();
                    let prefix = if key == b"xmlns" { Some(&b""[..]) } else { key.strip_prefix(b"xmlns:") };
                    if let Some(p) = prefix
                        && let (Ok(p), Ok(v)) = (std::str::from_utf8(p), std::str::from_utf8(&a.value))
                        && !v.contains('"')
                    {
                        decls.push((p.to_string(), v.to_string()));
                    }
                }
            }
            Event::Start(e) => {
                let local = e.local_name().as_ref().to_vec();
                reader.read_to_end(e.name()).ok()?;
                let end = usize::try_from(reader.buffer_position()).ok()?;
                if end.saturating_sub(start) <= MAX_FRAGMENT {
                    let frag = text.get(start..end)?;
                    let kind = match local.as_slice() {
                        b"AlternateContent" => Some(AnchorKind::Control),
                        b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" if is_smartart_anchor(frag) => Some(AnchorKind::SmartArt),
                        b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" if is_group_anchor(frag) => Some(AnchorKind::Group),
                        b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" if is_web_extension_anchor(frag) => Some(AnchorKind::WebExtension),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        out.push((index, with_decls(frag, &decls), kind));
                    }
                }
                index = index.saturating_add(1);
            }
            Event::Empty(_) if in_root => index = index.saturating_add(1),
            Event::End(_) => break,
            _ => {}
        }
    }
    Some(out)
}

/// The `r:id` of an element (an `id` attribute with a namespace prefix).
fn rel_id_attr(e: &BytesStart<'_>) -> Option<String> {
    e.attributes()
        .with_checks(false)
        .flatten()
        .find(|a| a.key.local_name().as_ref() == b"id" && a.key.prefix().is_some())
        .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
}

fn push_fragment(out: &mut SheetScan, frag: &str, local: &[u8], decls: &[(String, String)]) {
    let frag = with_decls(frag, decls);
    let slot = if local == b"AlternateContent" {
        // The kept element is the one the first choice holds.
        let Ok(el) = xml::parse(frag.as_bytes()) else { return };
        match el.child("Choice").and_then(|c| c.children.first()) {
            Some(c) => c.name.clone(),
            None => return,
        }
    } else {
        String::from_utf8_lossy(local).into_owned()
    };
    if (SHEET_SLOTS.contains(&slot.as_str()) || SHEET_SLOTS_EARLY.contains(&slot.as_str())) && !out.fragments.iter().any(|(s, _)| *s == slot) {
        out.fragments.push((slot, frag));
    }
}

/// Adds the root's namespace declarations a fragment doesn't declare itself.
pub(crate) fn with_decls(frag: &str, decls: &[(String, String)]) -> String {
    let Some(name_end) = frag.char_indices().skip(1).find(|(_, c)| c.is_whitespace() || *c == '/' || *c == '>').map(|(i, _)| i) else {
        return frag.to_string();
    };
    let head = frag.get(..frag.find('>').unwrap_or(frag.len())).unwrap_or("");
    let mut add = String::new();
    for (p, v) in decls {
        if p.is_empty() {
            if !head.contains(" xmlns=") {
                let _ = write!(add, " xmlns=\"{v}\"");
            }
        } else if !head.contains(&format!("xmlns:{p}=")) {
            let _ = write!(add, " xmlns:{p}=\"{v}\"");
        }
    }
    format!("{}{add}{}", frag.get(..name_end).unwrap_or(""), frag.get(name_end..).unwrap_or(""))
}

/// Byte ranges of the `<v:shape …>…</v:shape>` (or self-closed) elements in VML text.
fn shape_spans(vml: &str) -> Vec<(usize, usize)> {
    let mut spans = vec![];
    let mut from = 0;
    while let Some(i) = vml.get(from..).and_then(|s| s.find("<v:shape")).map(|i| i + from) {
        let after = i + "<v:shape".len();
        let is_shape = vml.get(after..).and_then(|s| s.chars().next()).is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
        if !is_shape {
            from = after;
            continue;
        }
        let Some(tag_end) = vml.get(i..).and_then(|s| s.find('>')).map(|j| i + j + 1) else { break };
        let end = if vml.get(..tag_end).is_some_and(|t| t.ends_with("/>")) {
            tag_end
        } else {
            match vml.get(tag_end..).and_then(|s| s.find("</v:shape>")) {
                Some(j) => tag_end + j + "</v:shape>".len(),
                None => break,
            }
        };
        spans.push((i, end));
        from = end;
    }
    spans
}

/// VML without its note shapes (`ObjectType="Note"`).
fn strip_notes(vml: &str) -> String {
    let mut out = String::with_capacity(vml.len());
    let mut last = 0;
    for (a, b) in shape_spans(vml) {
        let shape = vml.get(a..b).unwrap_or("");
        if shape.contains("ObjectType=\"Note\"") {
            out.push_str(vml.get(last..a).unwrap_or(""));
            last = b;
        }
    }
    out.push_str(vml.get(last..).unwrap_or(""));
    out
}

/// Shape numbers (`_x0000_s1025` → 1025) used in VML text.
fn shape_numbers(vml: &str) -> Vec<u64> {
    vml.match_indices("_x0000_s")
        .filter_map(|(i, m)| {
            let digits: String = vml.get(i + m.len()..)?.chars().take_while(char::is_ascii_digit).take(18).collect();
            digits.parse().ok()
        })
        .collect()
}

/// The highest shape block VML text claims in `<o:idmap data>` or uses in a shape id.
pub(super) fn vml_block_max(vml: &str) -> u64 {
    let used = shape_numbers(vml).into_iter().max().unwrap_or(0) / 1024;
    let claimed = vml
        .find("<o:idmap")
        .and_then(|i| vml.get(i..)?.find("data=\"").map(|d| i + d + "data=\"".len()))
        .and_then(|d| Some((d, d + vml.get(d..)?.find('"')?)))
        .and_then(|(d, e)| vml.get(d..e))
        .map(|list| list.split(',').filter_map(|b| b.trim().parse::<u64>().ok()).max().unwrap_or(0))
        .unwrap_or(0);
    used.max(claimed)
}

/// Kept VML (controls, OLE objects) plus GridCraft's note shapes: the notes get ids in a fresh
/// shape block so they can't collide with the kept shapes the sheet XML refers to.
pub(super) fn merge_vml(kept: &str, ours: &str) -> String {
    let Some(close) = kept.rfind("</xml>") else { return ours.to_string() };
    let block = shape_numbers(kept).into_iter().max().unwrap_or(0) / 1024 + 1;
    let mut add = String::new();
    if !kept.contains("id=\"_x0000_t202\"")
        && let Some(i) = ours.find("<v:shapetype")
        && let Some(j) = ours.get(i..).and_then(|s| s.find("</v:shapetype>"))
    {
        add.push_str(ours.get(i..i + j + "</v:shapetype>".len()).unwrap_or(""));
    }
    for (k, (a, b)) in shape_spans(ours).into_iter().enumerate() {
        let shape = ours.get(a..b).unwrap_or("");
        let id = block.saturating_mul(1024).saturating_add(1 + k as u64);
        match (shape.find("id=\"_x0000_s"), shape.find("id=\"_x0000_s").and_then(|i| shape.get(i + 4..)?.find('"').map(|j| i + 4 + j))) {
            (Some(i), Some(j)) => {
                let _ = write!(add, "{}id=\"_x0000_s{id}\"{}", shape.get(..i).unwrap_or(""), shape.get(j + 1..).unwrap_or(""));
            }
            _ => add.push_str(shape),
        }
    }
    let mut head = kept.get(..close).unwrap_or("").to_string();
    // Claim the block in the drawing's id map.
    if let Some(i) = head.find("<o:idmap")
        && let Some(d) = head.get(i..).and_then(|s| s.find("data=\"")).map(|d| i + d + "data=\"".len())
        && let Some(e) = head.get(d..).and_then(|s| s.find('"')).map(|e| d + e)
    {
        head.insert_str(e, &format!(",{block}"));
    }
    format!("{head}{add}{}", kept.get(close..).unwrap_or(""))
}

/// Rewrites relationship-id attributes (`r:id`, `r:embed`…: any attribute in the relationships
/// namespace) of an XML fragment through `map`. `None` when the fragment is damaged or `map` has
/// no id for one of them.
pub fn map_rel_ids(x: &str, map: &mut dyn FnMut(&str) -> Option<String>) -> Option<String> {
    let mut reader = Reader::from_str(x);
    let mut writer = Writer::new(Vec::with_capacity(x.len()));
    let mut prefixes: HashSet<Vec<u8>> = HashSet::new();
    let mut depth = 0usize;
    loop {
        match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(e) => {
                depth += 1;
                if depth > MAX_FRAGMENT_DEPTH {
                    return None;
                }
                let e = map_start(&e, &mut prefixes, map)?;
                writer.write_event(Event::Start(e)).ok()?;
            }
            Event::Empty(e) => {
                let e = map_start(&e, &mut prefixes, map)?;
                writer.write_event(Event::Empty(e)).ok()?;
            }
            Event::End(e) => {
                depth = depth.saturating_sub(1);
                writer.write_event(Event::End(e)).ok()?;
            }
            ev => writer.write_event(ev).ok()?,
        }
    }
    String::from_utf8(writer.into_inner()).ok()
}

fn map_start(e: &BytesStart<'_>, prefixes: &mut HashSet<Vec<u8>>, map: &mut dyn FnMut(&str) -> Option<String>) -> Option<BytesStart<'static>> {
    for a in e.attributes().with_checks(false) {
        let a = a.ok()?;
        if let Some(p) = a.key.as_ref().strip_prefix(b"xmlns:")
            && (a.value.as_ref() == NS_REL.as_bytes() || a.value.as_ref() == NS_REL_STRICT.as_bytes())
        {
            prefixes.insert(p.to_vec());
        }
    }
    let name = String::from_utf8(e.name().as_ref().to_vec()).ok()?;
    let mut out = BytesStart::new(name);
    for a in e.attributes().with_checks(false) {
        let a = a.ok()?;
        let key = a.key.as_ref();
        let is_rel = !key.starts_with(b"xmlns") && a.key.prefix().is_some_and(|p| prefixes.contains(p.as_ref()));
        if is_rel {
            let old = std::str::from_utf8(&a.value).ok()?;
            let new = map(old)?;
            out.push_attribute(Attribute { key: QName(key), value: Cow::Owned(esc_attr(&new).into_bytes()) });
        } else {
            out.push_attribute(Attribute { key: QName(key), value: a.value.clone() });
        }
    }
    Some(out)
}

#[cfg(test)]
mod group_tests {
    use super::*;

    const GROUP: &str =
        r#"<xdr:twoCellAnchor xmlns:xdr="x" xmlns:a="a"><xdr:from/><xdr:to/><xdr:grpSp><xdr:sp/>{}</xdr:grpSp><xdr:clientData/></xdr:twoCellAnchor>"#;

    fn group_with(inner: &str) -> String {
        GROUP.replace("{}", inner)
    }

    #[test]
    fn group_anchor_is_recognised_by_its_object() {
        assert!(is_group_anchor(&group_with("")));
        // A shape whose name mentions grpSp is not a group.
        assert!(!is_group_anchor(r#"<xdr:oneCellAnchor xmlns:xdr="x"><xdr:from/><xdr:ext/><xdr:sp name="grpSp"/></xdr:oneCellAnchor>"#));
    }

    #[test]
    fn group_with_chart_or_smartart_is_not_kept() {
        assert_eq!(group_unsupported(&group_with("")), None);
        let frame = |uri: &str| format!(r#"<xdr:graphicFrame><a:graphic><a:graphicData uri="{uri}"/></a:graphic></xdr:graphicFrame>"#);
        assert_eq!(group_unsupported(&group_with(&frame(URI_CHART))), Some("it contains a chart"));
        let nested = group_with(&format!("<xdr:grpSp>{}</xdr:grpSp>", frame(URI_DIAGRAM)));
        assert_eq!(group_unsupported(&nested), Some("it contains a SmartArt graphic"));
    }
}
