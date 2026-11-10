//! An `.xlsb` whose worksheet part is missing from the package (a truncated file) opens with that
//! sheet empty and one warning, like the `.xlsx` reader does, instead of refusing the whole file.
//! Excel opens such a file and shows the sheets it can read.

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Visibility, Workbook};

use super::make_zip;
use super::xlsb_tests::{RELS, ROOT, TYPES, book, cell, row, sst, worksheet};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

/// The two-sheet package of `xlsb_tests` (`Data` with A1 = 42, `Hidden` hidden), leaving out the
/// worksheet parts named in `missing`; `types` is the `[Content_Types].xml` to use.
fn package(missing: &[&str], types: &str) -> Vec<u8> {
    let mut data = row(0);
    data.extend(cell(5, 0, &42f64.to_le_bytes()));
    let one = worksheet(&data, &[]);
    let two = worksheet(&[], &[]);
    let book = book(false);
    let strings = sst();
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("text/strings.bin", &strings),
    ];
    for (name, bytes) in [("data/one.bin", &one), ("data/two.bin", &two)] {
        if !missing.contains(&name) {
            parts.push((name, bytes.as_slice()));
        }
    }
    make_zip(&parts)
}

fn open(missing: &[&str], types: &str) -> (Workbook, ReadReport) {
    read_xlsb(&package(missing, types)).unwrap_or_else(|e| panic!("missing {missing:?}: the file must open, got {e}"))
}

fn names(wb: &Workbook) -> Vec<String> {
    wb.sheets.iter().map(|s| s.name.clone()).collect()
}

fn missing_warnings(report: &ReadReport) -> Vec<&String> {
    report.warnings.iter().filter(|w| w.contains("missing part")).collect()
}

#[test]
fn xlsb_missing_second_sheet_part_opens_with_that_sheet_empty() {
    let (wb, report) = open(&["data/two.bin"], TYPES);
    assert_eq!(names(&wb), ["Data", "Hidden"]);
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
    assert_eq!(wb.sheets[1].used_range(), None);
    assert_eq!(wb.sheets[1].visibility, Visibility::Hidden);
    let warnings = missing_warnings(&report);
    assert_eq!(warnings.len(), 1, "{:?}", report.warnings);
    assert!(warnings[0].contains("Hidden"), "{:?}", report.warnings);
}

#[test]
fn xlsb_missing_first_sheet_part_keeps_the_sheet_empty_at_its_position() {
    let (wb, report) = open(&["data/one.bin"], TYPES);
    assert_eq!(names(&wb), ["Data", "Hidden"]);
    assert_eq!(wb.sheets[0].used_range(), None);
    assert_eq!(wb.sheets[0].visibility, Visibility::Visible);
    assert_eq!(wb.sheets[1].visibility, Visibility::Hidden);
    let warnings = missing_warnings(&report);
    assert_eq!(warnings.len(), 1, "{:?}", report.warnings);
    assert!(warnings[0].contains("Data"), "{:?}", report.warnings);
}

#[test]
fn xlsb_missing_sheet_part_without_content_type_override_still_opens() {
    let override_two = r#"<Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/>"#;
    assert!(TYPES.contains(override_two));
    let types = TYPES.replace(override_two, "");
    let (wb, report) = open(&["data/two.bin"], &types);
    assert_eq!(names(&wb), ["Data", "Hidden"]);
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
    assert_eq!(wb.sheets[1].used_range(), None);
    assert_eq!(wb.sheets[1].visibility, Visibility::Hidden);
    let warnings = missing_warnings(&report);
    assert_eq!(warnings.len(), 1, "{:?}", report.warnings);
    assert!(warnings[0].contains("Hidden"), "{:?}", report.warnings);
}

#[test]
fn xlsb_every_sheet_part_missing_opens_with_every_sheet_empty() {
    let (wb, report) = open(&["data/one.bin", "data/two.bin"], TYPES);
    assert_eq!(names(&wb), ["Data", "Hidden"]);
    assert!(wb.sheets.iter().all(|s| s.used_range().is_none()));
    assert_eq!(missing_warnings(&report).len(), 2, "{:?}", report.warnings);
}

#[test]
fn xlsb_missing_sheet_part_saves_as_xlsx_with_names_order_and_visibility() {
    let (wb, _) = open(&["data/two.bin"], TYPES);
    let saved = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(names(&back), ["Data", "Hidden"]);
    assert_eq!(back.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
    assert_eq!(back.sheets[1].visibility, Visibility::Hidden);
    assert_eq!(back.sheets[1].used_range(), None);
}
