use super::selection::{select_region, selection_set};
use super::{CellRef, EngineError, Json, RangeRef, Result, Session, bad, bool_param, coalesce, json, split_sheet, str_param};

/// How many references the Go To dialog keeps for going back to.
pub(super) const GO_TO_RECENT: usize = 10;

/// The most areas one Go To selects.
pub(super) const GO_TO_MAX_AREAS: usize = 2000;

/// How deep a name that refers to other names is followed.
const GO_TO_NAME_DEPTH: usize = 8;

pub(in super::super) fn go_to(s: &mut Session, p: &Json) -> Result<Json> {
    if bool_param(p, "list") == Some(true) {
        let d = s.doc()?;
        return Ok(json!({"names": go_to_names(&d.wb, d.wb.active_sheet()), "recent": d.go_to_recent}));
    }
    let Some(reference) = str_param(p, "reference").or_else(|| str_param(p, "range")) else { return Err(bad("edit.goTo", "missing `reference`")) };
    let d = s.doc()?;
    let mut target: Option<usize> = None;
    let mut areas: Vec<RangeRef> = Vec::new();
    let mut named = false;
    for part in reference_parts(reference) {
        let Some((sheet, found, by_name)) = go_to_target(&d.wb, d.wb.active_sheet(), part, 0) else {
            return Err(EngineError::Other("Reference isn't valid.".into()));
        };
        // One selection holds the areas of one sheet.
        if target.is_some_and(|t| t != sheet) {
            return Err(EngineError::Other("Reference isn't valid.".into()));
        }
        target = Some(sheet);
        named |= by_name;
        areas.extend(found);
        if areas.len() > GO_TO_MAX_AREAS {
            return Err(EngineError::Other(format!("Go To selects at most {GO_TO_MAX_AREAS} areas.")));
        }
    }
    let Some(sheet) = target else { return Err(EngineError::Other("Reference isn't valid.".into())) };
    let range = areas.iter().map(RangeRef::a1).collect::<Vec<_>>().join(",");
    let out = selection_set(s, &json!({"sheet": sheet, "range": range}))?;
    if bool_param(p, "record") == Some(true) && !named {
        let d = s.doc_mut()?;
        let prefix = gridcraft_core::quote_sheet(&d.wb.sheet(sheet).map(|sh| sh.name.clone()).unwrap_or_default());
        let entry = areas.iter().map(|r| format!("{prefix}!{}", r.a1())).collect::<Vec<_>>().join(",");
        d.go_to_recent.retain(|r| !r.eq_ignore_ascii_case(&entry));
        d.go_to_recent.insert(0, entry);
        d.go_to_recent.truncate(GO_TO_RECENT);
    }
    Ok(out)
}

/// The parts of a Go To reference, split at the commas outside quoted sheet names and
/// parentheses: `Price, Type` or `B14:C22,F19:G30`. The parts are read one at a time, so a caller
/// that stops at an invalid part or at `GO_TO_MAX_AREAS` does not split the rest of the text.
pub(super) fn reference_parts(text: &str) -> impl Iterator<Item = &str> {
    let (mut depth, mut quoted, mut start, mut done) = (0usize, false, 0, false);
    let mut chars = text.char_indices();
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        for (i, c) in chars.by_ref() {
            match c {
                '\'' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => depth = depth.saturating_sub(1),
                ',' if !quoted && depth == 0 => {
                    let part = text.get(start..i).unwrap_or("").trim();
                    start = i + 1;
                    return Some(part);
                }
                _ => {}
            }
        }
        done = true;
        Some(text.get(start..).unwrap_or("").trim())
    })
}

/// Where one part of a Go To reference points, seen from sheet `sheet`: the sheet, its areas,
/// and whether a defined name or a table gave them. `Sheet2!Name` is the name scoped to Sheet2.
/// A name counts when what it refers to is a reference; a constant or a formula is not one.
fn go_to_target(wb: &gridcraft_model::Workbook, sheet: usize, part: &str, depth: usize) -> Option<(usize, Vec<RangeRef>, bool)> {
    if part.is_empty() || depth > GO_TO_NAME_DEPTH {
        return None;
    }
    let (prefix, body) = split_sheet(part);
    let on = match &prefix {
        Some(n) => wb.sheet_index(n)?,
        None => sheet,
    };
    if let Some(r) = RangeRef::parse(&body.replace('$', "")) {
        return Some((on, vec![r], false));
    }
    let name = match prefix {
        Some(_) => wb.names.iter().find(|n| n.scope == Some(on) && gridcraft_formula::same_name(&n.name, body)),
        None => wb.name(body, sheet),
    };
    if let Some(n) = name {
        return name_areas(wb, n, sheet, depth).map(|(t, areas)| (t, areas, true));
    }
    if prefix.is_none()
        && let Some((si, ti)) = wb.table(body)
    {
        // As in Excel, a table's name stands for its data, without the header and totals rows.
        let t = wb.sheet(si)?.tables.get(ti)?;
        return Some((si, vec![t.data_range().unwrap_or(t.range)], true));
    }
    None
}

/// The sheet and areas defined name `n` refers to, seen from sheet `sheet`, when that is a
/// reference.
fn name_areas(wb: &gridcraft_model::Workbook, n: &gridcraft_model::DefinedName, sheet: usize, depth: usize) -> Option<(usize, Vec<RangeRef>)> {
    let from = n.scope.unwrap_or(sheet);
    let formula = n.formula.trim().trim_start_matches('=');
    let mut target: Option<usize> = None;
    let mut areas = Vec::new();
    for p in reference_parts(formula) {
        let (s, found, _) = go_to_target(wb, from, p, depth + 1)?;
        if target.is_some_and(|t| t != s) {
            return None;
        }
        target = Some(s);
        areas.extend(found);
        if areas.len() > GO_TO_MAX_AREAS {
            return None;
        }
    }
    target.map(|t| (t, areas))
}

/// The names the Go To dialog lists, seen from sheet `sheet`: the named ranges of the workbook and
/// of every sheet, in alphabetical order. A name scoped to another sheet is written `Sheet2!Name`,
/// and the active sheet's own name hides the workbook name it shares. Hidden names and names that
/// do not refer to a range are left out.
pub fn go_to_names(wb: &gridcraft_model::Workbook, sheet: usize) -> Vec<String> {
    let own: std::collections::HashSet<String> =
        wb.names.iter().filter(|n| n.scope == Some(sheet)).map(|n| gridcraft_formula::fold_name(&n.name)).collect();
    let mut out: Vec<String> = wb
        .names
        .iter()
        .filter(|n| !n.hidden && name_areas(wb, n, sheet, 0).is_some())
        .filter_map(|n| match n.scope {
            None if own.contains(&gridcraft_formula::fold_name(&n.name)) => None,
            None => Some(n.name.clone()),
            Some(s) if s == sheet => Some(n.name.clone()),
            Some(s) => Some(format!("{}!{}", gridcraft_core::quote_sheet(&wb.sheet(s)?.name), n.name)),
        })
        .collect();
    out.sort_by_cached_key(|n| gridcraft_formula::fold_name(n));
    out.dedup_by(|a, b| gridcraft_formula::same_name(a, b));
    out
}

/// The value types Go To Special's Constants and Formulas options filter by.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ValueTypes {
    numbers: bool,
    text: bool,
    logicals: bool,
    errors: bool,
}

impl ValueTypes {
    const ALL: ValueTypes = ValueTypes { numbers: true, text: true, logicals: true, errors: true };

    /// The `types` param: missing means every type, as Excel's dialog starts with every box
    /// ticked. An empty list or an unknown name is a usage error.
    fn param(p: &Json) -> Result<ValueTypes> {
        let Some(list) = p.get("types") else { return Ok(ValueTypes::ALL) };
        let Some(items) = list.as_array() else { return Err(bad("edit.goToSpecial", "`types` must be an array of numbers, text, logicals, errors")) };
        if items.is_empty() {
            return Err(bad("edit.goToSpecial", "`types` must name at least one of numbers, text, logicals, errors"));
        }
        let mut t = ValueTypes { numbers: false, text: false, logicals: false, errors: false };
        for item in items {
            match item.as_str().map(str::to_ascii_lowercase).as_deref() {
                Some("numbers") => t.numbers = true,
                Some("text") => t.text = true,
                Some("logicals") => t.logicals = true,
                Some("errors") => t.errors = true,
                _ => return Err(bad("edit.goToSpecial", format!("unknown type `{item}`; use numbers, text, logicals or errors"))),
            }
        }
        Ok(t)
    }

    /// Whether a value is of one of the chosen types. Dates are numbers; a formula returning `""`
    /// is text.
    fn matches(self, v: &gridcraft_core::Value) -> bool {
        match v {
            gridcraft_core::Value::Number(_) => self.numbers,
            gridcraft_core::Value::Text(_) => self.text,
            gridcraft_core::Value::Bool(_) => self.logicals,
            gridcraft_core::Value::Error(_) => self.errors,
            gridcraft_core::Value::Empty | gridcraft_core::Value::Array(_) => false,
        }
    }
}

/// A cell's formula in R1C1 form, relative to the cell itself, so that a formula and its relative
/// copies compare equal. `None` for a cell without a formula.
fn relative_formula(sheet: &gridcraft_model::Sheet, c: CellRef) -> Option<String> {
    let f = sheet.cell(c)?.formula.as_ref()?;
    let expr = match &f.expr {
        Some(e) => e.clone(),
        None => std::sync::Arc::new(gridcraft_formula::parse(&f.text).ok()?),
    };
    Some(gridcraft_formula::print_r1c1(&expr, c))
}

/// Whether two cells differ for Row/Column differences: formulas compare in relative R1C1 form,
/// anything else by value.
fn differs(sheet: &gridcraft_model::Sheet, a: CellRef, b: CellRef) -> bool {
    match (relative_formula(sheet, a), relative_formula(sheet, b)) {
        (Some(x), Some(y)) => x != y,
        (None, None) => sheet.value(a) != sheet.value(b),
        _ => true,
    }
}

/// The cells of `scope` that differ from the comparison cell of their row (or column): the cell
/// in the active cell's column (resp. row). The comparison cells themselves are left out.
fn differences(sheet: &gridcraft_model::Sheet, scope: RangeRef, active: CellRef, by_row: bool) -> Vec<CellRef> {
    let mut hits = Vec::new();
    for c in scope.iter() {
        let base = if by_row { CellRef::new(c.row, active.col) } else { CellRef::new(active.row, c.col) };
        if c != base && differs(sheet, c, base) {
            hits.push(c);
        }
    }
    hits
}

pub(super) fn go_to_special(s: &mut Session, p: &Json) -> Result<Json> {
    let kind = str_param(p, "kind").unwrap_or("blanks");
    let d = s.doc()?;
    let Some(sheet) = d.wb.active() else { return Err(EngineError::NoDocument) };
    let scope = if d.selection.is_single_cell() { sheet.used_range().unwrap_or(RangeRef::cell(d.selection.active)) } else { d.selection.bounds() };
    if kind == "lastCell" {
        let c = sheet.used_range().map(|r| r.end).unwrap_or_default();
        return selection_set(s, &json!({"cell": c.a1()}));
    }
    if kind == "currentRegion" {
        return select_region(s, &json!({}));
    }
    let types = match kind {
        "constants" | "formulas" => ValueTypes::param(p)?,
        _ => ValueTypes::ALL,
    };
    let active = d.selection.active;
    let same = bool_param(p, "same") == Some(true);
    if same && (kind == "dataValidation" || kind == "conditionalFormats") {
        // Excel looks at the whole sheet for "Same": the rules that hold the active cell. Their
        // ranges become the selection's areas as they are, so whole columns are not enumerated.
        let mut ranges: Vec<RangeRef> = Vec::new();
        let mut add = |rs: &[RangeRef]| {
            for r in rs {
                if !ranges.contains(r) {
                    ranges.push(*r);
                }
            }
        };
        if kind == "dataValidation" {
            if let Some(dv) = sheet.validations.iter().find(|dv| dv.ranges.iter().any(|r| r.contains(active))) {
                add(&dv.ranges);
            }
        } else {
            for cf in sheet.cond_formats.iter().filter(|cf| cf.ranges.iter().any(|r| r.contains(active))) {
                add(&cf.ranges);
            }
        }
        return select_rule_ranges(s, ranges);
    }
    let mut hits: Vec<CellRef> = Vec::new();
    if scope.count() > 4_000_000 {
        return Err(bad("edit.goToSpecial", "range too large"));
    }
    if kind == "rowDifferences" || kind == "columnDifferences" {
        if !d.selection.is_single_cell() {
            hits = differences(sheet, scope, active, kind == "rowDifferences");
        }
    } else {
        for c in scope.iter() {
            let cell = sheet.cell(c);
            let v = sheet.value(c);
            let ok = match kind {
                "blanks" => v.is_empty(),
                "constants" => cell.is_some_and(|x| x.formula.is_none() && x.py.is_none() && !x.value.is_empty()) && types.matches(&v),
                "formulas" => cell.is_some_and(|x| x.formula.is_some() || x.py.is_some()) && types.matches(&v),
                "comments" | "notes" => sheet.comments.contains_key(&c),
                "errors" => v.is_error(),
                "numbers" => v.is_number(),
                "text" => v.is_text(),
                "visible" => !sheet.is_row_hidden(c.row) && !sheet.is_col_hidden(c.col),
                "conditionalFormats" => sheet.cond_formats.iter().any(|cf| cf.ranges.iter().any(|r| r.contains(c))),
                "dataValidation" => sheet.validations.iter().any(|dv| dv.ranges.iter().any(|r| r.contains(c))),
                _ => return Err(bad("edit.goToSpecial", format!("unknown kind `{kind}`"))),
            };
            if ok {
                hits.push(c);
            }
        }
    }
    if hits.is_empty() {
        return Err(EngineError::Other("No cells were found.".into()));
    }
    let ranges = coalesce(&hits);
    let d = s.doc_mut()?;
    d.selection = crate::Selection { active: hits[0], anchor: hits[0], ranges };
    Ok(json!({"selection": d.selection.a1(), "count": hits.len()}))
}

/// Selects a rule's ranges as areas, the active cell at the top-left of the first one. No range
/// means the active cell is under no rule.
fn select_rule_ranges(s: &mut Session, ranges: Vec<RangeRef>) -> Result<Json> {
    let Some(first) = ranges.first() else { return Err(EngineError::Other("No cells were found.".into())) };
    let start = first.start;
    let count = ranges.iter().fold(0u64, |n, r| n.saturating_add(r.count()));
    let d = s.doc_mut()?;
    d.selection = crate::Selection { active: start, anchor: start, ranges };
    Ok(json!({"selection": d.selection.a1(), "count": count}))
}
