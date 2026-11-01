//! Protection records from [MS-XLS] 2.4.112 (FeatHdr), 2.4.191 (Password),
//! 2.4.207 (Protect), and the action flags in 2.5.104 (EnhancedProtection).
use gridcraft_model::{PasswordHash, SheetProtection};

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
