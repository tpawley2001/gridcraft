//! Worksheet data validation from [MS-XLS] 2.4.82 (DV).

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_formula::Expr;
use gridcraft_model::{CfOperator, ErrorStyle, Validation, ValidationKind};

use super::{Budget, Cursor, Links, Mode, SheetState, decode, invalid};
use crate::{IoError, ReadReport};

pub(super) fn read(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>, budget: &mut Budget, report: &mut ReadReport) -> Result<(), IoError> {
    let v = match parse(c, &st.g.links) {
        Ok(v) => v,
        Err(e) => {
            report.warn(format!("Data validation on sheet '{}' was skipped: {e}", st.sheet.name));
            return Ok(());
        }
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

fn parse(c: &mut Cursor<'_, '_>, links: &Links) -> Result<Validation, IoError> {
    let flags = c.u32()?;
    let kind = match flags & 0xF {
        0 => ValidationKind::Any,
        1 => ValidationKind::Whole,
        2 => ValidationKind::Decimal,
        3 => ValidationKind::List,
        4 => ValidationKind::Date,
        5 => ValidationKind::Time,
        6 => ValidationKind::TextLength,
        7 => ValidationKind::Custom,
        _ => return Err(invalid("unknown validation kind")),
    };
    let op = match (flags >> 20) & 0xF {
        0 => CfOperator::Between,
        1 => CfOperator::NotBetween,
        2 => CfOperator::Equal,
        3 => CfOperator::NotEqual,
        4 => CfOperator::Greater,
        5 => CfOperator::Less,
        6 => CfOperator::GreaterOrEqual,
        7 => CfOperator::LessOrEqual,
        _ => return Err(invalid("unknown validation operator")),
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
        let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
        if r0 > r1 || c0 > c1 || c1 >= 256 {
            return Err(invalid("invalid validation range"));
        }
        ranges.push(RangeRef::new(CellRef::new(u32::from(r0), u32::from(c0)), CellRef::new(u32::from(r1), u32::from(c1))));
    }
    let base = ranges.first().ok_or_else(|| invalid("missing validation range"))?.start;
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
    Ok(Validation {
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
    })
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
