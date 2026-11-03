//! SYLK text import and export, including cached values and R1C1 formulas.

use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, MAX_COLS, MAX_ROWS, Value, col_to_letters};
use gridcraft_model::{Formula, NumFmt, Sheet, Workbook};

use crate::{IoError, ReadReport};

/// Whether the bytes start with an `ID;` record, optionally after a UTF-8 BOM.
pub fn is_sylk(bytes: &[u8]) -> bool {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes).starts_with(b"ID;")
}

/// Iterates fields without allocating a collection for a hostile record of delimiters.
fn fields(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut chars = text.chars().peekable();
    std::iter::from_fn(move || {
        chars.peek()?;
        let mut field = String::new();
        while let Some(ch) = chars.next() {
            if ch == ';' {
                if chars.peek() == Some(&';') {
                    chars.next();
                } else {
                    break;
                }
            }
            field.push(ch);
        }
        Some(field)
    })
}

/// Reads a SYLK file into a one-sheet workbook.
pub fn read_sylk(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    if bytes.len() > crate::csv::MAX_CSV_BYTES {
        return Err(IoError::TooLarge("SYLK exceeds 512 MiB".into()));
    }
    if !is_sylk(bytes) {
        return Err(IoError::Format("missing SYLK ID record".into()));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map(str::to_owned).unwrap_or_else(|_| crate::csv::decode_codepage(bytes, 1252));
    let mut wb = Workbook::new();
    let mut sheet = Sheet::new("Sheet1");
    let mut report = ReadReport::default();
    let mut pictures = Vec::new();
    let (mut row, mut col) = (Some(0), Some(0));
    // Repeated width spans must not turn a small file into unbounded expansion work.
    let mut width_assignments = 0usize;
    for line in text.lines() {
        let (kind, rest) = line.split_once(';').unwrap_or((line, ""));
        if kind == "E" {
            break;
        }
        if kind == "P" {
            for field in fields(rest) {
                if let Some(code) = field.strip_prefix('P') {
                    pictures.push(NumFmt::new(code));
                }
            }
            continue;
        }
        if !matches!(kind, "C" | "F") {
            continue;
        }
        let (mut has_y, mut has_x) = (false, false);
        let (mut value, mut formula, mut picture) = (None, None, None);
        for field in fields(rest) {
            let Some(tag) = field.chars().next() else { continue };
            let body = field.get(tag.len_utf8()..).unwrap_or_default();
            match tag {
                'Y' | 'X' => {
                    let limit = if tag == 'Y' { MAX_ROWS } else { MAX_COLS };
                    let index = body.parse::<u32>().ok().and_then(|n| n.checked_sub(1)).filter(|&n| n < limit);
                    if index.is_none() {
                        report.warn("SYLK positions outside the sheet grid were skipped");
                    }
                    if tag == 'Y' {
                        row = index;
                        has_y = true;
                    } else {
                        col = index;
                        has_x = true;
                    }
                }
                'K' if kind == "C" => value = Some(read_value(body)),
                'E' if kind == "C" => formula = Some(body.to_owned()),
                'P' if kind == "F" => picture = body.parse::<usize>().ok(),
                'W' if kind == "F" => {
                    let mut parts = body.split_whitespace();
                    let first = parts.next().and_then(|s| s.parse::<u32>().ok());
                    let last = parts.next().and_then(|s| s.parse::<u32>().ok());
                    let width = parts.next().and_then(|s| s.parse::<f64>().ok());
                    if let (Some(first), Some(last), Some(width)) = (first, last, width) {
                        if first == 0 || last > MAX_COLS {
                            report.warn("SYLK positions outside the sheet grid were skipped");
                            continue;
                        }
                        if first > last || !width.is_finite() || !(0.0..=255.0).contains(&width) {
                            continue;
                        }
                        width_assignments = width_assignments.saturating_add((last - first + 1) as usize);
                        if width_assignments > 1_000_000 {
                            return Err(IoError::TooLarge("too many SYLK column width assignments".into()));
                        }
                        for c in first - 1..last {
                            sheet.cols.entry(c).or_default().size = Some((if width < 1.0 { width * 12.0 } else { width * 7.0 + 5.0 }) as f32);
                        }
                    }
                }
                _ => {}
            }
        }
        let (Some(row), Some(col)) = (row, col) else { continue };
        let at = CellRef::new(row, col);
        if kind == "C" {
            let mut cell = sheet.cell(at).cloned().unwrap_or_default();
            if let Some(value) = value {
                cell.value = value;
            }
            cell.formula = formula.and_then(|text| {
                let converted = r1c1_to_a1(&text, at);
                let parsed = converted.as_deref().and_then(|s| gridcraft_formula::parse(s).ok());
                match (converted, parsed) {
                    (Some(text), Some(expr)) => Some(Arc::new(Formula { text, expr: Some(Arc::new(expr)), array: None, dynamic: false })),
                    _ => {
                        report.warn("Some SYLK formulas could not be imported; their cached values were kept");
                        None
                    }
                }
            });
            sheet.cells.set(at, cell);
        } else if has_y
            && has_x
            && let Some(code) = picture.and_then(|p| pictures.get(p))
        {
            let id = wb.styles.derive(sheet.style_id(at), |s| s.num_fmt = code.clone());
            sheet.set_style(at, id);
        }
    }
    wb.sheets = vec![Arc::new(sheet)];
    Ok((wb, report))
}

fn read_value(text: &str) -> Value {
    if let Some(body) = text.strip_prefix('"') {
        return body.rsplit_once('"').map_or(Value::Empty, |(s, _)| Value::text(s));
    }
    match text {
        "TRUE" => Value::Bool(true),
        "FALSE" => Value::Bool(false),
        _ => CellError::parse(text).map(Value::Error).or_else(|| text.parse::<f64>().ok().map(Value::number)).unwrap_or_default(),
    }
}

/// Reads an optional R1C1 axis. The flag denotes an absolute coordinate.
fn axis(text: &str, cursor: &mut usize) -> Option<(i64, bool)> {
    let rest = text.get(*cursor..)?;
    if let Some(relative) = rest.strip_prefix('[') {
        let end = relative.find(']')?;
        let n = relative.get(..end)?.parse::<i64>().ok()?;
        *cursor += end + 2;
        Some((n, false))
    } else {
        let len = rest.bytes().take_while(u8::is_ascii_digit).count();
        *cursor += len;
        if len == 0 { Some((0, false)) } else { Some((rest.get(..len)?.parse().ok()?, true)) }
    }
}

fn identifier(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '.')
}

/// A SYLK whole-axis reference has no second axis. A single axis denotes a whole row/column.
fn read_whole_axis(text: &str) -> Option<(usize, String)> {
    let axis = text.chars().next()?;
    let mut cursor = 1;
    let (first, absolute) = self::axis(text, &mut cursor)?;
    if !absolute {
        return None;
    }
    let last = if text.get(cursor..)?.starts_with(&format!(":{axis}")) {
        cursor += 2;
        let (last, absolute) = self::axis(text, &mut cursor)?;
        if !absolute {
            return None;
        }
        last
    } else {
        first
    };
    if text.get(cursor..)?.chars().next().is_some_and(|c| identifier(c) || matches!(c, '(' | '!')) {
        return None;
    }
    let limit = if axis == 'C' { MAX_COLS } else { MAX_ROWS };
    if !(1..=i64::from(limit)).contains(&first) || !(1..=i64::from(limit)).contains(&last) {
        return None;
    }
    let label =
        |n: i64| -> Option<String> { Some(if axis == 'C' { format!("${}", col_to_letters(u32::try_from(n - 1).ok()?)) } else { format!("${n}") }) };
    Some((cursor, format!("{}:{}", label(first)?, label(last)?)))
}

/// Converts R1C1 references while preserving quoted strings, sheet names and identifiers.
pub(crate) fn r1c1_to_a1(formula: &str, at: CellRef) -> Option<String> {
    let mut out = String::new();
    let mut cursor = 0;
    let mut quote = None;
    let mut previous = None;
    while let Some(rest) = formula.get(cursor..).filter(|s| !s.is_empty()) {
        let ch = rest.chars().next()?;
        if let Some(q) = quote {
            out.push(ch);
            cursor += ch.len_utf8();
            if ch == q {
                quote = None;
            }
        } else if matches!(ch, '"' | '\'') {
            quote = Some(ch);
            out.push(ch);
            cursor += ch.len_utf8();
        } else if matches!(ch, 'R' | 'C')
            && !previous.is_some_and(identifier)
            && let Some((len, range)) = read_whole_axis(rest)
        {
            out.push_str(&range);
            cursor += len;
        } else if ch == 'R' && !previous.is_some_and(identifier) {
            let mut end = cursor + 1;
            let candidate = axis(formula, &mut end).and_then(|r| {
                if formula.as_bytes().get(end) != Some(&b'C') {
                    return None;
                }
                end += 1;
                axis(formula, &mut end).map(|c| (r, c))
            });
            let next = formula.get(end..)?.chars().next();
            if let Some(((r, ra), (c, ca))) = candidate.filter(|_| !next.is_some_and(|c| identifier(c) || matches!(c, '(' | '!'))) {
                let row = if ra { r.checked_sub(1)? } else { i64::from(at.row).checked_add(r)? };
                let col = if ca { c.checked_sub(1)? } else { i64::from(at.col).checked_add(c)? };
                if !(0..i64::from(MAX_ROWS)).contains(&row) || !(0..i64::from(MAX_COLS)).contains(&col) {
                    return None;
                }
                if ca {
                    out.push('$');
                }
                out.push_str(&col_to_letters(u32::try_from(col).ok()?));
                if ra {
                    out.push('$');
                }
                out.push_str(&(row + 1).to_string());
                cursor = end;
            } else {
                out.push(ch);
                cursor += ch.len_utf8();
            }
        } else {
            out.push(ch);
            cursor += ch.len_utf8();
        }
        previous = Some(ch);
    }
    if quote.is_some() { None } else { Some(out) }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn encoding_end_marker_and_format_position() {
        let (wb, report) = read_sylk(b"\xEF\xBB\xBFID;P\nP;Fignored\nP;P0.00\nF;P0;Y2;X3\nC;K2.5\nC;X1;K\"caf\xe9 \x80\"\nE\nC;K99\n").unwrap();
        assert!(report.warnings.is_empty());
        let sh = &wb.sheets[0];
        assert_eq!(sh.value(CellRef::new(1, 2)), Value::Number(2.5));
        assert_eq!(wb.styles.get(sh.style_id(CellRef::new(1, 2))).num_fmt.as_str(), "0.00");
        assert_eq!(sh.value(CellRef::new(1, 0)), Value::text("café €"));
    }

    #[test]
    fn invalid_formulas_and_positions_warn_once_each() {
        let (wb, report) = read_sylk(b"ID;P\nC;Y1;X1;K4;ER[-1]C\nC;X2;K5;E)(\nC;Y99999999999999999;X1;K9\nC;Y0;K8\nE\n").unwrap();
        assert_eq!(report.warnings.len(), 2);
        let sh = &wb.sheets[0];
        assert_eq!(sh.value(CellRef::new(0, 0)), Value::Number(4.0));
        assert_eq!(sh.value(CellRef::new(0, 1)), Value::Number(5.0));
        assert!(sh.cell(CellRef::new(0, 0)).unwrap().formula.is_none());
    }

    #[test]
    fn reference_boundaries_and_quoted_sheet_names() {
        let at = CellRef::new(2, 2);
        for text in ["MYRC+RC_NAME+RC.foo+RC(1)", "'R1C1'!R1C1", "R1C1!RC", "\"a\"\"R1C1\"&RC"] {
            let converted = r1c1_to_a1(text, at).unwrap();
            match text {
                "'R1C1'!R1C1" => assert_eq!(converted, "'R1C1'!$A$1"),
                "R1C1!RC" => assert_eq!(converted, "R1C1!C3"),
                "\"a\"\"R1C1\"&RC" => assert_eq!(converted, "\"a\"\"R1C1\"&C3"),
                _ => assert_eq!(converted, text),
            }
        }
        assert!(r1c1_to_a1("R[-3]C", at).is_none());
        assert!(r1c1_to_a1("R1C16385", at).is_none());
    }
}

/// Writes one sheet as SYLK, using the workbook's number format table.
/// Output is UTF-8, bounded like CSV, and deliberately excludes other sheet features.
pub fn write_sylk(wb: &Workbook, sheet: usize) -> Result<Vec<u8>, IoError> {
    let sh = wb.sheet(sheet).ok_or_else(|| IoError::Format("missing SYLK sheet".into()))?;
    let mut out = String::from("ID;PGRIDCRAFT;N;E\r\nP;PGeneral\r\n");
    let mut formats = std::collections::HashMap::from([(String::from("General"), 0usize)]);
    for (at, cell) in sh.cells.iter() {
        if !at.is_valid() || (cell.value == Value::Empty && cell.formula.is_none()) {
            continue;
        }
        let code = wb.styles.get(sh.style_id(at)).num_fmt.as_str();
        if !formats.contains_key(code) {
            let index = formats.len();
            formats.insert(code.to_owned(), index);
            append(&mut out, "P;P")?;
            field(&mut out, code)?;
            append(&mut out, "\r\n")?;
        }
    }
    for (&col, info) in &sh.cols {
        if col >= MAX_COLS {
            continue;
        }
        if let Some(size) = info.size.filter(|s| s.is_finite() && *s != sh.default_col_width) {
            let width = if size < 12.0 { size / 12.0 } else { (size - 5.0) / 7.0 };
            append(&mut out, &format!("F;W{} {} {}\r\n", col + 1, col + 1, width.round().clamp(0.0, 255.0)))?;
        }
    }
    for (at, cell) in sh.cells.iter() {
        if !at.is_valid() || (cell.value == Value::Empty && cell.formula.is_none()) {
            continue;
        }
        let code = wb.styles.get(sh.style_id(at)).num_fmt.as_str();
        let index = formats.get(code).copied().unwrap_or_default();
        if index != 0 {
            append(&mut out, &format!("F;P{index};Y{};X{}\r\n", at.row + 1, at.col + 1))?;
        }
        append(&mut out, &format!("C;Y{};X{};K", at.row + 1, at.col + 1))?;
        match &cell.value {
            Value::Empty => {}
            Value::Number(n) => append(&mut out, &n.to_string())?,
            Value::Text(t) => {
                append(&mut out, "\"")?;
                field(&mut out, t)?;
                append(&mut out, "\"")?;
            }
            Value::Bool(b) => append(&mut out, if *b { "TRUE" } else { "FALSE" })?,
            Value::Error(e) => append(&mut out, e.as_str())?,
            Value::Array(_) => return Err(IoError::Format("SYLK cannot store an array as a cell value".into())),
        }
        if let Some(formula) = cell.formula.as_ref().and_then(|f| a1_to_r1c1(&f.text, at)) {
            append(&mut out, ";E")?;
            field(&mut out, &formula)?;
        }
        append(&mut out, "\r\n")?;
    }
    append(&mut out, "E\r\n")?;
    Ok(out.into_bytes())
}

fn append(out: &mut String, text: &str) -> Result<(), IoError> {
    if out.len().saturating_add(text.len()) > crate::csv::MAX_CSV_BYTES {
        return Err(IoError::TooLarge("SYLK exceeds 512 MiB".into()));
    }
    out.push_str(text);
    Ok(())
}

/// Escape field delimiters and flatten line breaks without allocating a second copy.
fn field(out: &mut String, text: &str) -> Result<(), IoError> {
    for ch in text.chars() {
        match ch {
            ';' => append(out, ";;")?,
            '\r' | '\n' => append(out, " ")?,
            _ => append(out, ch.encode_utf8(&mut [0; 4]))?,
        }
    }
    Ok(())
}

fn relative_axis(axis: char, n: u32, absolute: bool, origin: u32) -> String {
    if absolute {
        format!("{axis}{}", n + 1)
    } else {
        let offset = i64::from(n) - i64::from(origin);
        if offset == 0 { axis.to_string() } else { format!("{axis}[{offset}]") }
    }
}

/// Converts A1 references, preserving quoted strings, sheet names and identifiers.
/// Unsupported reference syntax leaves the writer free to export just the cached value.
pub(crate) fn a1_to_r1c1(formula: &str, at: CellRef) -> Option<String> {
    if !at.is_valid() {
        return None;
    }
    let formula = formula.strip_prefix('=').unwrap_or(formula);
    // Also bounds malformed/nested input through the formula parser's normal limits.
    gridcraft_formula::parse(formula).ok()?;
    let mut out = String::new();
    let mut cursor = 0;
    let mut quote = None;
    let mut previous = None;
    while let Some(rest) = formula.get(cursor..).filter(|s| !s.is_empty()) {
        let ch = rest.chars().next()?;
        if let Some(q) = quote {
            out.push(ch);
            cursor += ch.len_utf8();
            if ch == q {
                quote = None;
            }
        } else if matches!(ch, '\"' | '\'') {
            quote = Some(ch);
            out.push(ch);
            cursor += ch.len_utf8();
        } else if matches!(ch, '[' | ']' | '{' | '}' | '\r' | '\n') {
            return None;
        } else if !previous.is_some_and(identifier) {
            if let Some((col, row, ca, ra, tail)) = gridcraft_core::addr::parse_a1_prefix(rest)
                && !tail.chars().next().is_some_and(|c| identifier(c) || matches!(c, '(' | '!'))
            {
                out.push_str(&relative_axis('R', row, ra, at.row));
                out.push_str(&relative_axis('C', col, ca, at.col));
                cursor += rest.len() - tail.len();
            } else if let Some((len, converted)) = whole_axis_range(rest) {
                out.push_str(&converted);
                cursor += len;
            } else {
                if ch == '$' {
                    return None;
                }
                out.push(ch);
                cursor += ch.len_utf8();
            }
        } else {
            out.push(ch);
            cursor += ch.len_utf8();
        }
        previous = Some(ch);
    }
    if quote.is_some() { None } else { Some(out) }
}

/// Whole axes use absolute SYLK coordinates; an equal-ended range needs only one axis.
fn whole_axis_range(text: &str) -> Option<(usize, String)> {
    fn endpoint(text: &str) -> Option<(usize, char, u32)> {
        let rest = text.strip_prefix('$').unwrap_or(text);
        let letters = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        let (len, axis, n) = if letters > 0 {
            (letters, 'C', gridcraft_core::letters_to_col(rest.get(..letters)?)?.checked_add(1)?)
        } else {
            let len = rest.bytes().take_while(u8::is_ascii_digit).count();
            let n = rest.get(..len)?.parse::<u32>().ok().filter(|n| (1..=MAX_ROWS).contains(n))?;
            (len, 'R', n)
        };
        Some((text.len() - rest.len() + len, axis, n))
    }
    let (first, axis, n) = endpoint(text)?;
    let tail = text.get(first..)?.strip_prefix(':')?;
    let (last, other, m) = endpoint(tail)?;
    if axis != other || tail.get(last..)?.chars().next().is_some_and(|c| identifier(c) || matches!(c, '(' | '!')) {
        return None;
    }
    Some((first + 1 + last, if n == m { format!("{axis}{n}") } else { format!("{axis}{n}:{axis}{m}") }))
}

#[cfg(test)]
mod write_additional_tests {
    use super::*;

    #[test]
    fn whole_axes_convert_and_round_trip() {
        let at = CellRef::new(4, 2);
        for (a1, r1c1, canonical) in [
            ("SUM(A:A)", "SUM(C1)", "SUM($A:$A)"),
            ("SUM(A:C)", "SUM(C1:C3)", "SUM($A:$C)"),
            ("SUM(1:1)", "SUM(R1)", "SUM($1:$1)"),
            ("SUM($2:$5)", "SUM(R2:R5)", "SUM($2:$5)"),
        ] {
            assert_eq!(a1_to_r1c1(a1, at).as_deref(), Some(r1c1));
            assert_eq!(r1c1_to_a1(r1c1, at).as_deref(), Some(canonical));
            let bytes = format!("ID;P\r\nC;Y5;X3;K42;E{r1c1}\r\nE\r\n");
            let (wb, report) = read_sylk(bytes.as_bytes()).unwrap();
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            let (back, report) = read_sylk(&write_sylk(&wb, 0).unwrap()).unwrap();
            assert!(report.warnings.is_empty());
            assert_eq!(back.sheet(0).unwrap().cell(at).unwrap().formula.as_ref().unwrap().text, canonical);
        }
    }

    #[test]
    fn text_delimiters_quotes_and_line_breaks() {
        let mut wb = Workbook::new();
        wb.sheet_mut(0).unwrap().set_value(CellRef::new(0, 0), Value::text("a;\"quoted\"\r\nb\nc"));
        let bytes = write_sylk(&wb, 0).unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("K\"a;;\"quoted\"  b c\"\r\n"));
        let (back, _) = read_sylk(&bytes).unwrap();
        assert_eq!(back.sheet(0).unwrap().value(CellRef::new(0, 0)), Value::text("a;\"quoted\"  b c"));
    }

    #[test]
    fn unsupported_formula_keeps_cached_value() {
        let mut wb = Workbook::new();
        let at = CellRef::new(0, 0);
        let mut cell = gridcraft_model::Cell::value(Value::Number(7.0));
        cell.formula = Some(Arc::new(Formula { text: "invalid(".into(), expr: None, array: None, dynamic: false }));
        wb.sheet_mut(0).unwrap().cells.set(at, cell);
        let bytes = write_sylk(&wb, 0).unwrap();
        assert!(!std::str::from_utf8(&bytes).unwrap().lines().any(|line| line.starts_with("C;") && line.contains(";E")));
        let (back, _) = read_sylk(&bytes).unwrap();
        assert_eq!(back.sheet(0).unwrap().value(at), Value::Number(7.0));
        assert!(back.sheet(0).unwrap().cell(at).unwrap().formula.is_none());
        assert!(a1_to_r1c1("SUM(Table1[Value])", at).is_none());
    }
}
