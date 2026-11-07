//! `.xls` links to other workbooks, read from the public [MS-XLS] specification: an external
//! SupBook record (2.4.271) names the linked file (a VirtualPath, 2.5.277) and its sheets; the XCT
//! (2.4.353) and CRN (2.4.65) records after it hold the cells of those sheets as last saved.
//!
//! Each link becomes the SpreadsheetML part the `.xlsx` reader keeps for it (see
//! [`crate::external_link`]), so saving as .xlsx keeps it, and formulas into it read as
//! `[n]Sheet!A1`. As in the other readers, links are kept all together or not at all (`[n]` is a
//! position). Links to other programs (DDE, OLE) and names defined in the linked workbook are not
//! read: formulas using them keep their saved results. The linked file is never opened.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{PreservedPart, PreservedRel, Workbook};

use super::formula::{Links, error_code};
use super::records::Cursor;
use crate::external_link::{CT_XML, Cached, Kept, MAX_CACHED_CELLS, keep, link_xml, push_cell};
use crate::{IoError, ReadReport};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// One linked workbook as its records were read.
struct Book {
    /// Its supporting link in [`Links::books`].
    book: usize,
    /// The linked file as a relationship target, or why the link can't be kept.
    target: Result<String, String>,
    sheets: Vec<String>,
    /// Cached cells: sheet index → row → column → value.
    cache: BTreeMap<u16, BTreeMap<u16, BTreeMap<u8, Cached>>>,
    cells: usize,
}

/// The XCT being read: its book (in [`Reader::books`]), sheet and the CRN records still to come.
struct Table {
    book: usize,
    itab: u16,
    left: u16,
}

/// Collects the external SupBook, XCT and CRN records of the workbook globals.
#[derive(Default)]
pub(super) struct Reader {
    books: Vec<Book>,
    table: Option<Table>,
    /// Whether the last SupBook read is one of [`Reader::books`].
    latest: bool,
    cache_warned: bool,
    dde_warned: bool,
}

impl Reader {
    /// A SupBook that is neither this workbook nor add-in functions, after `ctab` and `cch`;
    /// `book` is its index in [`Links::books`].
    pub(super) fn supbook(&mut self, c: &mut Cursor<'_, '_>, book: usize, ctab: u16, cch: u16, report: &mut ReadReport) {
        self.table = None;
        self.latest = false;
        let read = |c: &mut Cursor<'_, '_>| -> Result<(String, Vec<String>), IoError> {
            let high = c.u8()? & 1 != 0;
            let path = c.chars(usize::from(cch), high)?;
            if path == " " || path.starts_with('\u{2}') || (ctab == 0 && path.contains('\u{3}')) {
                return Ok((path, vec![]));
            }
            let mut sheets = Vec::with_capacity(usize::from(ctab).min(c.remaining() / 3));
            for _ in 0..ctab {
                sheets.push(c.string()?);
            }
            Ok((path, sheets))
        };
        let (target, sheets) = match read(c) {
            // This workbook named by a path: not a link.
            Ok((path, _)) if path == " " || path.starts_with('\u{2}') => return,
            Ok((path, _)) if ctab == 0 && path.contains('\u{3}') => {
                if !std::mem::replace(&mut self.dde_warned, true) {
                    report.warn("links to other programs (DDE or OLE) were not read; formulas using them keep their saved values");
                }
                return;
            }
            Ok((path, sheets)) => {
                let target = virtual_path(&path);
                if target.is_empty() || target.chars().any(|ch| u32::from(ch) < 0x20) {
                    (Err("a linked file's path can't be read".to_string()), sheets)
                } else {
                    (Ok(target), sheets)
                }
            }
            Err(e) => (Err(format!("a link can't be read ({e})")), vec![]),
        };
        self.books.push(Book { book, target, sheets, cache: BTreeMap::new(), cells: 0 });
        self.latest = true;
    }

    /// Any other SupBook (this workbook, add-ins): XCT records after it are not a link's.
    pub(super) fn other_supbook(&mut self) {
        self.table = None;
        self.latest = false;
    }

    /// XCT: `ccrn` CRN records of sheet `itab` of the latest linked workbook follow.
    pub(super) fn xct(&mut self, c: &mut Cursor<'_, '_>, report: &mut ReadReport) {
        self.table = None;
        let header = (|| Ok::<_, IoError>((c.u16()? as i16, c.u16()?)))();
        let Ok((ccrn, itab)) = header else {
            self.bad_cache(report);
            return;
        };
        let book = self.books.len().wrapping_sub(1);
        if !self.latest || self.books.get(book).is_none_or(|b| usize::from(itab) >= b.sheets.len()) {
            self.bad_cache(report);
            return;
        }
        self.table = Some(Table { book, itab, left: ccrn.unsigned_abs() });
    }

    /// CRN: one row of cached cells of the current XCT.
    pub(super) fn crn(&mut self, c: &mut Cursor<'_, '_>, report: &mut ReadReport) {
        let Some(t) = self.table.as_mut().filter(|t| t.left > 0) else {
            self.table = None;
            self.bad_cache(report);
            return;
        };
        t.left -= 1;
        let (book, itab) = (t.book, t.itab);
        let Some(b) = self.books.get_mut(book) else { return };
        let ok = (|| -> Option<()> {
            let last = c.u8().ok()?;
            let first = c.u8().ok()?;
            let rw = c.u16().ok()?;
            if last < first {
                return None;
            }
            for col in first..=last {
                let value = ser_ar(c)?;
                if b.cells >= MAX_CACHED_CELLS {
                    return None;
                }
                b.cells += 1;
                b.cache.entry(itab).or_default().entry(rw).or_default().insert(col, value);
            }
            Some(())
        })();
        if ok.is_none() {
            self.table = None;
            self.bad_cache(report);
        }
    }

    fn bad_cache(&mut self, report: &mut ReadReport) {
        if !std::mem::replace(&mut self.cache_warned, true) {
            report.warn("some cached cells of linked workbooks could not be read and were left out");
        }
    }

    /// Keeps the links read (all or none) and lets formulas name them.
    pub(super) fn finish(self, links: &mut Links, wb: &mut Workbook, report: &mut ReadReport) {
        if let Some(why) = self.books.iter().find_map(|b| b.target.as_ref().err()) {
            report.warn(format!("links to external workbooks were dropped: {why}"));
            return;
        }
        let mut kept = Vec::with_capacity(self.books.len());
        for (i, b) in self.books.into_iter().enumerate() {
            let Ok(target) = b.target else { return };
            let n = i + 1;
            let mut tables = String::new();
            for (itab, rows) in &b.cache {
                let _ = write!(tables, "<sheetData sheetId=\"{itab}\">");
                for (rw, cols) in rows {
                    let _ = write!(tables, "<row r=\"{}\">", u32::from(*rw) + 1);
                    for (col, value) in cols {
                        push_cell(&mut tables, CellRef::new(u32::from(*rw), u32::from(*col)), value);
                    }
                    tables.push_str("</row>");
                }
                tables.push_str("</sheetData>");
            }
            let xml = link_xml("rId1", &b.sheets, &tables);
            let name = format!("xl/externalLinks/externalLink{n}.xml");
            let path = PreservedRel { id: "rId1".into(), rel_type: format!("{REL}externalLinkPath"), target, external: true };
            let part = PreservedPart { name: name.clone(), content_type: CT_XML.into(), data: Arc::from(xml.as_bytes()), rels: vec![path] };
            let rel = PreservedRel { id: format!("rIdExternalLink{n}"), rel_type: format!("{REL}externalLink"), target: name, external: false };
            kept.push(Kept { book: b.book, sheets: b.sheets, part, rel });
        }
        keep(kept, links, wb, report);
    }
}

/// SerAr ([MS-XLS] 2.5.192): one cached value; `None` when truncated or of an unknown type.
fn ser_ar(c: &mut Cursor<'_, '_>) -> Option<Cached> {
    Some(match c.u8().ok()? {
        0x00 => {
            c.skip(8).ok()?;
            Cached::Blank
        }
        0x01 => Cached::Number(c.f64().ok()?),
        0x02 => Cached::Text(c.string().ok()?),
        0x04 => {
            let b = c.u8().ok()? != 0;
            c.skip(7).ok()?;
            Cached::Bool(b)
        }
        0x10 => {
            let e = error_code(c.u8().ok()?)?;
            c.skip(7).ok()?;
            Cached::Error(e)
        }
        _ => return None,
    })
}

/// VirtualPath ([MS-XLS] 2.5.277) as a file path: an encoded path (starting with 0x01) spells
/// drives, separators and parent folders with control characters; any other is the path as is.
fn virtual_path(raw: &str) -> String {
    let Some(rest) = raw.strip_prefix('\u{1}') else { return raw.to_string() };
    let mut out = String::with_capacity(rest.len() + 4);
    let sep = |out: &mut String| {
        if !out.ends_with('\\') {
            out.push('\\');
        }
    };
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            // A volume: a drive letter, or `@` for a UNC path whose server and share follow.
            '\u{1}' => match chars.next() {
                Some('@') => out.push_str(r"\\"),
                Some(drive) => {
                    out.push(drive);
                    out.push_str(":\\");
                }
                None => {}
            },
            // The root of the current drive, and a folder separator.
            '\u{2}' | '\u{3}' => sep(&mut out),
            '\u{4}' => out.push_str(r"..\"),
            // A UNC volume: its length, then its characters.
            '\u{5}' => {
                let n = chars.next().map_or(0, u32::from);
                out.push_str(r"\\");
                for _ in 0..n {
                    match chars.next() {
                        Some(v) => out.push(v),
                        None => break,
                    }
                }
            }
            // The startup, alternate startup and library folders.
            '\u{6}' | '\u{7}' | '\u{8}' => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Reader, virtual_path};
    use crate::ReadReport;
    use crate::xls::formula::{Links, SupBook};
    use crate::xls::records::Cursor;

    /// Reads one SupBook body (after `ctab` and `cch`) and keeps what can be kept.
    fn finish(ctab: u16, path: &str, rest: &[u8]) -> (Links, gridcraft_model::Workbook, ReadReport) {
        let body = [&[0u8][..], path.as_bytes(), rest].concat();
        let parts = [&body[..]];
        let (mut links, mut wb, mut report, mut r) = (Links::default(), gridcraft_model::Workbook::new(), ReadReport::default(), Reader::default());
        links.books.push(SupBook::default());
        r.supbook(&mut Cursor::new(&parts), 0, ctab, path.len() as u16, &mut report);
        r.finish(&mut links, &mut wb, &mut report);
        (links, wb, report)
    }

    #[test]
    fn dde_links_are_skipped_with_one_warning() {
        let (links, wb, report) = finish(0, "Server\u{3}topic", &[]);
        assert!(links.books[0].external.is_none() && wb.preserved.is_none());
        assert!(report.warnings.iter().any(|w| w.contains("DDE")), "{:?}", report.warnings);
    }

    #[test]
    fn a_path_with_control_characters_drops_the_links() {
        let (links, wb, report) = finish(1, "a\u{9}b.xls", &[1, 0, 0, b'S']);
        assert!(links.books[0].external.is_none() && wb.preserved.is_none());
        assert!(report.warnings.iter().any(|w| w.contains("were dropped")), "{:?}", report.warnings);
        let (links, wb, _) = finish(1, "ab.xls", &[1, 0, 0, b'S']);
        assert_eq!(links.books[0].external.as_ref().map(|e| (e.number, e.sheets.clone())), Some((1, vec!["S".to_string()])));
        assert!(wb.preserved.is_some());
    }

    #[test]
    fn virtual_paths_decode() {
        assert_eq!(virtual_path("other.xls"), "other.xls");
        assert_eq!(virtual_path("\u{1}\u{1}C\u{3}data\u{3}a.xls"), r"C:\data\a.xls");
        assert_eq!(virtual_path("\u{1}\u{1}@srv\u{3}share\u{3}a.xls"), r"\\srv\share\a.xls");
        assert_eq!(virtual_path("\u{1}\u{2}top\u{3}a.xls"), r"\top\a.xls");
        assert_eq!(virtual_path("\u{1}\u{4}\u{4}a.xls"), r"..\..\a.xls");
        assert_eq!(virtual_path("\u{1}\u{5}\u{3}srv\u{3}a.xls"), r"\\srv\a.xls");
        assert_eq!(virtual_path("\u{1}\u{6}a.xls"), "a.xls");
        // Truncated markers never panic.
        assert_eq!(virtual_path("\u{1}\u{1}"), "");
        assert_eq!(virtual_path("\u{1}\u{5}\u{9}ab"), r"\\ab");
    }
}
