//! OpenDocument formulas (`of:=SUM([.A1:.B2];1)`) in Excel's A1 syntax (`SUM(A1:B2,1)`).
//!
//! ODS import keeps a formula only when it translates cleanly; anything this doesn't understand
//! (external links, the `~` union and `!` intersection operators, LibreOffice-only functions)
//! returns `None` and the cell keeps its saved result, as before. Behaviour reference: OASIS
//! OpenDocument v1.3 part 4 (OpenFormula), §5.8 references and §5.3 inline arrays.

use gridcraft_formula::Expr;

/// The longest formula translated (Excel's own limit is 8,192 characters).
const MAX_LEN: usize = 8192;

/// An ODS `table:formula` in Excel syntax, without the leading `=`; `None` when it can't be
/// translated faithfully. Plain Excel-style references (`A1*2`) pass through unchanged.
pub fn to_excel(src: &str) -> Option<String> {
    let body = if let Some(rest) = src.strip_prefix("of:") {
        rest
    } else if let Some(rest) = src.strip_prefix("msoxl:") {
        // Already Excel syntax.
        return Some(rest.strip_prefix('=').unwrap_or(rest).to_string()).filter(|s| s.len() <= MAX_LEN);
    } else if src.starts_with('=') {
        src
    } else {
        // Another namespace (`oooc:` is the old OpenOffice dialect): not translated.
        return None;
    };
    let body = body.strip_prefix('=').unwrap_or(body);
    if body.len() > MAX_LEN {
        return None;
    }
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    // Depth of `{…}` inline arrays: `;` separates columns and `|` rows inside them.
    let mut in_array = false;
    while let Some(&c) = chars.get(i) {
        match c {
            '"' => {
                let end = string_end(&chars, i)?;
                out.extend(chars.get(i..=end)?);
                i = end + 1;
                continue;
            }
            '[' => {
                let end = bracket_end(&chars, i)?;
                let inner: String = chars.get(i + 1..end)?.iter().collect();
                out.push_str(&reference(&inner)?);
                i = end + 1;
                continue;
            }
            '{' => in_array = true,
            '}' => in_array = false,
            ';' => {
                out.push(',');
                i += 1;
                continue;
            }
            '|' if in_array => {
                out.push(';');
                i += 1;
                continue;
            }
            // Union, intersection and anything else Excel spells differently.
            '~' | '!' | '|' | '\'' | '$' if !in_array => return None,
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while chars.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_')) {
                    i += 1;
                }
                let word: String = chars.get(start..i)?.iter().collect();
                if chars.get(i) == Some(&'(') {
                    // OpenFormula's FLOOR/CEILING take an optional third Mode argument; Excel
                    // computes those calls as FLOOR.MATH/CEILING.MATH (its own take two).
                    let upper = word.to_ascii_uppercase();
                    if matches!(upper.as_str(), "FLOOR" | "CEILING") && call_separators(&chars, i)? == 2 {
                        out.push_str(&upper);
                        out.push_str(".MATH");
                    } else {
                        out.push_str(&function_name(&word)?);
                    }
                } else {
                    out.push_str(&word);
                }
                continue;
            }
            _ => {}
        }
        out.push(c);
        i += 1;
    }
    Some(out)
}

/// How many `;` separate the arguments of the call whose `(` is at `open`, counting only the
/// call's own depth (not those inside nested calls, inline arrays, strings or references).
fn call_separators(chars: &[char], open: usize) -> Option<usize> {
    let mut i = open + 1;
    let mut depth = 0usize;
    let mut separators = 0usize;
    loop {
        match chars.get(i)? {
            '"' => i = string_end(chars, i)?,
            '[' => i = bracket_end(chars, i)?,
            '(' | '{' => depth = depth.checked_add(1)?,
            ')' if depth == 0 => return Some(separators),
            ')' | '}' => depth = depth.checked_sub(1)?,
            ';' if depth == 0 => separators = separators.saturating_add(1),
            _ => {}
        }
        i += 1;
    }
}

/// The index of the quote closing the string literal opened at `start` (`""` is an escaped quote).
fn string_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    loop {
        match chars.get(i)? {
            '"' if chars.get(i + 1) == Some(&'"') => i += 2,
            '"' => return Some(i),
            _ => i += 1,
        }
    }
}

/// The index of the `]` closing the reference opened at `start`; quoted sheet names may hold `]`.
fn bracket_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut quoted = false;
    loop {
        match chars.get(i)? {
            '\'' if quoted && chars.get(i + 1) == Some(&'\'') => i += 1,
            '\'' => quoted = !quoted,
            ']' if !quoted => return Some(i),
            '[' if !quoted => return None,
            _ => {}
        }
        i += 1;
    }
}

/// Function names: OpenFormula prefixes the Excel-only ones with `COM.MICROSOFT.` and the old
/// forms with `LEGACY.`; LibreOffice's own (`ORG.…`) have no Excel equivalent.
fn function_name(word: &str) -> Option<String> {
    let upper = word.to_ascii_uppercase();
    if upper.starts_with("ORG.") {
        return None;
    }
    let name = upper.strip_prefix("COM.MICROSOFT.").or_else(|| upper.strip_prefix("LEGACY.")).unwrap_or(&upper);
    Some(name.to_string())
}

/// One reference's inside: `.A1`, `.A1:.B2`, `$Sheet1.A1`, `$'My sheet'.$A$1:.$B$2`,
/// `Sheet1.A1:Sheet2.B2` (a 3-D range).
fn reference(inner: &str) -> Option<String> {
    if inner.contains('#') || inner.contains("://") {
        return None;
    }
    let parts = split_outside_quotes(inner, ':');
    let (first, second) = match parts.as_slice() {
        [a] => (a.as_str(), None),
        [a, b] => (a.as_str(), Some(b.as_str())),
        _ => return None,
    };
    let (sheet1, cell1) = part(first)?;
    let Some(second) = second else {
        return Some(match sheet1 {
            Some(s) => format!("{s}!{cell1}"),
            None => cell1,
        });
    };
    let (sheet2, cell2) = part(second)?;
    Some(match (sheet1, sheet2) {
        (None, None) => format!("{cell1}:{cell2}"),
        (Some(s), None) => format!("{s}!{cell1}:{cell2}"),
        (Some(a), Some(b)) if a == b => format!("{a}!{cell1}:{cell2}"),
        (Some(a), Some(b)) => {
            // Excel's 3-D form names both sheets once, quoted together when either needs it.
            let (a, b) = (a.trim_matches('\''), b.trim_matches('\''));
            let both = format!("{a}:{b}");
            let quoted = if needs_quotes(a) || needs_quotes(b) { format!("'{both}'") } else { both };
            format!("{quoted}!{cell1}:{cell2}")
        }
        (None, Some(_)) => return None,
    })
}

/// A reference part: optional sheet (before the last `.` outside quotes) and the cell, column or
/// row address, with `$` markers kept on the address and dropped from the sheet.
fn part(p: &str) -> Option<(Option<String>, String)> {
    let dot = last_outside_quotes(p, '.')?;
    let (sheet, addr) = (p.get(..dot)?, p.get(dot + 1..)?);
    if addr.is_empty() || !addr.chars().all(|c| c.is_ascii_alphanumeric() || c == '$') {
        return None;
    }
    let sheet = sheet.strip_prefix('$').unwrap_or(sheet);
    if sheet.is_empty() {
        return Some((None, addr.to_string()));
    }
    let sheet = if let Some(q) = sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        format!("'{q}'")
    } else if needs_quotes(sheet) {
        format!("'{}'", sheet.replace('\'', "''"))
    } else {
        sheet.to_string()
    };
    Some((Some(sheet), addr.to_string()))
}

/// A sheet name Excel quotes in a reference: anything beyond letters, digits, `_` and `.`, or one
/// that starts with a digit or could read as a cell address.
fn needs_quotes(name: &str) -> bool {
    let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    let looks_like_cell = gridcraft_core::CellRef::parse(name).is_some();
    !plain || name.starts_with(|c: char| c.is_ascii_digit()) || looks_like_cell
}

fn split_outside_quotes(s: &str, sep: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut quoted = false;
    for c in s.chars() {
        if c == '\'' {
            quoted = !quoted;
        }
        if c == sep && !quoted {
            parts.push(String::new());
        } else if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts
}

fn last_outside_quotes(s: &str, needle: char) -> Option<usize> {
    let mut quoted = false;
    let mut found = None;
    for (i, c) in s.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        } else if c == needle && !quoted {
            found = Some(i);
        }
    }
    found
}

/// Whether a parsed formula only calls functions `known` accepts and only uses names in `names`
/// (upper-case): otherwise recalculating would turn the saved result into `#NAME?`.
pub fn is_supported(e: &Expr, names: &[String], known: &dyn Fn(&str) -> bool) -> bool {
    let mut ok = true;
    e.walk(&mut |n| match n {
        Expr::Call(f, _) if !known(f) => ok = false,
        Expr::Name(name) if !names.iter().any(|k| k.eq_ignore_ascii_case(name)) => ok = false,
        Expr::Invoke(..) | Expr::Struct(_) => ok = false,
        _ => {}
    });
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libreoffice_formulas_become_excel_formulas() {
        for (of, xl) in [
            ("of:=SUM([.B2:.B12])", "SUM(B2:B12)"),
            ("of:=IF([.B5]>[.C5];\"big\";\"small\")", "IF(B5>C5,\"big\",\"small\")"),
            ("of:=[$Formats.B2]+1", "Formats!B2+1"),
            ("of:=[$'Hold manifest'.$A$1:.$B$3]", "'Hold manifest'!$A$1:$B$3"),
            ("of:=SUM([$Sheet1.A1:$Sheet3.A1])", "SUM(Sheet1:Sheet3!A1:A1)"),
            ("of:=SUM([.A:.A])", "SUM(A:A)"),
            ("of:=IRR({-100;60;60})", "IRR({-100,60,60})"),
            ("of:=SUM({1;2|3;4})", "SUM({1,2;3,4})"),
            ("of:=COM.MICROSOFT.XLOOKUP(1;[.A1:.A3];[.B1:.B3])", "XLOOKUP(1,A1:A3,B1:B3)"),
            ("of:=LEGACY.NORMSDIST(1)", "NORMSDIST(1)"),
            ("of:=FLOOR(37; -1; 0)", "FLOOR.MATH(37, -1, 0)"),
            ("of:=CEILING(-37.2;1;0)", "CEILING.MATH(-37.2,1,0)"),
            ("of:=FLOOR(2.5;1;0)", "FLOOR.MATH(2.5,1,0)"),
            ("of:=CEILING(2.5;1)", "CEILING(2.5,1)"),
            ("of:=COM.MICROSOFT.FLOOR(2.5;1)", "FLOOR(2.5,1)"),
            ("of:=COM.MICROSOFT.CEILING(2.5;1)", "CEILING(2.5,1)"),
            ("of:=\"a;b|[.c]\"&[.A1]", "\"a;b|[.c]\"&A1"),
            ("of:=\"say \"\"hi\"\"\"", "\"say \"\"hi\"\"\""),
            ("of:=A1*2", "A1*2"),
            ("msoxl:=SUM(A1,B1)", "SUM(A1,B1)"),
            ("of:=[$'It''s'.A1]", "'It''s'!A1"),
            ("of:=[$1st.A1]", "'1st'!A1"),
        ] {
            assert_eq!(to_excel(of).as_deref(), Some(xl), "{of}");
        }
    }

    #[test]
    fn what_excel_spells_differently_is_left_as_a_value() {
        for of in [
            "of:=SUM([.A1:.A2]~[.C1])",
            "of:=[.A1:.C3]![.B1:.B5]",
            "of:=['file:///x.ods'#$Sheet1.A1]",
            "of:=[.#REF!A1]",
            "of:=ORG.LIBREOFFICE.REGEX(\"a\";\"b\")",
            "oooc:=[.A1]",
            "of:=\"unterminated",
            "of:=[.A1",
            "of:=[.A1:.B2:.C3]",
        ] {
            assert_eq!(to_excel(of), None, "{of}");
        }
        assert_eq!(to_excel(&format!("of:={}", "1+".repeat(5000))), None, "longer than Excel allows");
    }

    #[test]
    fn unknown_functions_and_names_are_not_supported() {
        let names = vec!["TAX".to_string()];
        let known = |f: &str| f != "NOSUCHFUNCTION";
        let ok = |s: &str| gridcraft_formula::parse(s).map(|e| is_supported(&e, &names, &known)).unwrap_or(false);
        assert!(ok("SUM(A1:A3)*Tax"));
        assert!(!ok("Rate*2"), "a name the file didn't define");
        assert!(!ok("NOSUCHFUNCTION(1)"));
    }
}
