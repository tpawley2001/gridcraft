//! DIF table import and export: typed values without workbook formatting.

use std::borrow::Cow;

use gridcraft_core::{CellError, CellRef, MAX_COLS, MAX_ROWS, Value};
use gridcraft_model::{Sheet, Workbook};

use crate::{IoError, MAX_CSV_BYTES, ReadReport};

fn malformed(detail: &str) -> IoError {
    IoError::Format(format!("invalid DIF: {detail}"))
}

fn too_large() -> IoError {
    IoError::TooLarge("DIF exceeds the 512 MiB text-file limit".into())
}

/// Reads a DIF file into a one-sheet workbook. Header dimensions are advisory only.
pub fn read_dif(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    if bytes.len() > MAX_CSV_BYTES {
        return Err(too_large());
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(crate::csv::decode_codepage(bytes, 1252)),
    };
    let mut lines = text.trim_start_matches('\u{feff}');
    loop {
        let topic = next_line(&mut lines).ok_or_else(|| malformed("missing DATA header"))?;
        let _numbers = next_line(&mut lines).ok_or_else(|| malformed("incomplete header"))?;
        let _string = next_line(&mut lines).ok_or_else(|| malformed("incomplete header"))?;
        if topic.trim() == "DATA" {
            break;
        }
    }
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).ok_or_else(|| malformed("missing sheet"))?;
    let mut report = ReadReport::default();
    let mut row = 0u32;
    let mut col = 0u32;
    loop {
        let pair = next_line(&mut lines).ok_or_else(|| malformed("missing EOD marker"))?;
        let (kind, number) = pair.split_once(',').ok_or_else(|| malformed("expected type,number"))?;
        let text;
        let string = if kind.trim() == "1" {
            text = read_text(&mut lines)?;
            text.as_str()
        } else {
            next_line(&mut lines).ok_or_else(|| malformed("incomplete value"))?
        };
        match (kind.trim(), string.trim()) {
            ("-1", "EOD") => break,
            ("-1", "BOT") => {
                row = row.saturating_add(1);
                col = 0;
                if row > MAX_ROWS {
                    report.warn("DIF cells outside the 1,048,576 × 16,384 grid were dropped");
                }
                continue;
            }
            _ => {}
        }
        let value = match kind.trim() {
            "0" => match string.trim() {
                "TRUE" => Value::Bool(true),
                "FALSE" => Value::Bool(false),
                "NA" => Value::Error(CellError::NA),
                "ERROR" => Value::Error(CellError::Value),
                "V" => {
                    let n = number.trim().parse::<f64>().map_err(|_| malformed("invalid number"))?;
                    Value::number(n)
                }
                _ => return Err(malformed("unknown numeric value marker")),
            },
            "1" => {
                if string.is_empty() {
                    Value::Empty
                } else {
                    Value::from(string)
                }
            }
            _ => return Err(malformed("unknown value type")),
        };
        if row == 0 {
            return Err(malformed("value before BOT marker"));
        }
        col = col.saturating_add(1);
        if row > MAX_ROWS || col > MAX_COLS {
            report.warn("DIF cells outside the 1,048,576 × 16,384 grid were dropped");
        } else if !value.is_empty() {
            sheet.set_value(CellRef::new(row - 1, col - 1), value);
        }
    }
    Ok((wb, report))
}

fn next_line<'a>(input: &mut &'a str) -> Option<&'a str> {
    if input.is_empty() {
        return None;
    }
    let (line, rest) = input.split_once('\n').unwrap_or((input, ""));
    *input = rest;
    Some(line.strip_suffix('\r').unwrap_or(line))
}

/// Quoted text may span lines; only record separators discard their CRLF.
fn read_text(lines: &mut &str) -> Result<String, IoError> {
    let remaining = *lines;
    let Some(quoted) = remaining.strip_prefix('"') else {
        return next_line(lines).map(str::to_owned).ok_or_else(|| malformed("incomplete text value"));
    };
    let mut chars = quoted.char_indices().peekable();
    let mut text = String::new();
    while let Some((i, c)) = chars.next() {
        if c != '"' {
            text.push(c);
        } else if chars.peek().is_some_and(|(_, next)| *next == '"') {
            chars.next();
            text.push('"');
        } else {
            let rest = quoted.get(i + 1..).ok_or_else(|| malformed("invalid text boundary"))?;
            let rest = if rest.is_empty() {
                rest
            } else {
                rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n')).ok_or_else(|| malformed("characters after closing quote"))?
            };
            *lines = rest;
            return Ok(text);
        }
    }
    Err(malformed("unclosed text"))
}

fn cp1252_byte(c: char) -> Option<u8> {
    let code = u32::from(c);
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        u8::try_from(code).ok()
    } else {
        crate::csv::CP1252.iter().position(|&n| n == code).and_then(|i| u8::try_from(i).ok()).map(|i| i + 0x80)
    }
}

/// A bounded output buffer, using one encoding for the entire file.
struct Output {
    bytes: Vec<u8>,
    utf8: bool,
}

impl Output {
    fn push(&mut self, text: &str) -> Result<(), IoError> {
        let len = if self.utf8 { text.len() } else { text.chars().count() };
        if len > MAX_CSV_BYTES.saturating_sub(self.bytes.len()) {
            return Err(too_large());
        }
        if self.utf8 {
            self.bytes.extend_from_slice(text.as_bytes());
        } else {
            for c in text.chars() {
                self.bytes.push(cp1252_byte(c).ok_or_else(|| malformed("text cannot be encoded as Windows-1252"))?);
            }
        }
        Ok(())
    }
}

/// Writes the active sheet's used rectangle from A1, using cached formula values.
pub fn write_dif(sheet: &Sheet) -> Result<Vec<u8>, IoError> {
    let (rows, cols) = sheet.used_range().map_or((0, 0), |r| (r.end.row.saturating_add(1), r.end.col.saturating_add(1)));
    // Every value takes at least eight bytes. Refuse huge rectangles before expanding.
    let minimum = u64::from(rows).saturating_mul(u64::from(cols)).saturating_mul(8).saturating_add(u64::from(rows).saturating_mul(11));
    if minimum > MAX_CSV_BYTES as u64 {
        return Err(too_large());
    }
    let utf8 = sheet
        .cells
        .iter()
        .map(|(_, c)| &c.value)
        .chain(sheet.spill.values())
        .any(|v| matches!(v, Value::Text(text) if text.chars().any(|c| cp1252_byte(c).is_none())));
    let mut out = Output { bytes: Vec::new(), utf8 };
    out.push(&format!("TABLE\r\n0,1\r\n\"\"\r\nVECTORS\r\n0,{cols}\r\n\"\"\r\nTUPLES\r\n0,{rows}\r\n\"\"\r\nDATA\r\n0,0\r\n\"\"\r\n"))?;
    for row in 0..rows {
        out.push("-1,0\r\nBOT\r\n")?;
        for col in 0..cols {
            match sheet.value_ref(CellRef::new(row, col)).unwrap_or(&Value::Empty) {
                Value::Number(n) => out.push(&format!("0,{n}\r\nV\r\n"))?,
                Value::Bool(true) => out.push("0,1\r\nTRUE\r\n")?,
                Value::Bool(false) => out.push("0,0\r\nFALSE\r\n")?,
                Value::Error(CellError::NA) => out.push("0,0\r\nNA\r\n")?,
                Value::Error(_) | Value::Array(_) => out.push("0,0\r\nERROR\r\n")?,
                Value::Empty => out.push("1,0\r\n\"\"\r\n")?,
                Value::Text(text) => {
                    out.push("1,0\r\n\"")?;
                    let mut parts = text.split('"');
                    if let Some(first) = parts.next() {
                        out.push(first)?;
                    }
                    for part in parts {
                        out.push("\"\"")?;
                        out.push(part)?;
                    }
                    out.push("\"\r\n")?;
                }
            }
        }
    }
    out.push("-1,0\r\nEOD\r\n")?;
    Ok(out.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings_and_multiline_text_round_trip() {
        for (text, encoded) in [("café €", vec![0xE9, b' ', 0x80]), ("東京 café", "東京 café".as_bytes().to_vec())] {
            let mut sheet = Sheet::new("Sheet1");
            let value = format!("{text}\r\nline \"two\"\nthree");
            sheet.set_value(CellRef::new(1, 1), Value::from(value.clone()));
            let bytes = write_dif(&sheet).unwrap();
            assert!(bytes.windows(encoded.len()).any(|w| w == encoded));
            let (wb, report) = read_dif(&bytes).unwrap();
            assert!(report.warnings.is_empty());
            assert_eq!(wb.sheet(0).unwrap().value(CellRef::new(1, 1)), Value::from(value));
        }
    }

    #[test]
    fn dimensions_are_advisory_and_excess_columns_warn_once() {
        let mut src = String::from("VECTORS\n0,999999999999999999999999\n\"\"\nTUPLES\n0,-1\n\"\"\nDATA\n0,0\n\"\"\n-1,0\nBOT\n");
        for _ in 0..MAX_COLS + 2 {
            src.push_str("1,0\nunquoted\n");
        }
        src.push_str("-1,0\nBOT\n0,42\nV\n-1,0\nEOD\n0,bad\nV\n");
        let (wb, report) = read_dif(src.as_bytes()).unwrap();
        assert_eq!(report.warnings.len(), 1);
        let sheet = wb.sheet(0).unwrap();
        assert_eq!(sheet.value(CellRef::new(0, MAX_COLS - 1)), Value::from("unquoted"));
        assert_eq!(sheet.value(CellRef::new(1, 0)), Value::Number(42.0));
    }

    #[test]
    fn refuses_huge_sparse_output_before_expansion() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.set_value(CellRef::new(MAX_ROWS - 1, MAX_COLS - 1), Value::Bool(true));
        assert!(matches!(write_dif(&sheet), Err(IoError::TooLarge(_))));
    }

    #[test]
    fn malformed_records_return_errors() {
        for data in ["1,0\n\"open\n", "0,no\nV\n", "-1,0\nBOT\n9,9\n?\n", "1,0\n\"ok\"junk\n"] {
            assert!(read_dif(format!("DATA\n0,0\n\"\"\n{data}").as_bytes()).is_err());
        }
    }
}
