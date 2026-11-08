//! XLSB links to other workbooks, read from the public [MS-XLSB] specification: the workbook's
//! BrtSupBookSrc records (section 2.4.811) name External Link parts (section 2.1.7.25,
//! `xl/externalLinks/externalLinkN.bin`), each holding BrtBeginSupBook, the linked workbook's sheet
//! names (BrtSupTabs) and its cached cells (BrtExternTableStart, BrtExternRowHdr, BrtExternCell*).
//!
//! Each part is translated to its SpreadsheetML equivalent (ECMA-376 Part 1 §18.14 `externalLink`:
//! `externalBook`, `sheetNames`, `sheetDataSet`) and handed to the same pass-through store the
//! .xlsx reader fills for an `externalLink` part, with the workbook's `<externalReferences>`, so
//! saving as .xlsx keeps the link. The part's own relationships (the linked file's path) are kept
//! with their type and target unchanged. Formulas then name the linked workbook as `[n]Sheet!A1`,
//! `n` being the link's position. The linked file is never opened, followed or refreshed.
//!
//! As in the .xlsx reader, links are kept all together or not at all (`[n]` is a position): when
//! a part can't be read, the formulas that use any link keep their saved results. Defined names of
//! the linked workbook (BrtSupNameStart…) and DDE or OLE links are not read. A cached number that
//! is NaN or infinite is kept as the error `#NUM!`, as SpreadsheetML has no such number.
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{PreservedPart, PreservedRel, Workbook};

use super::{Bytes, MAX_REL_ID, REL_TYPE, Records, check_type, error};
use crate::external_link::{CT_XML, Cached, Kept, MAX_CACHED_CELLS, keep, link_xml, push_cell};
use crate::package::{Package, Rel};
use crate::passthrough::{MAX_KEPT_PART, raw_rels};
use crate::read::Ctx;
use crate::xls::formula::Links;
use crate::{IoError, ReadReport};

/// The content type of a binary external link part.
const CT_BINARY: &str = "application/vnd.ms-excel.externalLink";
/// Most sheet names of a linked workbook (BrtSupTabs: fewer than 65535).
const MAX_SHEET_NAMES: usize = 65_535;

/// A link part read: the linked workbook's sheet names and the SpreadsheetML part.
struct Link {
    /// BrtBeginSupBook's relationship id: the linked file.
    rel_id: String,
    sheets: Vec<String>,
    xml: String,
}

/// Reads the external link parts the workbook's BrtSupBookSrc records name, marks the links
/// formulas may name and keeps the parts for saving. Returns the package.
pub(super) fn read<'a>(
    package: Package<'a>,
    types: &[super::Entry],
    rels: &[Rel],
    links: &mut Links,
    wb: &mut Workbook,
    report: &mut ReadReport,
) -> Package<'a> {
    let ids: Vec<(usize, String)> =
        links.books.iter().enumerate().filter(|(_, b)| !b.internal && !b.addin).filter_map(|(i, b)| b.rel.clone().map(|r| (i, r))).collect();
    if ids.is_empty() {
        return package;
    }
    let mut cx = Ctx::new(package);
    let mut kept = vec![];
    for (n, (book, id)) in ids.iter().enumerate() {
        match link(&mut cx, types, rels, id, n + 1, report) {
            Ok((link, part, rel)) => kept.push(Kept { book: *book, sheets: link.sheets, names: Vec::new(), part, rel }),
            Err(why) => {
                report.warn(format!("links to external workbooks were dropped: {why}"));
                return cx.pkg;
            }
        }
    }
    keep(kept, links, wb, report);
    cx.pkg
}

/// One link: the binary part behind workbook relationship `id`, translated, with its own
/// relationships, as the part and relationship to keep.
fn link(
    cx: &mut Ctx<'_>,
    types: &[super::Entry],
    rels: &[Rel],
    id: &str,
    number: usize,
    report: &mut ReadReport,
) -> Result<(Link, PreservedPart, PreservedRel), String> {
    let rel = rels.iter().find(|r| r.id == id && r.kind == "externalLink" && !r.external).ok_or("a link has no part")?;
    check_type(types, &rel.target, CT_BINARY).map_err(|e| e.to_string())?;
    let data = match cx.pkg.read_limited(&rel.target, MAX_KEPT_PART) {
        Ok(Some(d)) => d,
        Ok(None) => return Err(format!("{} is missing", rel.target)),
        Err(e) => return Err(e.to_string()),
    };
    let part_rels = raw_rels(cx, &rel.target)?;
    // The part names only the linked file; a relationship to another part would not be kept.
    if part_rels.iter().any(|r| !r.external) {
        return Err(format!("{} has relationships to other parts", rel.target));
    }
    let link = part(&data, report).map_err(|e| format!("{}: {e}", rel.target))?;
    if !part_rels.iter().any(|r| r.id == link.rel_id) {
        return Err(format!("{}: the linked file's relationship is missing", rel.target));
    }
    let name = format!("xl/externalLinks/externalLink{number}.xml");
    let part = PreservedPart { name: name.clone(), content_type: CT_XML.into(), data: Arc::from(link.xml.as_bytes()), rels: part_rels };
    let kept = PreservedRel { id: id.to_string(), rel_type: format!("{REL_TYPE}externalLink"), target: name, external: false };
    Ok((link, part, kept))
}

/// Translates an External Link part (EXTERNALLINK, [MS-XLSB] 2.1.7.25) to SpreadsheetML.
fn part(data: &[u8], report: &mut ReadReport) -> Result<Link, IoError> {
    let invalid = super::invalid;
    let mut records = Records::new(data);
    let mut rel_id = None;
    let mut sheets: Option<Vec<String>> = None;
    let mut tables = String::new();
    // The cache being read: its sheet index and whether a row is open.
    let mut table: Option<bool> = None;
    let mut row = 0u32;
    let mut cells = 0usize;
    let mut ended = false;
    while let Some((id, payload)) = records.next(report)? {
        if ended {
            return Err(invalid("records after BrtEndSupBook"));
        }
        let mut b = Bytes::new(payload);
        match id {
            // BrtBeginSupBook (2.4.235): sbt, then for a workbook the relationship id and a null string.
            360 => {
                if rel_id.is_some() {
                    return Err(invalid("duplicate BrtBeginSupBook"));
                }
                if b.u16()? != 0 {
                    return Err(invalid("a DDE or OLE link"));
                }
                rel_id = Some(b.text(MAX_REL_ID)?);
                b.string(u32::MAX, true)?;
            }
            _ if rel_id.is_none() => return Err(invalid("external link part does not begin with BrtBeginSupBook")),
            // BrtSupTabs (2.4.825): cTab, then the sheet names.
            359 => {
                if sheets.is_some() {
                    return Err(invalid("duplicate BrtSupTabs"));
                }
                let n = usize::try_from(b.u32()?).unwrap_or(usize::MAX);
                if n >= MAX_SHEET_NAMES {
                    return Err(invalid("too many linked sheet names"));
                }
                let mut names = Vec::with_capacity(n.min(b.rest.len() / 4));
                for _ in 0..n {
                    names.push(b.text(32767)?);
                }
                sheets = Some(names);
            }
            // BrtExternTableStart (2.4.674): iTab, then fRefreshError.
            363 => {
                let count = sheets.as_ref().map_or(0, Vec::len);
                if table.is_some() {
                    return Err(invalid("nested BrtExternTableStart"));
                }
                let itab = b.u32()?;
                if usize::try_from(itab).map_or(true, |i| i >= count) {
                    return Err(invalid("cached cells of a missing linked sheet"));
                }
                let refresh_error = b.u8()? & 1 != 0;
                let _ = write!(tables, "<sheetData sheetId=\"{itab}\"{}>", if refresh_error { " refreshError=\"1\"" } else { "" });
                table = Some(false);
            }
            // BrtExternTableEnd (2.4.673).
            364 => {
                match table {
                    Some(true) => tables.push_str("</row></sheetData>"),
                    Some(false) => tables.push_str("</sheetData>"),
                    None => return Err(invalid("BrtExternTableEnd outside a cache")),
                }
                table = None;
            }
            // BrtExternRowHdr (2.4.671): rw, increasing within a cache.
            366 => {
                let open = table.ok_or_else(|| invalid("BrtExternRowHdr outside a cache"))?;
                let rw = b.u32()?;
                if rw >= gridcraft_core::MAX_ROWS || (open && rw <= row) {
                    return Err(invalid("invalid cached row"));
                }
                if open {
                    tables.push_str("</row>");
                }
                let _ = write!(tables, "<row r=\"{}\">", u64::from(rw) + 1);
                row = rw;
                table = Some(true);
            }
            // BrtExternCellBlank, Real, Bool, Error, String (2.4.666-670): col, then the value.
            367..=371 => {
                if table != Some(true) {
                    return Err(invalid("cached cell outside a row"));
                }
                cells += 1;
                if cells > MAX_CACHED_CELLS {
                    return Err(IoError::TooLarge("XLSB external link caches too many cells".into()));
                }
                let col = b.u32()?;
                if col >= gridcraft_core::MAX_COLS {
                    return Err(invalid("invalid cached column"));
                }
                let value = match id {
                    // Read raw: `Bytes::f64` refuses non-finite numbers, which would drop the link.
                    368 => Cached::Number(f64::from_le_bytes(b.take(8)?.try_into().map_err(|_| invalid("truncated number"))?)),
                    369 => Cached::Bool(b.u8()? != 0),
                    370 => Cached::Error(error(b.u8()?)?),
                    371 => Cached::Text(b.text(32767)?),
                    _ => Cached::Blank,
                };
                push_cell(&mut tables, CellRef::new(row, col), &value);
            }
            // BrtEndSupBook (2.4.588).
            588 => {
                if table.is_some() {
                    return Err(invalid("unterminated cache"));
                }
                ended = true;
            }
            // BrtExternValueMeta, linked names, alternate URLs and later additions are not read.
            _ => {}
        }
    }
    if !ended {
        return Err(invalid("missing BrtEndSupBook"));
    }
    let rel_id = rel_id.ok_or_else(|| invalid("missing BrtBeginSupBook"))?;
    let sheets = sheets.ok_or_else(|| invalid("missing BrtSupTabs"))?;
    let xml = link_xml(&rel_id, &sheets, None, &tables);
    Ok(Link { rel_id, sheets, xml })
}
