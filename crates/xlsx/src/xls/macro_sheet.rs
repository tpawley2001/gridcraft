//! Excel 4.0 macro sheets ([MS-XLS] 2.4.28 BoundSheet8 `dt = 1`, BOF `dt = 0x0040`): read with the
//! worksheet reader into a detached sheet and kept as an `xl/macrosheets/sheetN.xml` part
//! ([MS-XLSX] 2.1.11 `CT_Macrosheet`, with the CT_Worksheet children), so a macro-enabled save
//! writes them back in place. They are never evaluated or shown.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_model::{Cell, LineInfo, PreservedPart, PreservedRel, PreservedSheet, Sheet, Visibility, Workbook};

use crate::tables::{px_to_col_width, px_to_pt};
use crate::xml::{encode_xstring, esc, esc_attr, num, push_a1};

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";
const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_MACRO_SHEET: &str = "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet";
const CT_MACRO_SHEET: &str = "application/vnd.ms-excel.macrosheet+xml";
const REL_INTL_MACRO_SHEET: &str = "http://schemas.microsoft.com/office/2006/relationships/xlIntlMacrosheet";
const CT_INTL_MACRO_SHEET: &str = "application/vnd.ms-excel.intlmacrosheet+xml";

/// The warning for a macro sheet formula that couldn't be read (its saved value is kept).
pub(crate) const UNREAD_FORMULAS: &str = "Some Excel 4.0 macro formulas could not be read and were kept as values.";

/// A macro sheet read from the file.
pub(crate) struct MacroSheet {
    pub name: String,
    pub visibility: Visibility,
    /// How many worksheets come before it.
    pub before: usize,
    /// Its BoundSheet8 position, for the order among the other kept sheets.
    pub bound: usize,
    /// An international macro sheet (`xlIntlMacrosheet`): same part, its own content and
    /// relationship types.
    pub intl: bool,
    pub sheet: Sheet,
}

/// The note a kept macro sheet gets, worded like the `.xlsx` reader's.
pub(crate) fn kept_warning(name: &str) -> String {
    format!("macro sheet \"{name}\" is kept as it is but can't be shown or edited; kept only when saved as a macro-enabled workbook")
}

/// Adds the macro sheets to the workbook's kept parts. `chart_bounds` gives the BoundSheet8
/// position of each kept chart sheet already there (in order), so every kept sheet stays in
/// file order.
pub(crate) fn add_macro_sheets(wb: &mut Workbook, sheets: Vec<MacroSheet>, chart_bounds: &[usize]) {
    if sheets.is_empty() {
        return;
    }
    let mut kept = wb.preserved.as_deref().cloned().unwrap_or_default();
    let mut order: Vec<(usize, PreservedSheet)> =
        std::mem::take(&mut kept.sheets).into_iter().enumerate().map(|(i, s)| (chart_bounds.get(i).copied().unwrap_or(usize::MAX), s)).collect();
    for (i, ms) in sheets.into_iter().enumerate() {
        let n = i + 1;
        let (content_type, rel_type) = if ms.intl { (CT_INTL_MACRO_SHEET, REL_INTL_MACRO_SHEET) } else { (CT_MACRO_SHEET, REL_MACRO_SHEET) };
        let part = format!("xl/macrosheets/sheet{n}.xml");
        kept.parts.push(PreservedPart {
            name: part.clone(),
            content_type: content_type.into(),
            data: Arc::from(macro_sheet_xml(&ms.sheet).into_bytes()),
            rels: vec![],
        });
        let rel_id = format!("rIdXlsMacroSheet{n}");
        kept.rels.push(PreservedRel { id: rel_id.clone(), rel_type: rel_type.into(), target: part, external: false });
        order.push((ms.bound, PreservedSheet { name: ms.name, visibility: ms.visibility, rel_id, before: ms.before, sheet_id: 0 }));
    }
    order.sort_by_key(|(bound, _)| *bound);
    kept.sheets = order.into_iter().map(|(_, s)| s).collect();
    wb.preserved = Some(Arc::new(kept));
}

/// A row's layout and its cells.
type Row<'a> = (LineInfo, Vec<(CellRef, &'a Cell)>);

/// The `<xm:macrosheet>` part of a detached sheet: dimension, column widths and the cells (with
/// row heights). Cell styles are the workbook's style indices, as in its worksheets.
pub(crate) fn macro_sheet_xml(sheet: &Sheet) -> String {
    let mut s = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xm:macrosheet xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xmlns:xm=\"{NS_XM}\">"
    );
    if let Some(used) = sheet.cells.used_range() {
        let _ = write!(s, "<dimension ref=\"{}\"/>", used.a1());
    }
    s.push_str("<sheetViews><sheetView workbookViewId=\"0\"/></sheetViews>");
    let _ = write!(s, "<sheetFormatPr defaultRowHeight=\"{}\"/>", num(px_to_pt(sheet.default_row_height)));
    let widths: Vec<(u32, f32, bool)> = sheet.cols.iter().filter_map(|(c, info)| info.size.map(|w| (*c, w, info.hidden))).collect();
    if !widths.is_empty() {
        s.push_str("<cols>");
        for (c, w, hidden) in widths {
            let n = u64::from(c) + 1;
            let _ = write!(
                s,
                "<col min=\"{n}\" max=\"{n}\" width=\"{}\" customWidth=\"1\"{}/>",
                num(px_to_col_width(w)),
                if hidden { " hidden=\"1\"" } else { "" }
            );
        }
        s.push_str("</cols>");
    }
    let mut rows: BTreeMap<u32, Row<'_>> = BTreeMap::new();
    for (r, info) in &sheet.rows {
        rows.entry(*r).or_default().0 = *info;
    }
    for (at, cell) in sheet.cells.iter() {
        rows.entry(at.row).or_default().1.push((at, cell));
    }
    s.push_str("<sheetData>");
    for (r, (info, mut cells)) in rows {
        if cells.is_empty() && info.size.is_none() && !info.hidden {
            continue;
        }
        cells.sort_by_key(|(at, _)| at.col);
        let _ = write!(s, "<row r=\"{}\"", u64::from(r) + 1);
        if let Some(h) = info.size {
            let _ = write!(s, " ht=\"{}\" customHeight=\"1\"", num(px_to_pt(h)));
        }
        if info.hidden {
            s.push_str(" hidden=\"1\"");
        }
        s.push('>');
        for (at, cell) in cells {
            write_cell(&mut s, at, cell);
        }
        s.push_str("</row>");
    }
    s.push_str("</sheetData><pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/></xm:macrosheet>");
    s
}

fn write_cell(s: &mut String, at: CellRef, cell: &Cell) {
    s.push_str("<c r=\"");
    push_a1(s, at);
    s.push('"');
    if cell.style.0 != 0 {
        let _ = write!(s, " s=\"{}\"", cell.style.0);
    }
    let value = match cell.value.scalar() {
        Value::Array(_) => Value::Empty,
        Value::Error(CellError::Spill | CellError::Calc | CellError::Circ) => Value::Error(CellError::Value),
        v => v,
    };
    let formula = cell.formula.as_deref();
    let t = match (&value, formula.is_some()) {
        (Value::Text(_), true) => " t=\"str\"",
        (Value::Text(_), false) => " t=\"inlineStr\"",
        (Value::Bool(_), _) => " t=\"b\"",
        (Value::Error(_), _) => " t=\"e\"",
        _ => "",
    };
    if formula.is_none() && matches!(value, Value::Empty) {
        s.push_str("/>");
        return;
    }
    s.push_str(t);
    s.push('>');
    if let Some(f) = formula {
        let text = esc(&crate::fmla::to_file(f));
        match f.array {
            Some(r) => {
                let _ = write!(s, "<f t=\"array\" ref=\"{}\">{text}</f>", esc_attr(&r.a1()));
            }
            None => {
                let _ = write!(s, "<f>{text}</f>");
            }
        }
    }
    match &value {
        Value::Number(n) => {
            let _ = write!(s, "<v>{}</v>", num(*n));
        }
        Value::Text(t) if formula.is_some() => {
            let _ = write!(s, "<v>{}</v>", esc(&encode_xstring(t)));
        }
        Value::Text(t) => {
            let _ = write!(s, "<is><t xml:space=\"preserve\">{}</t></is>", esc(&encode_xstring(t)));
        }
        Value::Bool(b) => {
            let _ = write!(s, "<v>{}</v>", u8::from(*b));
        }
        Value::Error(e) => {
            let _ = write!(s, "<v>{}</v>", e.as_str());
        }
        Value::Empty | Value::Array(_) => {}
    }
    s.push_str("</c>");
}
