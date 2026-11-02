//! XLSB macros: the VBA project part (`xl/vbaProject.bin`, [MS-XLSB] 2.1.7) of a synthetic
//! package is carried through unchanged and saved in a macro-enabled XML package.
use gridcraft_core::{CellRef, Value};

use super::make_zip;
use super::xlsb_tests::{RELS, ROOT, TYPES, book, cell, row, sst, worksheet};
use crate::{XlsxKind, read_xlsb, read_xlsx, write_xlsx_as};

const VBA: &[u8] = b"\xD0\xCF\x11\xE0 not really a project";
const VBA_REL: &str =
    r#"<Relationship Id="vba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="../xl/vbaProject.bin"/>"#;

fn package(linked: bool, part: bool) -> Vec<u8> {
    let rels = RELS.replace("</Relationships>", &format!("{}</Relationships>", if linked { VBA_REL } else { "" }));
    let types = TYPES.replace("</Types>", r#"<Override PartName="/xl/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/></Types>"#);
    let mut data = row(0);
    data.extend(cell(5, 0, &42.0f64.to_le_bytes()));
    let (book, sheet, two, strings) = (book(false), worksheet(&data, &[]), worksheet(&[], &[]), sst());
    let mut parts = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", rels.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
    ];
    if part {
        parts.push(("xl/vbaProject.bin", VBA));
    }
    make_zip(&parts)
}

fn macro_warnings(warnings: &[String]) -> Vec<&String> {
    warnings.iter().filter(|w| w.contains("VBA project")).collect()
}

#[test]
fn xlsb_vba_project_is_kept_byte_for_byte() {
    let (wb, report) = read_xlsb(&package(true, true)).unwrap();
    assert_eq!(wb.vba_project.as_deref().map(Vec::as_slice), Some(VBA));
    let warnings = macro_warnings(&report.warnings);
    assert_eq!(warnings.len(), 1, "{:?}", report.warnings);
    assert!(warnings[0].contains("kept but not executed"));
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
}

#[test]
fn xlsb_vba_relationship_to_missing_part_drops_macros_and_loads_the_rest() {
    let (wb, report) = read_xlsb(&package(true, false)).unwrap();
    assert_eq!(wb.vba_project, None);
    let warnings = macro_warnings(&report.warnings);
    assert_eq!(warnings.len(), 1, "{:?}", report.warnings);
    assert!(warnings[0].contains("missing part"));
    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
}

#[test]
fn xlsb_unlinked_vba_part_is_ignored() {
    let (wb, report) = read_xlsb(&package(false, true)).unwrap();
    assert_eq!(wb.vba_project, None);
    assert!(macro_warnings(&report.warnings).is_empty(), "{:?}", report.warnings);
}

#[test]
fn xlsb_vba_project_survives_saving_as_xlsm() {
    let (wb, _) = read_xlsb(&package(true, true)).unwrap();
    let xlsm = write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap();
    let (back, report) = read_xlsx(&xlsm).unwrap();
    assert_eq!(back.vba_project.as_deref().map(Vec::as_slice), Some(VBA));
    assert!(report.warnings.iter().any(|w| w.contains("kept but not executed")), "{:?}", report.warnings);
    assert_eq!(back.sheets[0].value(CellRef::new(0, 0)), Value::Number(42.0));
}
