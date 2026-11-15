//! Cell notes, hyperlinks and frozen panes of a BIFF8 worksheet substream, from the public
//! [MS-XLS] and [MS-OSHARED] specifications:
//!
//! - notes: Obj ([MS-XLS] 2.4.181, its FtCmo names the note's object id), TxO (2.4.329, the text,
//!   in the `Continue` records after it) and Note (2.4.179, the cell, author and shown flag);
//! - hyperlinks: HLink (2.4.140) holding a Hyperlink Object ([MS-OSHARED] 2.3.7.1) with a URL
//!   or file moniker and/or a location in the workbook, and HLinkTooltip (2.4.141);
//! - panes: the fFrozen flag of Window2 (2.4.346, the other display flags are read with the
//!   record in `xls.rs`) and Pane (2.4.189).
//!
//! Obj and TxO records are also read by the drawing reader (`chart::Embedded`): a note's Obj
//! leaves it nothing pending, so the note's TxO is taken here and never as a text box's or
//! shape's text; any other Obj clears the pending note, so a text box's TxO is never a note's.
//!
//! The records are gathered while the substream is read and applied when it ends, so their
//! order in the file doesn't matter. A malformed note or hyperlink is skipped with a warning;
//! it never fails the whole file.

use std::collections::HashMap;

use gridcraft_core::{CellRef, MAX_ROWS, RangeRef};
use gridcraft_model::{Comment, Hyperlink, Sheet};

use super::records::{Cursor, Record};
use super::{Budget, XLS_MAX_COLS};
use crate::{IoError, ReadReport};

/// Notes and hyperlink records kept per sheet at most (Excel allows 65,530 hyperlinks).
const MAX_NOTES: usize = 65_536;
const MAX_LINKS: usize = 65_536;
/// Cells given a hyperlink per sheet at most (as for .xlsx).
const MAX_LINK_CELLS: usize = 10_000;
/// Bytes of hyperlink targets and tooltips copied onto a sheet's cells at most (copied per cell).
const MAX_LINK_BYTES: usize = 16 * 1024 * 1024;
/// `..\` steps of a relative file link at most.
const MAX_ANTI: usize = 64;

/// FtCmo.ot of a note ([MS-XLS] 2.5.143).
const OT_NOTE: u16 = 0x0019;
/// CLSID_URLMoniker {79EAC9E0-BAF9-11CE-8C82-00AA004BA90B} as stored.
const URL_MONIKER: [u8; 16] = [0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B];
/// CLSID_FileMoniker {00000303-0000-0000-C000-000000000046} as stored.
const FILE_MONIKER: [u8; 16] = [0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];

struct Note {
    pos: CellRef,
    shown: bool,
    obj: u16,
    author: String,
}

/// A Pane record: the split position and the scrolling pane's first cell, and the active pane.
pub(super) struct Pane {
    x: u16,
    y: u16,
    top: u16,
    left: u16,
    pub(super) active: Option<u8>,
}

/// What a worksheet's note, hyperlink and pane records said, applied by [`Objects::finish`].
#[derive(Default)]
pub(super) struct Objects {
    /// The object id of the last Obj that is a note, waiting for its TxO.
    note_obj: Option<u16>,
    texts: HashMap<u16, String>,
    notes: Vec<Note>,
    links: Vec<(RangeRef, Hyperlink)>,
    /// Index in `links` of the last link for each range, for the tooltips that follow.
    link_at: HashMap<RangeRef, usize>,
    bad_notes: bool,
    bad_links: bool,
    /// Window2's fFrozen, from the sheet's first Window2.
    frozen: Option<bool>,
    pub(super) pane: Option<Pane>,
}

/// An error that must stop the import (a resource budget), as opposed to a malformed record.
fn fatal(e: &IoError) -> bool {
    matches!(e, IoError::TooLarge(_))
}

/// `n` UTF-16 code units (fewer if the record ends first), up to the first NUL.
fn utf16(c: &mut Cursor<'_, '_>, n: usize) -> Result<String, IoError> {
    let n = n.min(c.remaining() / 2);
    let mut units = Vec::with_capacity(n);
    for _ in 0..n {
        units.push(c.u16()?);
    }
    if let Some(end) = units.iter().position(|&u| u == 0) {
        units.truncate(end);
    }
    Ok(String::from_utf16_lossy(&units))
}

/// HyperlinkString ([MS-OSHARED] 2.3.7.9): a character count that includes the final NUL,
/// then UTF-16.
fn hyperlink_string(c: &mut Cursor<'_, '_>) -> Result<String, IoError> {
    let n = c.u32()? as usize;
    utf16(c, n)
}

/// HyperlinkMoniker ([MS-OSHARED] 2.3.7.2): a URL or file moniker. `None` for other kinds.
fn moniker(c: &mut Cursor<'_, '_>) -> Result<Option<String>, IoError> {
    let mut clsid = [0u8; 16];
    for b in clsid.iter_mut() {
        *b = c.u8()?;
    }
    if clsid == URL_MONIKER {
        // URLMoniker (2.3.7.6): byte length, then the NUL-terminated URL (maybe more after it).
        let len = c.u32()? as usize;
        return Ok(Some(utf16(c, len / 2)?));
    }
    if clsid != FILE_MONIKER {
        return Ok(None);
    }
    // FileMoniker (2.3.7.8): `..\` count, ANSI path, then maybe the Unicode path.
    let anti = usize::from(c.u16()?).min(MAX_ANTI);
    let ansi_len = (c.u32()? as usize).min(c.remaining());
    let mut ansi = Vec::with_capacity(ansi_len);
    for _ in 0..ansi_len {
        ansi.push(c.u8()?);
    }
    if let Some(end) = ansi.iter().position(|&b| b == 0) {
        ansi.truncate(end);
    }
    let mut path: String = ansi.iter().map(|&b| char::from(b)).collect();
    // endServer, versionNumber, reserved1 (16) and reserved2 (4).
    if c.remaining() >= 24 {
        c.skip(24)?;
        let size = c.u32()? as usize;
        if size > 0 {
            let bytes = c.u32()? as usize;
            c.u16()?; // usKeyValue
            let wide = utf16(c, bytes / 2)?;
            if !wide.is_empty() {
                path = wide;
            }
        }
    }
    Ok(Some(format!("{}{path}", "..\\".repeat(anti))))
}

/// Ref8U: rows and columns, `None` when reversed or outside a BIFF8 sheet.
fn ref8(c: &mut Cursor<'_, '_>) -> Result<Option<RangeRef>, IoError> {
    let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
    if r1 < r0 || c1 < c0 || u32::from(c1) >= XLS_MAX_COLS || u32::from(r1) >= MAX_ROWS {
        return Ok(None);
    }
    Ok(Some(RangeRef::new(CellRef::new(u32::from(r0), u32::from(c0)), CellRef::new(u32::from(r1), u32::from(c1)))))
}

/// HLink: the cells and the link. `Ok(None)` for a link GridCraft can't represent.
fn hlink(c: &mut Cursor<'_, '_>) -> Result<Option<(RangeRef, Hyperlink)>, IoError> {
    let Some(range) = ref8(c)? else { return Ok(None) };
    c.skip(16)?; // hlinkClsid
    let _version = c.u32()?;
    let flags = c.u32()?;
    if flags & 0x10 != 0 {
        hyperlink_string(c)?; // displayName: the cell's text is what shows
    }
    if flags & 0x80 != 0 {
        hyperlink_string(c)?; // targetFrameName
    }
    let mut target = String::new();
    let external = flags & 0x01 != 0;
    if external {
        target = if flags & 0x100 != 0 {
            hyperlink_string(c)?
        } else {
            match moniker(c)? {
                Some(t) => t,
                None => return Ok(None),
            }
        };
    }
    let location = if flags & 0x08 != 0 { hyperlink_string(c)? } else { String::new() };
    let location = location.trim_start_matches('#');
    if !location.is_empty() {
        target = if target.is_empty() { location.to_string() } else { format!("{target}#{location}") };
    }
    if target.is_empty() {
        return Ok(None);
    }
    Ok(Some((range, Hyperlink { target, tooltip: None, external, display: None })))
}

/// TxO: the text in the `Continue` records after the header, each starting with a compression
/// flag. Only the declared characters are read: formatting runs and any drawing data after them
/// are left to the drawing reader.
fn txo_text(rec: &Record<'_>) -> Result<String, IoError> {
    let mut c = Cursor::new(&rec.parts);
    c.skip(10)?; // flags, rot, controlInfo
    let cch = usize::from(c.u16()?);
    if cch == 0 {
        return Ok(String::new());
    }
    let parts = rec.parts.get(1..).unwrap_or(&[]);
    let mut t = Cursor::new(parts);
    let high = t.u8()? & 1 != 0;
    t.chars(cch.min(t.remaining()), high)
}

/// Note text as Excel shows it: line breaks as `\n`, without the "Author:" line Excel starts
/// a note with (as the .xlsx reader does).
fn note_text(raw: &str, author: &str) -> String {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    if !author.is_empty()
        && let Some(rest) = text.strip_prefix(&format!("{author}:"))
    {
        return rest.strip_prefix('\n').unwrap_or(rest).to_string();
    }
    text
}

impl Objects {
    /// An Obj record: remembers a note's object id for the TxO that follows; any other object
    /// forgets it.
    pub(super) fn obj(&mut self, rec: &Record<'_>) {
        let mut c = Cursor::new(&rec.parts);
        // FtCmo: ft, cb, ot, id.
        let cmo = (|| Ok::<_, IoError>((c.u16()?, c.u16()?, c.u16()?, c.u16()?)))();
        self.note_obj = match cmo {
            Ok((0x0015, _, OT_NOTE, id)) => Some(id),
            _ => None,
        };
    }

    /// A TxO record: a note's text when the Obj before it is a note.
    pub(super) fn txo(&mut self, rec: &Record<'_>, budget: &mut Budget) -> Result<(), IoError> {
        let Some(id) = self.note_obj.take() else { return Ok(()) };
        match txo_text(rec) {
            Ok(text) => {
                budget.text(text.len())?;
                self.texts.insert(id, text);
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(_) => self.bad_notes = true,
        }
        Ok(())
    }

    /// A Note record: the cell, the shown flag, the object id and the author.
    pub(super) fn note(&mut self, rec: &Record<'_>, budget: &mut Budget) -> Result<(), IoError> {
        let mut c = Cursor::new(&rec.parts);
        let note = (|| {
            let (row, col, grbit, obj) = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
            let author = if c.remaining() >= 3 { c.string()? } else { String::new() };
            Ok::<_, IoError>((row, col, grbit, obj, author))
        })();
        match note {
            Ok((row, col, grbit, obj, author)) if u32::from(col) < XLS_MAX_COLS && self.notes.len() < MAX_NOTES => {
                budget.text(author.len())?;
                self.notes.push(Note { pos: CellRef::new(u32::from(row), u32::from(col)), shown: grbit & 0x0002 != 0, obj, author });
            }
            Err(e) if fatal(&e) => return Err(e),
            _ => self.bad_notes = true,
        }
        Ok(())
    }

    /// An HLink record.
    pub(super) fn hlink(&mut self, rec: &Record<'_>, budget: &mut Budget) -> Result<(), IoError> {
        let mut c = Cursor::new(&rec.parts);
        match hlink(&mut c) {
            Ok(Some(link)) if self.links.len() < MAX_LINKS => {
                budget.text(link.1.target.len())?;
                self.link_at.insert(link.0, self.links.len());
                self.links.push(link);
            }
            Err(e) if fatal(&e) => return Err(e),
            _ => self.bad_links = true,
        }
        Ok(())
    }

    /// An HLinkTooltip record: the tooltip of the last link on the same range.
    pub(super) fn tooltip(&mut self, rec: &Record<'_>, budget: &mut Budget) -> Result<(), IoError> {
        let mut c = Cursor::new(&rec.parts);
        let tip = (|| {
            c.u16()?; // rt
            let range = ref8(&mut c)?;
            let n = c.remaining() / 2;
            Ok::<_, IoError>((range, utf16(&mut c, n)?))
        })();
        if let Ok((Some(range), tip)) = tip
            && !tip.is_empty()
            && let Some(&i) = self.link_at.get(&range)
            && let Some((_, link)) = self.links.get_mut(i)
        {
            budget.text(tip.len())?;
            link.tooltip = Some(tip);
        }
        Ok(())
    }

    /// Window2's flags: whether the panes are frozen (the sheet's first Window2 counts).
    pub(super) fn window2(&mut self, grbit: u16) {
        if self.frozen.is_none() {
            self.frozen = Some(grbit & 0x0008 != 0);
        }
    }

    /// A Pane record: x, y, rwTop, colLeft, pnnAct. The first one counts.
    pub(super) fn pane(&mut self, rec: &Record<'_>) {
        let mut c = Cursor::new(&rec.parts);
        if self.pane.is_none()
            && let Ok((x, y, top, left)) = (|| Ok::<_, IoError>((c.u16()?, c.u16()?, c.u16()?, c.u16()?)))()
        {
            self.pane = Some(Pane { x, y, top, left, active: c.u8().ok() });
        }
    }

    /// Applies what was read to the sheet. Notes and hyperlinks are only read from BIFF8 files.
    pub(super) fn finish(self, sheet: &mut Sheet, report: &mut ReadReport) {
        for n in self.notes {
            let raw = self.texts.get(&n.obj).map_or("", String::as_str);
            let text = note_text(raw, &n.author);
            sheet.comments.insert(
                n.pos,
                Comment {
                    author: n.author,
                    text,
                    replies: vec![],
                    threaded: false,
                    resolved: false,
                    visible: n.shown,
                    dates: vec![],
                    note_box: None,
                    picture_fill: None,
                    note_shape: None,
                    note_runs: None,
                },
            );
        }
        let mut cells = 0usize;
        let mut bytes = 0usize;
        'links: for (range, link) in self.links {
            for at in range.iter() {
                if cells >= MAX_LINK_CELLS {
                    report.warn(format!("Hyperlinks past the first {MAX_LINK_CELLS} cells of a sheet were dropped."));
                    break 'links;
                }
                // Each copy holds the target and the tooltip, which can span Continue records.
                let copied = link.target.len().saturating_add(link.tooltip.as_ref().map_or(0, String::len));
                if bytes.saturating_add(copied) > MAX_LINK_BYTES {
                    report.warn(format!("Hyperlink data past {MAX_LINK_BYTES} bytes of a sheet were dropped."));
                    break 'links;
                }
                cells += 1;
                bytes += copied;
                sheet.hyperlinks.insert(at, link.clone());
            }
        }
        match (&self.pane, self.frozen) {
            (Some(p), Some(true)) if p.x > 0 || p.y > 0 => {
                let rows = u32::from(p.y).min(MAX_ROWS.saturating_sub(1));
                let cols = u32::from(p.x).min(XLS_MAX_COLS - 1);
                sheet.freeze = Some((rows, cols));
                // With frozen panes the scroll position is the scrolling pane's first cell.
                if u32::from(p.top) < MAX_ROWS && u32::from(p.left) < XLS_MAX_COLS {
                    sheet.view_top_left = CellRef::new(u32::from(p.top), u32::from(p.left));
                }
            }
            (Some(p), Some(false)) if p.x > 0 || p.y > 0 => {
                report.warn("Split (not frozen) window panes in .xls files are not supported and were removed.")
            }
            _ => {}
        }
        if self.bad_notes {
            report.warn("Some cell notes in this .xls file could not be read and were skipped.");
        }
        if self.bad_links {
            report.warn("Some hyperlinks in this .xls file could not be read and were skipped.");
        }
    }
}
