//! Protection records from [MS-XLS] 2.4.112 (FeatHdr), 2.4.191 (Password),
//! 2.4.207 (Protect), and the action flags in 2.5.104 (EnhancedProtection).
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{EditRange, PasswordHash, Sheet, SheetProtection};

use super::{Cursor, invalid};
use crate::{IoError, ReadReport};

/// Protection records are independent and may precede or follow one another.
#[derive(Default)]
pub(super) struct Protection {
    pub locked: bool,
    pub password: Option<PasswordHash>,
    actions: Option<u32>,
}

impl Protection {
    pub fn sheet(&self) -> Option<SheetProtection> {
        if !self.locked {
            return None;
        }
        // Without EnhancedProtection, only selecting cells is allowed.
        let bits = self.actions.unwrap_or((1 << 10) | (1 << 14));
        let allowed = |bit: u32| bits & (1u32 << bit) != 0;
        Some(SheetProtection {
            password: self.password.clone(),
            legacy_password_hash: None,
            select_locked: allowed(10),
            select_unlocked: allowed(14),
            format_cells: allowed(2),
            format_columns: allowed(3),
            format_rows: allowed(4),
            insert_columns: allowed(5),
            insert_rows: allowed(6),
            delete_columns: allowed(8),
            delete_rows: allowed(9),
            sort: allowed(11),
            autofilter: allowed(12),
        })
    }
}

fn word(c: &mut Cursor<'_, '_>) -> Result<u16, IoError> {
    if c.remaining() != 2 {
        return Err(invalid("protection word must be two bytes"));
    }
    c.u16()
}

pub(super) fn protect(c: &mut Cursor<'_, '_>, state: &mut Protection, report: &mut ReadReport) {
    match word(c) {
        Ok(value @ 0..=1) => state.locked = value == 1,
        _ => report.warn("An invalid .xls Protect record was skipped."),
    }
}

pub(super) fn password(c: &mut Cursor<'_, '_>, state: &mut Protection, report: &mut ReadReport) {
    match word(c) {
        Ok(value) => state.password = (value != 0).then_some(PasswordHash::Legacy(value)),
        Err(_) => report.warn("An invalid .xls Password record was skipped."),
    }
}

pub(super) fn feat_hdr(c: &mut Cursor<'_, '_>, state: &mut Protection, report: &mut ReadReport) {
    match enhanced(c) {
        Ok(Some(bits)) => state.actions = Some(bits),
        Ok(None) => {}
        Err(_) => report.warn("An invalid .xls FeatHdr protection record was skipped."),
    }
}

fn enhanced(c: &mut Cursor<'_, '_>) -> Result<Option<u32>, IoError> {
    c.skip(12)?; // FrtHeader
    let isf = c.u16()?;
    c.u8()?; // reserved
    let presence = c.u32()?;
    if isf != 2 {
        return Ok(None);
    }
    // MS-XLS 2.4.112: 0 means absent; FFFFFFFF means present, not a length.
    match presence {
        0 if c.remaining() == 0 => Ok(None),
        u32::MAX if c.remaining() == 4 => c.u32().map(Some),
        _ => Err(invalid("invalid EnhancedProtection payload")),
    }
}

// Feat ([MS-XLS] 2.4.111), FeatProtection (2.5.124) and SDContainer (2.5.234).
pub(super) fn feat(c: &mut Cursor<'_, '_>, sheet: &mut Sheet, report: &mut ReadReport) {
    match edit_range(c) {
        Ok(Some((range, permissions))) => {
            sheet.edit_ranges.push(range);
            if permissions {
                report.warn("Windows user permissions on Allow Edit Ranges were not kept; their passwords still apply.");
            }
        }
        Ok(None) => {}
        Err(_) => report.warn("An invalid .xls Feat Allow Edit Ranges record was skipped."),
    }
}

fn edit_range(c: &mut Cursor<'_, '_>) -> Result<Option<(EditRange, bool)>, IoError> {
    c.skip(12)?; // FrtHeader
    if c.u16()? != 2 {
        return Ok(None);
    }
    c.skip(5)?; // reserved1, reserved2
    let count = usize::from(c.u16()?);
    c.skip(6)?; // cbFeatData is ignored for ISFPROTECTION; reserved3
    if count == 0 {
        return Ok(None);
    }
    if count > c.remaining() / 8 {
        return Err(invalid("truncated edit range references"));
    }
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
        if r0 > r1 || c0 > c1 || c1 >= 256 {
            return Err(invalid("invalid edit range reference"));
        }
        ranges.push(RangeRef::new(CellRef::new(u32::from(r0), u32::from(c0)), CellRef::new(u32::from(r1), u32::from(c1))));
    }
    let permissions = c.u32()? & 1 != 0;
    let password = c.u32()? as u16;
    let title = c.string()?;
    if permissions {
        let size = c.u32()? as usize;
        if size > c.remaining() {
            return Err(invalid("truncated edit range security descriptor"));
        }
        c.skip(size)?;
    }
    if c.remaining() != 0 {
        return Err(invalid("unexpected edit range bytes"));
    }
    Ok(Some((
        EditRange { title, ranges, password: (password != 0).then_some(PasswordHash::Legacy(password)), security_descriptor: None },
        permissions,
    )))
}
