//! A table's saved sort settings: `<sortState>` (ECMA-376 Part 1 §18.3.1.92) with its ordered
//! `<sortCondition>`s (§18.3.1.91), directly under `<table>` or inside its `<autoFilter>`.

use std::fmt::Write as _;

use gridcraft_core::RangeRef;
use gridcraft_model::{MAX_CUSTOM_LIST_CHARS, MAX_SORT_CONDITIONS, SortBy, SortCondition, Table, TableSort};

use crate::read::Ctx;
use crate::styles::dxf_id;
use crate::write::Out;
use crate::xml::{El, esc_attr};

/// Reads the sort state of the table element `x` whose range is `table_range`. A `<sortState>`
/// directly under `<table>` wins over one nested in its `<autoFilter>`.
pub(crate) fn read_table_sort(cx: &mut Ctx<'_>, x: &El, table_range: RangeRef) -> Option<TableSort> {
    let (e, in_autofilter) = match x.child("sortState") {
        Some(e) => (e, false),
        None => (x.child("autoFilter")?.child("sortState")?, true),
    };
    read_sort_state(cx, e, table_range, in_autofilter)
}

/// Reads the `<sortState>` element `e` whose owner (table, sheet AutoFilter or the sort state
/// itself) covers `owner_range`; `in_autofilter` says whether it sat inside an `<autoFilter>`.
pub(crate) fn read_sort_state(cx: &mut Ctx<'_>, e: &El, owner_range: RangeRef, in_autofilter: bool) -> Option<TableSort> {
    let Some(range) = e.attr("ref").and_then(RangeRef::parse) else {
        cx.warn("a table's sort settings have no valid range and were dropped");
        return None;
    };
    let mut conditions = Vec::new();
    let mut dropped_color = false;
    for c in e.kids("sortCondition") {
        if conditions.len() >= MAX_SORT_CONDITIONS {
            cx.warn(format!("a table's sort keys past the first {MAX_SORT_CONDITIONS} were dropped"));
            break;
        }
        let Some(cond_range) = c.attr("ref").and_then(RangeRef::parse) else { continue };
        let dxf = |cx: &Ctx<'_>| c.attr_u32("dxfId").map(|i| cx.dxfs.get(i as usize).cloned());
        let by = match c.attr("sortBy").unwrap_or("value") {
            "cellColor" | "fontColor" => {
                let style = match dxf(cx) {
                    // A dxfId outside `<dxfs>` can't be remapped on save.
                    Some(None) => {
                        dropped_color = true;
                        continue;
                    }
                    Some(Some(s)) => Some(s),
                    None => None,
                };
                if c.attr("sortBy") == Some("cellColor") { SortBy::CellColor(style) } else { SortBy::FontColor(style) }
            }
            "icon" => SortBy::Icon { set: c.attr("iconSet").filter(|s| s.len() <= 32).map(str::to_string), id: c.attr_u32("iconId") },
            _ => SortBy::Value,
        };
        let custom_list = c.attr("customList").map(|l| l.chars().take(MAX_CUSTOM_LIST_CHARS).collect());
        conditions.push(SortCondition { range: cond_range, descending: c.flag("descending", false), by, custom_list });
    }
    if dropped_color {
        cx.warn("a table's colour sort key pointing at a missing format was dropped");
    }
    Some(TableSort {
        range,
        table_range: owner_range,
        in_autofilter,
        column_sort: e.flag("columnSort", false),
        case_sensitive: e.flag("caseSensitive", false),
        method: e.attr("sortMethod").filter(|m| matches!(*m, "stroke" | "pinYin")).map(str::to_string),
        conditions,
    })
}

/// The sort state to write for `t`: none once the table has been resized since it was read.
pub(crate) fn current_sort(t: &Table) -> Option<&TableSort> {
    t.sort.as_ref().filter(|s| s.table_range == t.range)
}

/// `<sortState>` XML for `s`; colour keys get their format registered in the output's `<dxfs>`.
pub(crate) fn sort_state_xml(s: &TableSort, out: &mut Out) -> String {
    let mut x = String::from("<sortState");
    if s.column_sort {
        x.push_str(" columnSort=\"1\"");
    }
    if s.case_sensitive {
        x.push_str(" caseSensitive=\"1\"");
    }
    if let Some(m) = &s.method {
        let _ = write!(x, " sortMethod=\"{}\"", esc_attr(m));
    }
    let _ = write!(x, " ref=\"{}\"", s.range.a1());
    if s.conditions.is_empty() {
        x.push_str("/>");
        return x;
    }
    x.push('>');
    for c in &s.conditions {
        x.push_str("<sortCondition");
        if c.descending {
            x.push_str(" descending=\"1\"");
        }
        match &c.by {
            SortBy::Value => {}
            SortBy::CellColor(st) | SortBy::FontColor(st) => {
                let by = if matches!(c.by, SortBy::CellColor(_)) { "cellColor" } else { "fontColor" };
                let _ = write!(x, " sortBy=\"{by}\"");
                if let Some(st) = st {
                    let _ = write!(x, " dxfId=\"{}\"", dxf_id(&mut out.dxfs, st));
                }
            }
            SortBy::Icon { set, id } => {
                x.push_str(" sortBy=\"icon\"");
                if let Some(set) = set {
                    let _ = write!(x, " iconSet=\"{}\"", esc_attr(set));
                }
                if let Some(id) = id {
                    let _ = write!(x, " iconId=\"{id}\"");
                }
            }
        }
        let _ = write!(x, " ref=\"{}\"", c.range.a1());
        if let Some(l) = &c.custom_list {
            let _ = write!(x, " customList=\"{}\"", esc_attr(l));
        }
        x.push_str("/>");
    }
    x.push_str("</sortState>");
    x
}

/// Puts `sort` inside the `<autoFilter>` element `af` (after its filter columns).
pub(crate) fn nest_in_autofilter(af: &str, sort: &str) -> String {
    if let Some(head) = af.strip_suffix("/>") {
        format!("{head}>{sort}</autoFilter>")
    } else if let Some(head) = af.strip_suffix("</autoFilter>") {
        format!("{head}{sort}</autoFilter>")
    } else {
        af.to_string()
    }
}
