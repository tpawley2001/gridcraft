//! `.xls` NAME records called `_xlfn.<FUNCTION>` / `_xlpm.<param>` ([MS-XLS] Lbl 2.4.150 with the
//! fFunc/fProc bits and `rgce` = PtgErr #NAME?) are placeholders Excel writes for functions newer than
//! BIFF8. They are not imported as defined names and are not reported as skipped macro names.
//! A real macro name (any other name with fFunc/fProc) still is.

use super::xls_tests::{bof, cat, compound, eof, rec, u16s};
use crate::{read_xls, write_xlsx};

const MACRO_WARNING: &str = "Macro names were skipped.";

/// PtgErr #NAME?, the `rgce` measured on real files for these records.
const NAME_ERR: [u8; 2] = [0x1C, 0x1D];

/// NAME (Lbl) record with an explicit grbit, workbook scope, 8-bit name.
fn lbl(grbit: u16, name: &str, rgce: &[u8]) -> Vec<u8> {
    rec(0x0018, &cat(&[&u16s(&[grbit]), &[0, name.len() as u8], &u16s(&[rgce.len() as u16, 0, 0]), &[0; 4], &[0], name.as_bytes(), rgce]))
}

/// One empty worksheet "Sheet1" and the given NAME records.
fn file(names: &[Vec<u8>]) -> Vec<u8> {
    let globals = |pos: u32| {
        let mut g = cat(&[&bof(0x0005), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]))]);
        for n in names {
            g.extend_from_slice(n);
        }
        g.extend(eof());
        g
    };
    let sheet = cat(&[&bof(0x0010), &eof()]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn workbook_xml(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name("xl/workbook.xml").unwrap(), &mut s).unwrap();
    s
}

#[test]
fn xlfn_placeholders_are_skipped_without_a_macro_warning() {
    // grbit values as measured: 0x000B (hidden|fFunc|fProc) and 0x038B (same, fGrp 14).
    let (wb, report) = read_xls(&file(&[lbl(0x000B, "_xlfn.IFNA", &NAME_ERR), lbl(0x038B, "_xlfn.SUMIFS", &NAME_ERR)])).unwrap();
    assert!(wb.names.is_empty(), "{:?}", wb.names);
    assert!(!report.warnings.iter().any(|w| w == MACRO_WARNING), "{:?}", report.warnings);
}

#[test]
fn xlpm_lambda_parameter_placeholders_are_skipped_without_a_macro_warning() {
    let (wb, report) = read_xls(&file(&[lbl(0x000B, "_xlfn.LAMBDA", &NAME_ERR), lbl(0x0019, "_xlpm.X", &NAME_ERR)])).unwrap();
    assert!(wb.names.is_empty(), "{:?}", wb.names);
    assert!(!report.warnings.iter().any(|w| w == MACRO_WARNING), "{:?}", report.warnings);
}

#[test]
fn the_prefix_is_matched_case_insensitively() {
    let (wb, report) = read_xls(&file(&[lbl(0x038B, "_XLFN.CONCAT", &NAME_ERR)])).unwrap();
    assert!(wb.names.is_empty(), "{:?}", wb.names);
    assert!(!report.warnings.iter().any(|w| w == MACRO_WARNING), "{:?}", report.warnings);
}

#[test]
fn placeholders_are_not_written_to_the_saved_xlsx() {
    let (wb, _) = read_xls(&file(&[lbl(0x038B, "_xlfn.IFNA", &NAME_ERR), lbl(0, "Plain", &[0x1E, 7, 0])])).unwrap();
    let book = workbook_xml(&write_xlsx(&wb).unwrap());
    assert!(!book.contains("_xlfn"), "{book}");
    assert!(book.contains(r#"<definedName name="Plain">7</definedName>"#), "{book}");
}

#[test]
fn a_real_macro_name_still_warns() {
    let (wb, report) = read_xls(&file(&[lbl(0x000A, "Auto_Open", &NAME_ERR)])).unwrap();
    assert!(wb.names.is_empty(), "{:?}", wb.names);
    assert!(report.warnings.iter().any(|w| w == MACRO_WARNING), "{:?}", report.warnings);
}
