//! Excel 4.0 names cannot be stored in a macro-free Open XML workbook.

use std::collections::{HashMap, HashSet};

use gridcraft_formula::lexer::{Tok, tokenize};
use gridcraft_model::Workbook;

// Macro-sheet functions from [MS-XLS] §2.5.198.17 (Ftab) and §2.5.198.18 (Cetab).
const FUNCTIONS: &[&str] = &[
    "GET.CELL",
    "GET.WORKSPACE",
    "GET.WORKBOOK",
    "GET.DOCUMENT",
    "GET.WINDOW",
    "GET.NAME",
    "GET.FORMULA",
    "GET.NOTE",
    "GET.OBJECT",
    "GET.DEF",
    "GET.BAR",
    "GET.TOOL",
    "GET.TOOLBAR",
    "GET.CHART.ITEM",
    "GET.LINK.INFO",
    "GET.MOVIE",
    "GET.PIVOT.FIELD",
    "GET.PIVOT.ITEM",
    "GET.PIVOT.TABLE",
    "EVALUATE",
    "FILES",
    "DOCUMENTS",
    "DIRECTORY",
    "NAMES",
    "LINKS",
    "WINDOWS",
    "SELECTION",
    "ACTIVE.CELL",
    "REFTEXT",
    "TEXTREF",
    "ABSREF",
    "DEREF",
    "CALLER",
    "EXEC",
    "EXECUTE",
    "REGISTER",
    "REGISTER.ID",
    "CALL",
];

/// Recognizes calls even when the formula cannot be parsed by our worksheet parser.
pub(crate) fn calls_xlm(formula: &str) -> bool {
    let mut chars = formula.char_indices().peekable();
    let mut start = None;
    while let Some((i, c)) = chars.next() {
        if c.is_alphanumeric() || matches!(c, '.' | '_') {
            start.get_or_insert(i);
            continue;
        }
        if c == '('
            && let Some(word) = start.and_then(|s| formula.get(s..i))
            && FUNCTIONS.iter().any(|f| word.eq_ignore_ascii_case(f))
        {
            return true;
        }
        start = None;
        if matches!(c, '"' | '\'') {
            // Both text literals and quoted sheet names escape quotes by doubling them.
            while let Some((_, q)) = chars.next() {
                if q == c {
                    if chars.peek().is_some_and(|(_, next)| *next == c) {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        }
    }
    false
}

/// Follows reverse dependency edges iteratively, so chains and cycles need no recursion.
pub(crate) fn omitted_names(wb: &Workbook) -> HashSet<usize> {
    let mut lookup = HashMap::new();
    for (i, name) in wb.names.iter().enumerate() {
        lookup.entry((name.name.to_ascii_lowercase(), name.scope)).or_insert(i);
    }
    let sheets: HashMap<_, _> = wb.sheets.iter().enumerate().map(|(i, s)| (s.name.to_ascii_lowercase(), i)).collect();
    let mut users: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut omitted = HashSet::new();
    let mut pending = Vec::new();
    for (i, name) in wb.names.iter().enumerate() {
        if calls_xlm(&name.formula) {
            omitted.insert(i);
            pending.push(i);
        }
        let Ok(tokens) = tokenize(name.formula.strip_prefix('=').unwrap_or(&name.formula)) else { continue };
        let mut sheet = None;
        for token in tokens {
            match token.tok {
                Tok::Sheet(s, None) => {
                    sheet = Some(sheets.get(&s.to_ascii_lowercase()).copied());
                    continue;
                }
                Tok::Space => continue,
                Tok::Word(word) | Tok::Func(word) => {
                    let word = word.to_ascii_lowercase();
                    let scope = sheet.unwrap_or(name.scope);
                    // An unknown explicit sheet never resolves to a workbook name.
                    if sheet != Some(None) {
                        let target = lookup.get(&(word.clone(), scope)).or_else(|| lookup.get(&(word, None)));
                        if let Some(target) = target {
                            users.entry(*target).or_default().push(i);
                        }
                    }
                }
                _ => {}
            }
            sheet = None;
        }
    }
    while let Some(target) = pending.pop() {
        for user in users.get(&target).into_iter().flatten() {
            if omitted.insert(*user) {
                pending.push(*user);
            }
        }
    }
    omitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_model::{DefinedName, Sheet};

    #[test]
    fn call_tokens_respect_boundaries_and_quoted_text() {
        for function in FUNCTIONS {
            assert!(calls_xlm(&format!("INDEX({}(1),2)", function.to_ascii_lowercase())));
            for prefix in ["x", "1", ".", "_"] {
                assert!(!calls_xlm(&format!("{prefix}{function}(1)")));
            }
        }
        for formula in [
            "\"GET.CELL(1)\"",
            "\"a\"\"GET.WORKSPACE(1)\"",
            "'GET.CELL(1)'!A1",
            "'a''GET.CELL(1)'!A1",
            "CELL(\"row\")+INFO(\"osversion\")",
            "GET.CELL_NAME(1)",
            "GET.CELL",
        ] {
            assert!(!calls_xlm(formula), "{formula}");
        }
        assert!(calls_xlm("\"GET.CELL(1)\"&GET.WORKSPACE(37)"));
    }

    fn name(label: &str, scope: Option<usize>, formula: &str) -> DefinedName {
        DefinedName { name: label.into(), scope, formula: formula.into(), comment: String::new(), hidden: false }
    }

    #[test]
    fn dependencies_follow_scopes_and_cycles_without_matching_text_or_cells() {
        let wb = Workbook {
            sheets: vec![std::sync::Arc::new(Sheet::new("Data")), std::sync::Arc::new(Sheet::new("Other"))],
            names: vec![
                name("Macro", None, "GET.CELL(1)"),
                name("Macro", Some(0), "42"),
                name("Local", Some(0), "Macro"),
                name("GlobalUser", None, "mAcRo"),
                name("Qualified", None, "'Other'!Macro"),
                name("Shadowed", None, "Data!Macro"),
                name("Text", None, "\"Macro\""),
                name("Cell", None, "Data!A1"),
                name("CycleA", None, "CycleB"),
                name("CycleB", None, "CycleA+Macro"),
                name("CleanCycleA", None, "CleanCycleB"),
                name("CleanCycleB", None, "CleanCycleA"),
            ],
            ..Default::default()
        };
        let omitted = omitted_names(&wb);
        assert_eq!(omitted, HashSet::from([0, 3, 4, 8, 9]));
    }

    #[test]
    fn long_reverse_order_chain_is_fully_omitted() {
        let mut wb = Workbook::default();
        for i in 0..1024 {
            wb.names.push(name(&format!("Name_{i}"), None, &format!("Name_{}", i + 1)));
        }
        wb.names.push(name("Name_1024", None, "GET.WORKSPACE(37)"));
        assert_eq!(omitted_names(&wb).len(), wb.names.len());
    }

    #[test]
    fn templates_use_the_package_kind_without_changing_workbook_or_cell_formulas() {
        let mut wb = Workbook { names: vec![name("Macro", None, "GET.CELL(1)")], ..Default::default() };
        let mut sheet = Sheet::new("Data");
        sheet.set_cell(gridcraft_core::CellRef::new(0, 0), gridcraft_model::Cell::formula(gridcraft_model::Formula::new("GET.CELL(1)")));
        wb.sheets = vec![std::sync::Arc::new(sheet)];
        let before = wb.names.clone();
        for kind in [crate::XlsxKind::Template, crate::XlsxKind::MacroTemplate] {
            let bytes = crate::write_xlsx_as(&wb, kind).unwrap();
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut zip.by_name("xl/workbook.xml").unwrap(), &mut xml).unwrap();
            assert_eq!(xml.contains("<definedName name=\"Macro\""), kind.is_macro_enabled());
            xml.clear();
            std::io::Read::read_to_string(&mut zip.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut xml).unwrap();
            assert!(xml.contains("GET.CELL(1)"), "{xml}");
        }
        assert_eq!(wb.names, before);
    }
}

#[cfg(test)]
mod extended_tests {
    use super::*;
    use gridcraft_model::{DefinedName, Sheet};

    #[test]
    fn recognizes_table_and_respects_token_boundaries_and_quotes() {
        for f in FUNCTIONS {
            assert!(calls_xlm(&format!("INDEX({}(1),2)", f.to_ascii_lowercase())));
            for prefix in ["A", "0", ".", "_", "é"] {
                assert!(!calls_xlm(&format!("{prefix}{f}(1)")));
            }
        }
        for formula in ["\"escaped \"\" GET.CELL(1)\"", "'GET.CELL(1)'!A1", "CELL(\"row\")+INFO(\"osversion\")+INDEX(A1:A2,1)"] {
            assert!(!calls_xlm(formula), "{formula}");
        }
        assert!(calls_xlm("\"text\"&GET.CELL(1)"));
    }

    fn name(n: &str, formula: &str, scope: Option<usize>) -> DefinedName {
        DefinedName { name: n.into(), formula: formula.into(), scope, hidden: false, comment: String::new() }
    }

    #[test]
    fn dependencies_respect_scope_strings_and_cycles() {
        let wb = Workbook {
            sheets: vec![std::sync::Arc::new(Sheet::new("Data")), std::sync::Arc::new(Sheet::new("Other"))],
            names: vec![
                name("Source", "GET.CELL(1)", Some(0)),
                name("Source", "42", None),
                name("LocalUser", "source", Some(0)),
                name("GlobalUser", "Source", None),
                name("QualifiedUser", "'Data'!SOURCE", None),
                name("OtherUser", "Other!Source", None),
                name("Text", "\"Source\"", Some(0)),
                name("CycleA", "CycleB", None),
                name("CycleB", "CycleA+QualifiedUser", None),
                name("SafeCycle", "SafeCycle", None),
            ],
            ..Default::default()
        };
        assert_eq!(omitted_names(&wb), HashSet::from([0, 2, 4, 7, 8]));
    }

    #[test]
    fn long_reverse_chain_is_omitted_without_changing_workbook() {
        let mut wb = Workbook::default();
        for i in 0..2000 {
            wb.names.push(name(&format!("Name_{i}"), &format!("Name_{}", i + 1), None));
        }
        wb.names.push(name("Name_2000", "GET.WORKSPACE(37)", None));
        let original = wb.names.clone();
        assert_eq!(omitted_names(&wb).len(), 2001);
        for kind in [crate::XlsxKind::Template, crate::XlsxKind::MacroTemplate] {
            let bytes = crate::write_xlsx_as(&wb, kind).unwrap();
            let saved = crate::read_xlsx(&bytes).unwrap().0;
            assert_eq!(saved.names.len(), if kind.is_macro_enabled() { 2001 } else { 0 });
            assert_eq!(wb.names, original);
        }
    }
}
