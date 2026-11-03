//! DIF (`.dif`) import and export. Stub: the task replaces these bodies.

use gridcraft_model::{Sheet, Workbook};

use crate::{IoError, ReadReport};

/// Reads a DIF file into a one-sheet workbook.
pub fn read_dif(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    let _ = bytes;
    Err(IoError::Format("DIF import is not implemented".into()))
}

/// Writes the sheet's values as DIF.
pub fn write_dif(sheet: &Sheet) -> Result<Vec<u8>, IoError> {
    let _ = sheet;
    Err(IoError::Format("DIF export is not implemented".into()))
}
