//! Text boxes in `.xls` files ([MS-XLS] 2.4.329 TxO, 2.5.195 OfficeArtClientTextbox): an Obj
//! record whose `cmo.ot` is 6 marks the drawing shape before it as a text box; the TxO record
//! that follows holds the text's alignment, its rotation and the character count, and the
//! `Continue` records after it carry the text as `XLUnicodeStringNoCch` parts (each starting
//! with its own compression flag, [MS-XLS] 2.5.293) and then the TxORuns formatting, which is
//! read separately from the characters; only the first run's font is imported.

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
    pub font: Option<u16>,
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
    let parts = parts.get(1..).unwrap_or_default();
    let text_parts = text_parts(parts, usize::from(cb_runs));
    let font = if cch != 0 && usize::from(cb_runs) >= MIN_RUNS && cb_runs % 8 == 0 {
        // Run bytes can span Continue records, including the first run's header.
        let mut bytes = parts.get(text_parts.len()..).unwrap_or_default().iter().flat_map(|p| p.iter().copied());
        let header: Option<[u8; 4]> = (|| Some([bytes.next()?, bytes.next()?, bytes.next()?, bytes.next()?]))();
        header.and_then(|[lo, hi, fl, fh]| (u16::from_le_bytes([lo, hi]) == 0).then_some(u16::from_le_bytes([fl, fh])))
    } else {
        None
    };
    let (text, complete) = chars(text_parts, cch);
    if cch > MAX_CHARS {
        report.warn(format!("Only the first {MAX_CHARS} characters of a text box in a .xls file were imported."));
    } else if !complete {
        report.warn("A text box cut off by the end of its Continue records kept the text that was read.");
    }
    Some(Text { text, font, align: align(flags), anchor: anchor(flags), rotation: rot })
}

/// The part and byte after the declared characters and formatting runs. Character parts
/// each have a compression flag; run bytes have none ([MS-XLS] 2.4.329). This only locates
/// trailing drawing data and does not decode or repair text.
pub(super) fn text_end(parts: &[&[u8]]) -> Option<(usize, usize)> {
    let head = parts.first()?;
    let mut characters = usize::from(u16_at(head, 10)?);
    let mut runs = usize::from(u16_at(head, 12)?);
    for (index, part) in parts.iter().enumerate().skip(1) {
        let mut used = 0;
        if characters != 0 {
            let (&flag, body) = part.split_first()?;
            let width = if flag & 1 != 0 { 2 } else { 1 };
            let count = characters.min(body.len() / width);
            characters -= count;
            used = 1 + count * width;
            if characters != 0 {
                // A partial character cannot establish a reliable drawing boundary.
                if used != part.len() || count == 0 {
                    return None;
                }
                continue;
            }
        }
        let count = runs.min(part.len().saturating_sub(used));
        runs -= count;
        used += count;
        if runs == 0 {
            return Some((index, used));
        }
    }
    None
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

/// The `Continue` parts that hold the text. The formatting runs are the last parts, whose
/// lengths add up to `cbRuns` ([MS-XLS] 2.4.329); the parts before them are the text. When the
/// lengths don't add up (a damaged record), every part is offered to the text reader, which
/// stops at the first part that holds no whole character.
fn text_parts<'a>(parts: &'a [&'a [u8]], cb_runs: usize) -> &'a [&'a [u8]] {
    if cb_runs == 0 {
        return parts;
    }
    let mut total = 0usize;
    for (i, part) in parts.iter().enumerate().rev() {
        total = total.saturating_add(part.len());
        if total == cb_runs {
            return parts.get(..i).unwrap_or_default();
        }
        if total > cb_runs {
            break;
        }
    }
    parts
}

/// `cch` characters from the text `Continue` parts, and whether all of them were read. Every
/// part starts with its own compression flag ([MS-XLS] 2.5.293); a part that ends in the middle
/// of a character (the next part's flag then comes before its second byte) and a part with no
/// characters at all both end the text, so the formatting runs are never read as text.
fn chars(parts: &[&[u8]], cch: usize) -> (String, bool) {
    let want = cch.min(MAX_CHARS);
    let mut units: Vec<u16> = Vec::with_capacity(want.min(1024));
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
            break;
        }
    }
    let complete = cch <= MAX_CHARS && units.len() >= want;
    (String::from_utf16_lossy(&units), complete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(parts: &[&[u8]], cch: usize) -> (String, bool) {
        chars(parts, cch)
    }

    #[test]
    fn drawing_boundary_counts_characters_and_split_runs() {
        let mut head = [0u8; 18];
        head[10..12].copy_from_slice(&4u16.to_le_bytes());
        head[12..14].copy_from_slice(&16u16.to_le_bytes());
        let parts: [&[u8]; 6] = [&head, &[0, b'a', b'b'], &[1, b'c', 0, b'd', 0], &[0; 3], &[0; 13], &[15, 0, 4, 240]];
        assert_eq!(text_end(&parts), Some((4, 13)));
    }

    #[test]
    fn drawing_boundary_can_be_inside_a_part() {
        let mut head = [0u8; 18];
        head[10..12].copy_from_slice(&2u16.to_le_bytes());
        head[12..14].copy_from_slice(&16u16.to_le_bytes());
        let mut part = vec![0, b'h', b'i'];
        part.extend([0; 16]);
        part.extend([15, 0, 4, 240]);
        assert_eq!(text_end(&[&head, &part]), Some((1, 19)));
    }

    #[test]
    fn truncated_text_or_runs_have_no_drawing_boundary() {
        let mut head = [0u8; 18];
        head[10..12].copy_from_slice(&2u16.to_le_bytes());
        head[12..14].copy_from_slice(&16u16.to_le_bytes());
        assert_eq!(text_end(&[&head, &[1, b'a']]), None);
        assert_eq!(text_end(&[&head, &[0, b'h', b'i'], &[0; 15]]), None);
    }

    #[test]
    fn first_font_run_can_span_continue_records() {
        let mut head = [0u8; 18];
        head[10..12].copy_from_slice(&2u16.to_le_bytes());
        head[12..14].copy_from_slice(&16u16.to_le_bytes());
        let runs = [0, 0, 5, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];
        let parts: [&[u8]; 4] = [&head, &[0, b'h', b'i'], &runs[..3], &runs[3..]];
        let text = read(&parts, &mut ReadReport::default()).unwrap();
        assert_eq!(text.text, "hi");
        assert_eq!(text.font, Some(5));
    }

    #[test]
    fn malformed_runs_keep_text_without_a_font() {
        let mut head = [0u8; 18];
        head[10..12].copy_from_slice(&2u16.to_le_bytes());
        head[12..14].copy_from_slice(&16u16.to_le_bytes());
        let runs = [0u8; 8];
        let text = read(&[&head, &[0, b'h', b'i'], &runs], &mut ReadReport::default()).unwrap();
        assert_eq!(text.text, "hi");
        assert_eq!(text.font, None);
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

    #[test]
    fn the_runs_are_cut_off_by_their_declared_length() {
        // Two characters, then a 16-byte run part whose bytes look like text: `cchText` says
        // three, so without the cut the run's first byte would be read as a character.
        let runs = [0u8; 16];
        let parts: [&[u8]; 2] = [&[0, b'h', b'i'], &runs];
        assert_eq!(text_parts(&parts, 16), &parts[..1]);
        let (s, complete) = text(text_parts(&parts, 16), 3);
        assert_eq!(s, "hi");
        assert!(!complete);
    }

    #[test]
    fn a_run_length_that_does_not_add_up_keeps_every_part() {
        // A damaged `cbRuns` matches no suffix: the text reader still stops at the first part
        // that holds no whole character, so the runs are not read as text.
        let parts: [&[u8]; 2] = [&[0, b'o', b'k'], &[0u8; 16]];
        assert_eq!(text_parts(&parts, 10), &parts[..]);
        let (s, _) = text(text_parts(&parts, 10), 2);
        assert_eq!(s, "ok");
    }
}
