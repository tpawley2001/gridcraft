//! Shared workbook revision history (ECMA-376 Part 1 §18.11): the revision headers part, the
//! revision logs it names and the user names part are kept byte for byte, never applied.
//!
//! A revision header's `<sheetIdMap>` and the logs' `sId` attributes name sheets by `sheetId`, so
//! the history is only written back while the workbook still has the sheets it was read with: the
//! same `sheetId`s and names in the same order. When sheets are added, deleted, moved or renamed
//! the whole revision graph (headers and logs) is dropped on save, and the user is told so on open.

use super::*;

/// The workbook relationship kind of the revision headers part.
pub(super) const REVISION_HEADERS: &str = "revisionHeaders";
/// The kept-element slot of the sheets the revision history was read with.
pub(super) const SHEETS_SLOT: &str = "revisionSheets";

/// Checks that every `<header>` of the revision headers part behind `root` names a revision log
/// through one of the part's own relationships, so a kept header never points at nothing.
pub(super) fn check_headers(cx: &mut Ctx<'_>, root: &PreservedRel) -> Result<(), String> {
    if root.external {
        return Err("it is outside the package".into());
    }
    let data = match cx.pkg.read_limited(&root.target, MAX_KEPT_PART) {
        Ok(Some(d)) => d,
        Ok(None) => return Err(format!("{} is missing", root.target)),
        Err(e) => return Err(e.to_string()),
    };
    let el = xml::parse(&data).map_err(|e| format!("{} could not be read: {e}", root.target))?;
    if el.name != "headers" {
        return Err(format!("{} is not a revision headers part", root.target));
    }
    let rels = raw_rels(cx, &root.target)?;
    for h in el.kids("header") {
        let id = h.attr("id").ok_or("a revision header names no revision log")?;
        if !rels.iter().any(|r| r.id == id && !r.external && is_kind(r, &["revisionLog"])) {
            return Err(format!("revision log {id} is missing"));
        }
    }
    Ok(())
}

/// The kept element recording the worksheets (in order: `sheetId`, name) the revision history
/// was read with; `file_to_model[i]` says whether the workbook's `i`th `<sheet>` is a worksheet.
pub(super) fn sheets_snapshot(wb_xml: &xml::El, file_to_model: &[Option<usize>]) -> PreservedXml {
    let mut x = String::from("<sheets>");
    if let Some(sheets) = wb_xml.child("sheets") {
        for (se, _) in sheets.kids("sheet").zip(file_to_model).filter(|(_, m)| m.is_some()) {
            let _ = write!(x, "<sheet sheetId=\"{}\" name=\"{}\"/>", se.attr_u32("sheetId").unwrap_or(0), esc_attr(se.attr("name").unwrap_or("")));
        }
    }
    x.push_str("</sheets>");
    PreservedXml { slot: SHEETS_SLOT.into(), xml: x }
}

/// Whether the revision history kept in `p` still matches the workbook's sheets: the same
/// worksheets, `sheetId`s (non-zero and distinct, so they are written unchanged) and names, in the
/// same order, and every kept chart, dialog or macro sheet written back (`kept_sheets_live`).
fn current(wb: &Workbook, p: &Preserved, kept_sheets_live: bool) -> bool {
    if !kept_sheets_live {
        return false;
    }
    let Some(el) = p.elements.iter().find(|e| e.slot == SHEETS_SLOT).and_then(|e| xml::parse(e.xml.as_bytes()).ok()) else { return false };
    let then: Vec<(u32, &str)> = el.kids("sheet").map(|s| (s.attr_u32("sheetId").unwrap_or(0), s.attr("name").unwrap_or(""))).collect();
    let mut ids = HashSet::new();
    then.len() == wb.sheets.len()
        && then.iter().zip(&wb.sheets).all(|(&(id, name), s)| id != 0 && ids.insert(id) && s.file_id == id && s.name == name)
}

/// The kept revision headers relationships of `p` that can't be written back because the sheets
/// changed (see [`current`]).
pub(super) fn stale_roots<'a>(wb: &Workbook, p: &'a Preserved, kept_sheets_live: bool) -> Vec<&'a PreservedRel> {
    let roots: Vec<&PreservedRel> = p.rels.iter().filter(|r| is_kind(r, &[REVISION_HEADERS])).collect();
    if roots.is_empty() || current(wb, p, kept_sheets_live) { vec![] } else { roots }
}
