//! XML Spreadsheet 2003 (SpreadsheetML 2003, one `.xml` document with a `<Workbook>` root in
//! `urn:schemas-microsoft-com:office:spreadsheet`) import: values, R1C1 formulas, styles, merges,
//! column widths and row heights, notes, hyperlinks, defined names and sheet view options.
//! Import only. Format reference: Microsoft's published "XML Spreadsheet Reference" (Office XP).

mod style;

use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_model::{Cell, Comment, DefinedName, Formula, Hyperlink, LineInfo, Sheet, StyleId, Visibility, Workbook};
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::xml::El;
use crate::{IoError, ReadReport};

const SS: &[u8] = b"urn:schemas-microsoft-com:office:spreadsheet";
/// How far into the file [`is_spreadsheetml`] looks for the root element.
const SNIFF_BYTES: usize = 64 * 1024;
/// Most column / row entries one file may set through `ss:Span`.
const MAX_LINE_ENTRIES: usize = 1_000_000;
/// Widest column the model keeps (255 characters), in px.
const MAX_COL_PX: f64 = 1790.0;

/// True when the document's root element is `Workbook` in the SpreadsheetML 2003 namespace
/// (default or prefixed), after an optional BOM, the XML declaration, comments and processing
/// instructions. Only the first 64 KiB are looked at.
pub fn is_spreadsheetml(bytes: &[u8]) -> bool {
    let head = bytes.get(..bytes.len().min(SNIFF_BYTES)).unwrap_or(bytes);
    let data = crate::xml::to_utf8(head);
    let mut reader = NsReader::from_reader(data.as_ref());
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let (ns, local) = reader.resolver().resolve_element(e.name());
                return matches!(ns, ResolveResult::Bound(n) if n.as_ref() == SS) && local.as_ref() == b"Workbook";
            }
            Ok(Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_)) => {}
            Ok(Event::Text(t)) if t.iter().all(u8::is_ascii_whitespace) => {}
            _ => return false,
        }
    }
}

/// Reads an XML Spreadsheet 2003 workbook. Malformed XML or another root element is an error;
/// unsupported content is skipped and reported in the warnings.
pub fn read_spreadsheetml(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    if bytes.len() > crate::csv::MAX_CSV_BYTES {
        return Err(IoError::TooLarge("XML Spreadsheet exceeds 512 MiB".into()));
    }
    if !is_spreadsheetml(bytes) {
        return Err(IoError::Format("not an XML Spreadsheet 2003 workbook".into()));
    }
    let root = crate::xml::parse(bytes)?;
    let mut wb = Workbook::new();
    wb.sheets.clear();
    if root.path(&["ExcelWorkbook", "Date1904"]).is_some() {
        wb.date_system = DateSystem::D1904;
    }
    let styles = style::Styles::read(root.child("Styles"));
    let mut import = Import { report: ReadReport::default(), styles, bad_formulas: 0, bad_names: false };
    let mut selected = None;
    let mut sheet_names = Vec::new();
    for (i, ws) in root.kids("Worksheet").enumerate() {
        let original = ws.attr("Name").unwrap_or_default();
        let mut name = crate::read::SheetNames::of(&wb).unique(original, i);
        if wb.check_sheet_name(&name, None).is_err() {
            name = wb.next_sheet_name();
        }
        if name != original {
            import.report.warn("Some sheet names were shortened or renamed to valid, unique sheet names.");
        }
        let mut sheet = Sheet::new(name);
        if let Some(table) = ws.child("Table") {
            import.table(&mut sheet, table, wb.date_system);
        }
        if let Some(options) = ws.child("WorksheetOptions") {
            if options.child("Selected").is_some() && selected.is_none() {
                selected = Some(i);
            }
            options_into(&mut sheet, options);
        }
        sheet_names.push((ws, wb.sheets.len()));
        wb.sheets.push(Arc::new(sheet));
    }
    if wb.sheets.is_empty() {
        return Err(IoError::Format("the XML Spreadsheet has no worksheets".into()));
    }
    let active = selected.or_else(|| root.path(&["ExcelWorkbook", "ActiveSheet"]).and_then(|e| e.text.trim().parse::<usize>().ok()));
    if let Some(i) = active.filter(|i| *i < wb.sheets.len()) {
        wb.active_sheet.set(i);
    }
    wb.styles = std::mem::take(&mut import.styles.table);
    if let Some(names) = root.child("Names") {
        import.names(&mut wb, names, None);
    }
    for (ws, index) in sheet_names {
        if let Some(names) = ws.child("Names") {
            import.names(&mut wb, names, Some(index));
        }
    }
    if import.bad_formulas > 0 {
        let n = import.bad_formulas;
        import.report.warn(format!("{n} formula(s) couldn't be read and were kept as their saved values."));
    }
    if import.bad_names {
        import.report.warn("Some defined names couldn't be read and were skipped.");
    }
    Ok((wb, import.report))
}

struct Import {
    report: ReadReport,
    styles: style::Styles,
    bad_formulas: usize,
    bad_names: bool,
}

/// Where an element with an optional 1-based `ss:Index` goes: `next` when it has none. `None`
/// (with a warning) when the index doesn't move forward.
fn position(e: &El, next: u64, report: &mut ReadReport) -> Option<u64> {
    match e.attr("Index") {
        None => Some(next),
        Some(i) => match i.trim().parse::<u64>().ok().and_then(|i| i.checked_sub(1)) {
            Some(i) if i >= next => Some(i),
            _ => {
                report.warn("Some rows, columns or cells whose ss:Index doesn't move forward were skipped.");
                None
            }
        },
    }
}

fn beyond_grid(report: &mut ReadReport) {
    report.warn("Cells, rows or columns beyond the sheet's last row or column were dropped.");
}

impl Import {
    fn table(&mut self, sheet: &mut Sheet, table: &El, sys: DateSystem) {
        if let Some(w) = table.attr_f64("DefaultColumnWidth").filter(|w| *w > 0.0) {
            sheet.default_col_width = (w * 4.0 / 3.0).min(MAX_COL_PX) as f32;
        }
        if let Some(h) = table.attr_f64("DefaultRowHeight").filter(|h| *h > 0.0) {
            sheet.default_row_height = crate::tables::pt_to_px(h);
        }
        let mut entries = 0usize;
        let mut next_col = 0u64;
        for col in table.kids("Column") {
            let Some(c) = position(col, next_col, &mut self.report) else { continue };
            let span = u64::from(col.attr_u32("Span").unwrap_or(0));
            next_col = c.saturating_add(span).saturating_add(1);
            let info = LineInfo {
                size: col.attr_f64("Width").filter(|w| *w >= 0.0).map(|w| (w * 4.0 / 3.0).min(MAX_COL_PX) as f32),
                hidden: col.flag("Hidden", false),
                style: self.styles.id(col),
                ..LineInfo::default()
            };
            if next_col > u64::from(MAX_COLS) {
                beyond_grid(&mut self.report);
            }
            for c in c..next_col.min(u64::from(MAX_COLS)) {
                entries += 1;
                if entries > MAX_LINE_ENTRIES {
                    break;
                }
                sheet.cols.insert(c as u32, info);
            }
        }
        let mut next_row = 0u64;
        for row in table.kids("Row") {
            let Some(r) = position(row, next_row, &mut self.report) else { continue };
            let span = u64::from(row.attr_u32("Span").unwrap_or(0));
            next_row = r.saturating_add(span).saturating_add(1);
            if r >= u64::from(MAX_ROWS) {
                beyond_grid(&mut self.report);
                continue;
            }
            let row_style = self.styles.id(row);
            let height = row.attr_f64("Height").filter(|h| *h >= 0.0);
            let hidden = row.flag("Hidden", false);
            if height.is_some() || hidden || row_style.is_some() {
                let info =
                    LineInfo { size: height.map(crate::tables::pt_to_px), hidden, style: row_style, custom: height.is_some(), ..LineInfo::default() };
                for r in r..next_row.min(u64::from(MAX_ROWS)) {
                    entries += 1;
                    if entries > MAX_LINE_ENTRIES {
                        break;
                    }
                    sheet.rows.insert(r as u32, info);
                }
            }
            self.row(sheet, row, r as u32, row_style, sys);
        }
    }

    fn row(&mut self, sheet: &mut Sheet, row: &El, r: u32, row_style: Option<StyleId>, sys: DateSystem) {
        let mut next = 0u64;
        for cell in row.kids("Cell") {
            let Some(c) = position(cell, next, &mut self.report) else { continue };
            let across = u64::from(cell.attr_u32("MergeAcross").unwrap_or(0));
            let down = cell.attr_u32("MergeDown").unwrap_or(0);
            next = c.saturating_add(across).saturating_add(1);
            if c >= u64::from(MAX_COLS) {
                beyond_grid(&mut self.report);
                continue;
            }
            let at = CellRef::new(r, c as u32);
            if across > 0 || down > 0 {
                let last_col = c.saturating_add(across).min(u64::from(MAX_COLS) - 1) as u32;
                let last_row = r.saturating_add(down).min(MAX_ROWS - 1);
                if u64::from(last_col) < c.saturating_add(across) || u64::from(last_row) < u64::from(r).saturating_add(u64::from(down)) {
                    beyond_grid(&mut self.report);
                }
                sheet.merges.push(RangeRef::new(at, CellRef::new(last_row, last_col)));
            }
            let style = self.styles.id(cell).or(row_style).or_else(|| sheet.cols.get(&(c as u32)).and_then(|i| i.style)).unwrap_or(StyleId::DEFAULT);
            let data = cell.child("Data");
            let value = data.map(|d| value(d, sys)).unwrap_or(Value::Empty);
            let formula = cell.attr("Formula").and_then(|f| {
                let text = f.strip_prefix('=').unwrap_or(f);
                let parsed = crate::sylk::r1c1_to_a1(text, at).map(|a1| Formula::new(&a1)).filter(|f| f.expr.is_some());
                if parsed.is_none() {
                    self.bad_formulas += 1;
                }
                parsed.map(Arc::new)
            });
            if formula.is_some() || !matches!(value, Value::Empty) {
                sheet.cells.set(at, Cell { formula, style, ..Cell::value(value) });
            } else if style != StyleId::DEFAULT {
                sheet.set_style(at, style);
            }
            if let Some(target) = cell.attr("HRef").filter(|t| !t.is_empty()) {
                let (target, external) = match target.strip_prefix('#') {
                    Some(location) => (location, false),
                    None => (target, true),
                };
                let link = Hyperlink { target: target.to_string(), tooltip: cell.attr("HRefScreenTip").map(str::to_string), external };
                sheet.hyperlinks.insert(at, link);
            }
            if let Some(note) = cell.child("Comment") {
                let text = note.child("Data").map(text_content).unwrap_or_default();
                let author = note.attr("Author").unwrap_or_default().to_string();
                let visible = note.flag("ShowAlways", false);
                let comment = Comment {
                    author,
                    text,
                    replies: vec![],
                    threaded: false,
                    resolved: false,
                    visible,
                    dates: vec![],
                    note_box: None,
                    picture_fill: None,
                };
                sheet.comments.insert(at, comment);
            }
        }
    }

    fn names(&mut self, wb: &mut Workbook, names: &El, scope: Option<usize>) {
        for n in names.kids("NamedRange") {
            let Some(name) = n.attr("Name").map(str::trim).filter(|s| !s.is_empty()) else {
                self.bad_names = true;
                continue;
            };
            if name.eq_ignore_ascii_case("_FilterDatabase") {
                continue;
            }
            let refers = n.attr("RefersTo").unwrap_or_default();
            let formula = crate::sylk::r1c1_to_a1(refers.strip_prefix('=').unwrap_or(refers), CellRef::new(0, 0))
                .filter(|f| !f.is_empty() && gridcraft_formula::parse(f).is_ok());
            let duplicate = wb.names.iter().any(|d| d.scope == scope && d.name.eq_ignore_ascii_case(name));
            match formula {
                Some(formula) if !duplicate => {
                    wb.names.push(DefinedName { name: name.to_string(), scope, formula, comment: String::new(), hidden: n.flag("Hidden", false) })
                }
                _ => self.bad_names = true,
            }
        }
    }
}

/// `WorksheetOptions`: frozen panes and sheet visibility.
fn options_into(sheet: &mut Sheet, options: &El) {
    if options.child("FreezePanes").is_some() {
        let split = |name: &str| options.child(name).and_then(|e| e.text.trim().parse::<u32>().ok()).unwrap_or(0);
        let (rows, cols) = (split("SplitHorizontal").min(MAX_ROWS - 1), split("SplitVertical").min(MAX_COLS - 1));
        if rows > 0 || cols > 0 {
            sheet.freeze = Some((rows, cols));
        }
    }
    match options.child("Visible").map(|e| e.text.trim()) {
        Some("SheetHidden") => sheet.visibility = Visibility::Hidden,
        Some("SheetVeryHidden") => sheet.visibility = Visibility::VeryHidden,
        _ => {}
    }
}

/// All the text inside an element, its children's (rich text runs) included. The tree keeps an
/// element's own text apart from its children, so text between runs comes after them; Excel
/// wraps every run of rich text in an element, so its files never mix the two.
fn text_content(e: &El) -> String {
    let mut out = String::new();
    for child in &e.children {
        out.push_str(&text_content(child));
    }
    out.push_str(&e.text);
    out
}

/// A `Data` element's value by its `ss:Type`; what doesn't read as its type stays text.
fn value(data: &El, sys: DateSystem) -> Value {
    let text = text_content(data);
    let typed = match data.attr("Type") {
        Some("Number") => text.trim().parse::<f64>().ok().filter(|n| n.is_finite()).map(Value::Number),
        Some("Boolean") => match text.trim() {
            "1" => Some(Value::Bool(true)),
            "0" => Some(Value::Bool(false)),
            _ => None,
        },
        Some("DateTime") => style::datetime(&text, sys).map(Value::Number),
        Some("Error") => match text.trim() {
            e @ ("#NULL!" | "#DIV/0!" | "#VALUE!" | "#REF!" | "#NAME?" | "#NUM!" | "#N/A") => CellError::parse(e).map(Value::Error),
            _ => None,
        },
        _ => None,
    };
    match typed {
        Some(v) => v,
        None if text.is_empty() => Value::Empty,
        None => Value::Text(Arc::from(text.as_str())),
    }
}
