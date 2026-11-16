//! Links to other workbooks read from the binary formats (`.xlsb`, `.xls`), kept as the
//! SpreadsheetML parts the `.xlsx` reader keeps for them (ECMA-376 Part 1 §18.14 `externalLink`:
//! `externalBook`, `sheetNames`, `sheetDataSet`) with the workbook's `<externalReferences>`, so
//! saving as .xlsx keeps the link. Formulas then name a linked workbook as `[n]Sheet!A1`, `n`
//! being the link's position. The linked file is never opened, followed or refreshed.
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::{CellError, CellRef};
use gridcraft_model::{Preserved, PreservedPart, PreservedRel, PreservedXml, Workbook};

use crate::ReadReport;
use crate::passthrough::merge_preserved;
use crate::write::{NS_REL, XML_DECL};
use crate::xls::formula::{ExternalBook, Links};
use crate::xml::{esc_attr, push_a1, push_esc};

/// The content type of a SpreadsheetML external link part.
pub(crate) const CT_XML: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// Most cached cells read from one link.
pub(crate) const MAX_CACHED_CELLS: usize = 1_048_576;

/// A cached cell value of a linked workbook.
pub(crate) enum Cached {
    Blank,
    Number(f64),
    Bool(bool),
    Error(CellError),
    Text(String),
}

/// Appends `<cell>` for a cached value at `at` (inside an open `<row>`). A number that is NaN or
/// infinite is kept as the error `#NUM!`, as SpreadsheetML has no such number.
pub(crate) fn push_cell(out: &mut String, at: CellRef, value: &Cached) {
    out.push_str("<cell r=\"");
    push_a1(out, at);
    out.push('"');
    match value {
        Cached::Number(v) if v.is_finite() => {
            let _ = write!(out, "><v>{v}</v></cell>");
        }
        Cached::Number(_) => {
            let _ = write!(out, " t=\"e\"><v>{}</v></cell>", CellError::Num.as_str());
        }
        Cached::Bool(b) => {
            let _ = write!(out, " t=\"b\"><v>{}</v></cell>", u8::from(*b));
        }
        Cached::Error(e) => {
            let _ = write!(out, " t=\"e\"><v>{}</v></cell>", e.as_str());
        }
        Cached::Text(s) => {
            out.push_str(" t=\"str\"><v>");
            push_esc(out, s);
            out.push_str("</v></cell>");
        }
        Cached::Blank => out.push_str("/>"),
    }
}

/// A defined name of a linked workbook (ECMA-376 Part 1 §18.14.5 `definedName`).
pub(crate) struct LinkName {
    pub name: String,
    /// The index of the linked sheet the name is scoped to; `None` for a workbook-level name.
    pub sheet: Option<usize>,
    /// The definition (without `=`), in the linked workbook's own terms (`Prices!$B$2:$B$4`,
    /// `#REF!`); `None` when it wasn't saved or can't be read.
    pub refers_to: Option<String>,
}

/// Valid Excel sheet names standing in for a linked workbook's sheet names, index-aligned.
///
/// A legacy link can list names Excel rejects in an `.xlsx` (empty, over 31 characters, holding
/// `[ ] : * ? / \`, or repeated); Excel repairs a file whose `sheetName` (ECMA-376 Part 1
/// §18.14.7) holds one. Each name drops those characters, control characters and leading or
/// trailing apostrophes, keeps its first 31 characters, becomes `Sheet{n}` when nothing is left,
/// and gets ` (k)` when it repeats an earlier stand-in (ignoring case). Valid names are unchanged.
pub(crate) fn sheet_stand_ins(sheets: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::with_capacity(sheets.len());
    let mut out = Vec::with_capacity(sheets.len());
    for (i, name) in sheets.iter().enumerate() {
        let cleaned: String =
            name.chars().filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') && !('\u{0}'..='\u{1f}').contains(c)).collect();
        let mut s: String = cleaned.trim_matches('\'').chars().take(31).collect();
        if s.is_empty() {
            s = format!("Sheet{}", i.saturating_add(1));
        }
        if seen.contains(&s.to_lowercase()) {
            let base: String = s.chars().take(27).collect();
            // At most `sheets.len()` earlier stand-ins, so one of the first `len + 1` suffixes is free.
            for k in 2..=sheets.len().saturating_add(2) {
                let candidate = format!("{base} ({k})");
                if !seen.contains(&candidate.to_lowercase()) {
                    s = candidate;
                    break;
                }
            }
        }
        seen.insert(s.to_lowercase());
        out.push(s);
    }
    out
}

/// The `externalLink` part: the linked file is relationship `rel_id`, then its sheet names, its
/// defined `names` (`None` entries are skipped) and `tables` (the `<sheetData>` elements of its
/// cached cells).
pub(crate) fn link_xml(rel_id: &str, sheets: &[String], names: Option<&[Option<LinkName>]>, tables: &str) -> String {
    let mut xml = format!("{XML_DECL}<externalLink xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\"><externalBook r:id=\"{}\">", esc_attr(rel_id));
    if !sheets.is_empty() {
        xml.push_str("<sheetNames>");
        for s in sheets {
            let _ = write!(xml, "<sheetName val=\"{}\"/>", esc_attr(s));
        }
        xml.push_str("</sheetNames>");
    }
    let mut names = names.unwrap_or_default().iter().flatten().filter(|n| !n.name.is_empty()).peekable();
    if names.peek().is_some() {
        xml.push_str("<definedNames>");
        for n in names {
            let _ = write!(xml, "<definedName name=\"{}\"", esc_attr(&n.name));
            if let Some(f) = &n.refers_to {
                let _ = write!(xml, " refersTo=\"={}\"", esc_attr(f));
            }
            if let Some(s) = n.sheet {
                let _ = write!(xml, " sheetId=\"{s}\"");
            }
            xml.push_str("/>");
        }
        xml.push_str("</definedNames>");
    }
    if !tables.is_empty() {
        xml.push_str("<sheetDataSet>");
        xml.push_str(tables);
        xml.push_str("</sheetDataSet>");
    }
    xml.push_str("</externalBook></externalLink>");
    xml
}

/// One link read and ready to keep.
pub(crate) struct Kept {
    /// Its supporting link in [`Links::books`].
    pub book: usize,
    /// The linked workbook's sheet names.
    pub sheets: Vec<String>,
    /// The linked workbook's defined names, index-aligned with the records that named them.
    pub names: Vec<Option<LinkName>>,
    /// The `externalLink` part and the workbook relationship to it.
    pub part: PreservedPart,
    pub rel: PreservedRel,
}

/// Keeps every link (in order: link `n` is the `n`th) with the workbook's `<externalReferences>`,
/// lets formulas name the linked workbooks, and says so once. The values saved with each link are
/// read back from its part, as the `.xlsx` reader reads them ([`crate::external`]), so formulas
/// into it calculate from them.
pub(crate) fn keep(kept: Vec<Kept>, links: &mut Links, wb: &mut Workbook, report: &mut ReadReport) {
    if kept.is_empty() {
        return;
    }
    let mut p = Preserved::default();
    let mut x = format!("<externalReferences xmlns:r=\"{NS_REL}\">");
    for k in &kept {
        let _ = write!(x, "<externalReference r:id=\"{}\"/>", esc_attr(&k.rel.id));
    }
    x.push_str("</externalReferences>");
    p.elements.push(PreservedXml { slot: "externalReferences".into(), xml: x });
    let mut books = Vec::with_capacity(kept.len());
    for (n, k) in kept.into_iter().enumerate() {
        books.push(crate::external::book_of_part(&k.part.data));
        p.parts.push(k.part);
        p.rels.push(k.rel);
        if let Some(b) = links.books.get_mut(k.book) {
            b.external = Some(ExternalBook { number: n + 1, sheets: k.sheets, names: k.names });
        }
    }
    merge_preserved(&mut wb.preserved, Some(Arc::new(p)));
    wb.external_books = Arc::new(books);
    report.warn("links to external workbooks were kept; they are not followed or refreshed");
}

#[cfg(test)]
mod tests {
    use super::sheet_stand_ins;

    fn v(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn valid_unique_names_are_unchanged() {
        let names = v(&["Data", "Sales 2024", "Ünïcødé", "a'b", "Thirty-one characters exactly!!"]);
        assert_eq!(sheet_stand_ins(&names), names);
    }

    #[test]
    fn invalid_names_become_valid_unique_stand_ins() {
        let long = "A linked sheet name well over thirty-one characters";
        let got = sheet_stand_ins(&v(&["Data", "", "Q1:2024/Plan", long, "DATA", "'quoted'", "[]", "\u{1}x\t"]));
        assert_eq!(got, v(&["Data", "Sheet2", "Q12024Plan", "A linked sheet name well over t", "DATA (2)", "quoted", "Sheet7", "x"]));
    }

    #[test]
    fn repeated_long_names_are_cut_before_the_suffix() {
        let long = "x".repeat(40);
        let got = sheet_stand_ins(&v(&[&long, &long, &long]));
        let base = "x".repeat(27);
        assert_eq!(got, vec!["x".repeat(31), format!("{base} (2)"), format!("{base} (3)")]);
    }

    #[test]
    fn a_suffix_skips_names_already_taken() {
        let got = sheet_stand_ins(&v(&["A (2)", "A", "a"]));
        assert_eq!(got, v(&["A (2)", "A", "a (3)"]));
    }
}
