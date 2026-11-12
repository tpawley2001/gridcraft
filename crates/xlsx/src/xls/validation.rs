//! Worksheet data validation from [MS-XLS] 2.4.96 (DVal) and 2.4.95 (Dv).
//!
//! As in Excel, a sheet's DVal record gives how many Dv records follow; Dv records with no DVal
//! before them, or past its count, are left out. A Dv record that can't be read is skipped on its
//! own, and the sheet keeps its other rules.

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_formula::Expr;
use gridcraft_model::{CfOperator, ErrorStyle, Validation, ValidationKind};

use super::{Budget, Cursor, Links, Mode, SheetState, decode, invalid};
use crate::{IoError, ReadReport};

/// The most rules a workbook keeps: a rule with no messages costs no text, so a crafted file
/// could otherwise keep millions of them.
const MAX_RULES: usize = 65_536;

const SKIPPED: &str = "Some data validation rules in .xls files could not be read and were skipped.";
const IGNORED: &str = "Some data validation records in .xls files were ignored, as Excel ignores them: they have no DVal record before them, or come after the number of rules it gives.";

/// Data validation state for the whole workbook.
#[derive(Default)]
pub(super) struct Totals {
    /// Rules kept so far, in every sheet.
    rules: usize,
    /// The [`SKIPPED`] and [`IGNORED`] warnings were given.
    skipped_warned: bool,
    ignored_warned: bool,
}

impl Totals {
    fn skipped(&mut self, report: &mut ReadReport) {
        if !self.skipped_warned {
            self.skipped_warned = true;
            report.warn(SKIPPED);
        }
    }
}

/// DVal's count of Dv records (idvMac, the last 4 bytes of its 18). A record too short to hold
/// it allows none. The count is only compared against, never used to size anything.
pub(super) fn dval_count(c: &mut Cursor<'_, '_>) -> u32 {
    c.skip(14).and_then(|()| c.u32()).unwrap_or(0)
}

pub(super) fn read(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>, budget: &mut Budget, report: &mut ReadReport) -> Result<(), IoError> {
    match st.dv_left.as_mut() {
        Some(left) if *left > 0 => *left -= 1,
        _ => {
            if !budget.dv.ignored_warned {
                budget.dv.ignored_warned = true;
                report.warn(IGNORED);
            }
            return Ok(());
        }
    }
    if budget.dv.rules >= MAX_RULES {
        budget.dv.skipped(report);
        return Ok(());
    }
    let Ok(v) = parse(c, &st.g.links) else {
        budget.dv.skipped(report);
        return Ok(());
    };
    // Bound retained ranges across records and worksheets with the existing cell allowance.
    for _ in &v.ranges {
        budget.cell()?;
    }
    for text in [&v.input_title, &v.error_title, &v.input_message, &v.error_message, &v.f1] {
        budget.text(text.len())?;
    }
    if let Some(text) = &v.f2 {
        budget.text(text.len())?;
    }
    budget.dv.rules += 1;
    st.sheet.validations.push(v);
    Ok(())
}

fn string(c: &mut Cursor<'_, '_>) -> Result<String, IoError> {
    let text = c.string()?;
    Ok(if text == "\0" { String::new() } else { text })
}

fn tokens(c: &mut Cursor<'_, '_>) -> Result<Vec<u8>, IoError> {
    let len = usize::from(c.u16()?);
    c.skip(2)?;
    if len > c.remaining() {
        return Err(invalid("validation formula runs past the record"));
    }
    // The length is a u16 and is checked before allocation.
    (0..len).map(|_| c.u8()).collect()
}

fn formula(tokens: &[u8], base: CellRef, links: &Links) -> Result<Expr, IoError> {
    decode(tokens, &[], base, Mode::Cell, links).map_err(|e| invalid(format!("unsupported validation formula: {}", e.0)))
}

/// The last column of the BIFF8 grid (IV).
const LAST_COL: u16 = 0xFF;

fn parse(c: &mut Cursor<'_, '_>, links: &Links) -> Result<Validation, IoError> {
    let flags = c.u32()?;
    // Excel reads a type it doesn't know as "any value", with no formulas.
    let kind = match flags & 0xF {
        1 => Some(ValidationKind::Whole),
        2 => Some(ValidationKind::Decimal),
        3 => Some(ValidationKind::List),
        4 => Some(ValidationKind::Date),
        5 => Some(ValidationKind::Time),
        6 => Some(ValidationKind::TextLength),
        7 => Some(ValidationKind::Custom),
        0 => Some(ValidationKind::Any),
        _ => None,
    };
    // Only comparisons use the operator; the other types read as "between", as in `.xlsx`.
    let op = match kind {
        Some(ValidationKind::Whole | ValidationKind::Decimal | ValidationKind::Date | ValidationKind::Time | ValidationKind::TextLength) => {
            match (flags >> 20) & 0xF {
                0 => CfOperator::Between,
                1 => CfOperator::NotBetween,
                2 => CfOperator::Equal,
                3 => CfOperator::NotEqual,
                4 => CfOperator::Greater,
                5 => CfOperator::Less,
                6 => CfOperator::GreaterOrEqual,
                7 => CfOperator::LessOrEqual,
                _ => return Err(invalid("unknown validation operator")),
            }
        }
        _ => CfOperator::Between,
    };
    let error_style = match (flags >> 4) & 7 {
        0 => ErrorStyle::Stop,
        1 => ErrorStyle::Warning,
        2 => ErrorStyle::Information,
        _ => return Err(invalid("unknown validation error style")),
    };
    let input_title = string(c)?;
    let error_title = string(c)?;
    let input_message = string(c)?;
    let error_message = string(c)?;
    let first = tokens(c)?;
    let second = tokens(c)?;
    let count = usize::from(c.u16()?);
    if count == 0 || count > c.remaining() / 8 {
        return Err(invalid("missing or truncated validation ranges"));
    }
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u16()?.min(LAST_COL), c.u16()?.min(LAST_COL));
        if r0 > r1 || c0 > c1 {
            return Err(invalid("invalid validation range"));
        }
        // Every row (or column) of the BIFF8 grid is a whole column (row), as in formulas.
        let (r0, r1) = if (r0, r1) == (0, u16::MAX) { (0, MAX_ROWS - 1) } else { (u32::from(r0), u32::from(r1)) };
        let (c0, c1) = if (c0, c1) == (0, LAST_COL) { (0, MAX_COLS - 1) } else { (u32::from(c0), u32::from(c1)) };
        ranges.push(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)));
    }
    // Relative references are offsets from the top-left cell of the box around every range.
    let row = ranges.iter().map(|r| r.start.row).min().ok_or_else(|| invalid("missing validation range"))?;
    let col = ranges.iter().map(|r| r.start.col).min().ok_or_else(|| invalid("missing validation range"))?;
    let base = CellRef::new(row, col);
    let Some(kind) = kind else {
        return Ok(rule(
            ValidationKind::Any,
            op,
            String::new(),
            None,
            flags,
            ranges,
            error_style,
            [input_title, error_title, input_message, error_message],
        ));
    };
    let f1 = if first.is_empty() {
        String::new()
    } else {
        let expr = formula(&first, base, links)?;
        if flags & (1 << 7) != 0 {
            let Expr::Text(text) = expr else { return Err(invalid("validation list is not a string")) };
            gridcraft_formula::print(&Expr::Text(text.replace('\0', ",").into()))
        } else {
            let text = gridcraft_formula::print(&expr);
            if kind == ValidationKind::List { format!("={text}") } else { text }
        }
    };
    let f2 = if second.is_empty() { None } else { Some(gridcraft_formula::print(&formula(&second, base, links)?)) };
    Ok(rule(kind, op, f1, f2, flags, ranges, error_style, [input_title, error_title, input_message, error_message]))
}

/// A rule from its parts; `messages` are the input title, error title, input message and error
/// message.
#[allow(clippy::too_many_arguments)]
fn rule(
    kind: ValidationKind,
    op: CfOperator,
    f1: String,
    f2: Option<String>,
    flags: u32,
    ranges: Vec<RangeRef>,
    error_style: ErrorStyle,
    messages: [String; 4],
) -> Validation {
    let [input_title, error_title, input_message, error_message] = messages;
    Validation {
        ranges,
        kind,
        op,
        f1,
        f2,
        allow_blank: flags & (1 << 8) != 0,
        in_cell_dropdown: flags & (1 << 9) == 0,
        input_title,
        input_message,
        show_input: flags & (1 << 18) != 0,
        error_title,
        error_message,
        error_style,
        show_error: flags & (1 << 19) != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(flags: u32, first: &[u8], second: &[u8]) -> Vec<u8> {
        let mut bytes = flags.to_le_bytes().to_vec();
        for _ in 0..4 {
            bytes.extend_from_slice(&[1, 0, 0, 0]);
        }
        for formula in [first, second] {
            bytes.extend_from_slice(&(formula.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);
            bytes.extend_from_slice(formula);
        }
        // One range, C6:D8.
        for n in [1u16, 5, 7, 2, 3] {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        bytes
    }

    fn read_payload(bytes: &[u8]) -> Result<Validation, IoError> {
        parse(&mut Cursor::new(&[bytes]), &Links::default())
    }

    #[test]
    fn every_truncated_prefix_is_rejected() {
        let bytes = payload(1, &[0x1E, 1, 0], &[0x1E, 9, 0]);
        assert!(read_payload(&bytes).is_ok());
        for end in 0..bytes.len() {
            assert!(read_payload(&bytes[..end]).is_err(), "prefix length {end}");
        }
    }

    #[test]
    fn unsupported_formulas_in_either_slot_are_rejected() {
        for (first, second) in [(&[0xFF][..], &[][..]), (&[0x1E, 1, 0][..], &[0xFF][..])] {
            assert!(read_payload(&payload(1, first, second)).is_err());
        }
    }

    #[test]
    fn date_time_any_and_relative_reference() {
        for (flags, kind) in [(0, ValidationKind::Any), (4, ValidationKind::Date), (5, ValidationKind::Time)] {
            let v = read_payload(&payload(flags, &[], &[])).unwrap();
            assert_eq!(v.kind, kind);
            assert_eq!((v.f1.as_str(), v.f2), ("", None));
        }
        // PtgRefN: relative row +1, column -1, based on C6.
        let v = read_payload(&payload(7, &[0x4C, 1, 0, 0xFF, 0xFF], &[])).unwrap();
        assert_eq!(v.f1, "B7");
    }
}
