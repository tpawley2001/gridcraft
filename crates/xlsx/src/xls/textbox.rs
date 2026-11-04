//! Text boxes in `.xls` files ([MS-XLS] 2.4.329 TxO, 2.5.195 OfficeArtClientTextbox): an Obj
//! record whose `cmo.ot` is 6 marks the drawing shape before it as a text box; the TxO record
//! that follows holds the text's alignment, its rotation and the character count, and the
//! `Continue` records after it carry the text as `XLUnicodeStringNoCch` parts (each starting
//! with its own compression flag, [MS-XLS] 2.5.293) and then the TxORuns formatting, which is
//! not imported: its bytes are never read as text.

use gridcraft_model::{ShapeTextAlign, ShapeTextAnchor};

use crate::ReadReport;

/// Characters kept of one text box at most.
const MAX_CHARS: usize = 32_768;
/// The shortest TxORuns, and its size's alignment ([MS-XLS] 2.5.272).
const MIN_RUNS: usize = 16;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

/// What a TxO record says about its text box.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Text {
    pub text: String,
    pub align: Option<ShapeTextAlign>,
    pub anchor: Option<ShapeTextAnchor>,
    /// `rot`: 1 stacked, 2 counter-clockwise, 3 clockwise ([MS-XLS] 2.4.329).
    pub rotation: u16,
}

/// Reads a TxO record and the `Continue` records that follow it. A form control's TxO carries a
/// ControlInfo where a text box's carries two reserved fields, both six bytes, so the counts
/// sit at the same offsets either way. `None` when the record is too short to hold them.
pub(super) fn read(parts: &[&[u8]], report: &mut ReadReport) -> Option<Text> {
    let head = parts.first().copied().unwrap_or_default();
    let (Some(flags), Some(rot)) = (u16_at(head, 0), u16_at(head, 2)) else {
        report.warn("A text box in a .xls file was damaged; its text was not read.");
        return None;
    };
    let (Some(cch_text), Some(cb_runs)) = (u16_at(head, 10), u16_at(head, 12)) else {
        report.warn("A text box in a .xls file was damaged; its text was not read.");
        return None;
    };
    let cch = usize::from(cch_text);
    // A text box with no text has no runs; otherwise the runs are at least one run's header.
    if (cch == 0 && cb_runs != 0) || (cch != 0 && (usize::from(cb_runs) < MIN_RUNS || cb_runs % 8 != 0)) {
        report.warn("A text box in a .xls file has malformed formatting runs; its text was kept without them.");
    }
    let (text, complete) = chars(parts.get(1..).unwrap_or_default(), cch);
    if cch > MAX_CHARS {
        report.warn(format!("Only the first {MAX_CHARS} characters of a text box in a .xls file were imported."));
    } else if !complete {
        report.warn("A text box cut off by the end of its Continue records kept the text that was read.");
    }
    Some(Text { text, align: align(flags), anchor: anchor(flags), rotation: rot })
}

/// The horizontal alignment in the TxO's first flags word.
fn align(flags: u16) -> Option<ShapeTextAlign> {
    match (flags >> 1) & 0x7 {
        1 => Some(ShapeTextAlign::Left),
        2 => Some(ShapeTextAlign::Center),
        3 => Some(ShapeTextAlign::Right),
        _ => None,
    }
}

/// The vertical alignment in the TxO's first flags word.
fn anchor(flags: u16) -> Option<ShapeTextAnchor> {
    match (flags >> 4) & 0x7 {
        1 => Some(ShapeTextAnchor::Top),
        2 => Some(ShapeTextAnchor::Middle),
        3 => Some(ShapeTextAnchor::Bottom),
        _ => None,
    }
}

/// `cch` characters from the text `Continue` records, and whether all of them were read. Every
/// part starts with its own compression flag; a part that ends in the middle of a character
/// (the next part's flag then comes before its second byte) and a part with no characters at
/// all both end the text: the `Continue` records after it are the formatting runs, whose bytes
/// are never read as text.
fn chars(parts: &[&[u8]], cch: usize) -> (String, bool) {
    let want = cch.min(MAX_CHARS);
    let mut units: Vec<u16> = Vec::with_capacity(want.min(1024));
    let mut complete = cch <= MAX_CHARS;
    for part in parts {
        if units.len() >= want {
            break;
        }
        let Some((&flag, body)) = part.split_first() else { continue };
        let high = flag & 1 != 0;
        let size = if high { 2 } else { 1 };
        let mut at = 0usize;
        let mut read = 0usize;
        while units.len() < want && at.saturating_add(size) <= body.len() {
            if high {
                let s = body.get(at..at.saturating_add(2)).unwrap_or_default();
                units.push(u16::from_le_bytes([s.first().copied().unwrap_or(0), s.get(1).copied().unwrap_or(0)]));
            } else {
                units.push(u16::from(body.get(at).copied().unwrap_or(0)));
            }
            at += size;
            read += 1;
        }
        if read == 0 {
            complete = false;
            break;
        }
    }
    if units.len() < want {
        complete = false;
    }
    (String::from_utf16_lossy(&units), complete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(parts: &[&[u8]], cch: usize) -> (String, bool) {
        chars(parts, cch)
    }

    #[test]
    fn text_spans_parts_with_their_own_flags() {
        // "ab" compressed, then "ç" and "d" as UTF-16.
        let (s, complete) = text(&[&[0, b'a', b'b'], &[1, 0xE7, 0, b'd', 0]], 4);
        assert_eq!(s, "abçd");
        assert!(complete);
    }

    #[test]
    fn formatting_after_the_last_character_is_not_text() {
        // Two characters, then bytes that look like text but are the runs.
        let (s, complete) = text(&[&[0, b'h', b'i', b'Z', b'Z']], 2);
        assert_eq!(s, "hi");
        assert!(complete);
    }

    #[test]
    fn a_truncated_continue_keeps_what_was_read() {
        let (s, complete) = text(&[&[1, 0x41, 0], &[0]], 3);
        assert_eq!(s, "A");
        assert!(!complete);
    }

    #[test]
    fn a_part_that_cannot_hold_a_character_ends_the_text() {
        // A UTF-16 part with one byte has no whole character: what follows is the runs.
        let (s, complete) = text(&[&[0, b'a'], &[1, 0x42], &[1, 0x43, 0]], 2);
        assert_eq!(s, "a");
        assert!(!complete);
    }

    #[test]
    fn an_empty_text_box_reads_no_characters() {
        let (s, complete) = text(&[], 0);
        assert_eq!(s, "");
        assert!(complete);
    }

    #[test]
    fn a_part_without_its_flag_is_skipped() {
        let (s, complete) = text(&[&[], &[0, b'o', b'k']], 2);
        assert_eq!(s, "ok");
        assert!(complete);
    }
}
