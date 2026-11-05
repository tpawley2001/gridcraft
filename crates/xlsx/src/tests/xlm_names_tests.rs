//! Defined names that call Excel 4.0 macro-sheet functions (`GET.WORKSPACE`, `GET.CELL`, …) make
//! Excel refuse a macro-free package ("a macro-free file, but contains macro-enabled content"):
//! a `.xlsx` save leaves them out, with every name that uses them; `.xlsm` keeps them.

use std::io::Read;

use gridcraft_model::{DefinedName, Sheet, Workbook};

use crate::{XlsxKind, write_xlsx_as};

fn name(n: &str, formula: &str) -> DefinedName {
    DefinedName { name: n.into(), scope: None, formula: formula.into(), comment: String::new(), hidden: true }
}

fn book(names: &[(&str, &str)]) -> Workbook {
    Workbook { sheets: vec![std::sync::Arc::new(Sheet::new("Data"))], names: names.iter().map(|(n, f)| name(n, f)).collect(), ..Default::default() }
}

fn workbook_xml(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("xl/workbook.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn written_names(x: &str) -> Vec<String> {
    x.split("<definedName name=\"").skip(1).filter_map(|r| r.split('"').next()).map(str::to_string).collect()
}

const NAMES: &[(&str, &str)] =
    &[("Sep", "INDEX(GET.WORKSPACE(37),17)"), ("Sep2", "Sep&Sep"), ("Rate", "0.07"), ("Label", "\"GET.WORKSPACE(1) is text here\"")];

#[test]
fn xlsx_leaves_out_names_calling_xlm_functions_and_their_users() {
    let x = workbook_xml(&write_xlsx_as(&book(NAMES), XlsxKind::Workbook).unwrap());
    let names = written_names(&x);
    assert!(!names.iter().any(|n| n == "Sep"), "{x}");
    assert!(!names.iter().any(|n| n == "Sep2"), "a name using a left-out name is left out too: {x}");
    assert!(names.iter().any(|n| n == "Rate"), "{x}");
    assert!(names.iter().any(|n| n == "Label"), "text inside a string literal is not a call: {x}");
    assert!(!x.to_ascii_uppercase().contains("GET.WORKSPACE(37)"), "{x}");
}

#[test]
fn xlm_detection_ignores_case_and_chains_in_any_order() {
    let x = workbook_xml(
        &write_xlsx_as(
            &book(&[("Early", "Later*2"), ("Later", "Here"), ("Here", "get.cell(48,Data!$A$1)"), ("Plain", "Data!$A$1")]),
            XlsxKind::Workbook,
        )
        .unwrap(),
    );
    assert_eq!(written_names(&x), vec!["Plain".to_string()], "{x}");
}

#[test]
fn xlsm_keeps_names_calling_xlm_functions() {
    let x = workbook_xml(&write_xlsx_as(&book(NAMES), XlsxKind::MacroWorkbook).unwrap());
    let names = written_names(&x);
    for n in ["Sep", "Sep2", "Rate", "Label"] {
        assert!(names.iter().any(|w| w == n), "{n} missing: {x}");
    }
    assert!(x.contains("GET.WORKSPACE(37)"), "{x}");
}

#[test]
fn worksheet_functions_with_similar_names_are_kept() {
    let x = workbook_xml(
        &write_xlsx_as(
            &book(&[("Row1", "CELL(\"row\",Data!$A$1)"), ("Os", "INFO(\"osversion\")"), ("Ix", "INDEX(Data!$A$1:$A$3,2)")]),
            XlsxKind::Workbook,
        )
        .unwrap(),
    );
    assert_eq!(written_names(&x).len(), 3, "{x}");
}
