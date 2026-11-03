//! SYLK (`.slk`) import. Stub: the task replaces these bodies.

use gridcraft_core::CellRef;
use gridcraft_model::Workbook;

use crate::{IoError, ReadReport};

/// Whether the bytes are a SYLK file (they start with an `ID;` record).
pub fn is_sylk(bytes: &[u8]) -> bool {
    let _ = bytes;
    false
}

/// Reads a SYLK file into a one-sheet workbook.
pub fn read_sylk(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    let _ = bytes;
    Err(IoError::Format("SYLK import is not implemented".into()))
}

/// Converts an R1C1 formula written in the cell `at` to A1 text.
pub(crate) fn r1c1_to_a1(formula: &str, at: CellRef) -> Option<String> {
    let _ = (formula, at);
    None
}
