//! Defined names in Excel 97-2003 workbooks whose formulas hold relative-offset references
//! (`PtgRefN` / `PtgAreaN`, [MS-XLS] 2.5.198.88 and 2.5.198.31). Those tokens belong to shared
//! formulas, not to names; Excel keeps such a name and replaces its formula with `#N/A`.
//! Written as an A1-relative reference instead, the saved `.xlsx` needs a repair in Excel.

use gridcraft_core::CellRef;

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, write_xlsx};

/// Lbl (0x0018) with workbook scope, 8-bit name text.
fn lbl(name: &str, rgce: &[u8]) -> Vec<u8> {
    rec(0x0018, &cat(&[&u16s(&[0]), &[0, name.len() as u8], &u16s(&[rgce.len() as u16, 0, 0]), &[0; 4], &[0], name.as_bytes(), rgce]))
}

/// PtgRefN of class `ptg` with a row offset and a signed 8-bit column offset, both relative.
fn ref_n(ptg: u8, dr: u16, dc: i8) -> Vec<u8> {
    cat(&[&[ptg], &dr.to_le_bytes(), &[dc as u8, 0xC0]])
}

/// PtgAreaN of class `ptg`: rows +0..+1, columns -1..-1, all relative.
fn area_n(ptg: u8) -> Vec<u8> {
    cat(&[&[ptg], &u16s(&[0, 1]), &[0xFF, 0xC0, 0xFF, 0xC0]])
}

/// PtgRef3d through XTI 0 (Sheet1), absolute row and column.
fn ref3d(r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &0u16.to_le_bytes(), &u16s(&[r, c])])
}

/// The exact tokens of a real name: `RefN(c-2)/SQRT(RefN(c-5))` (PtgFunc 0x41 iftab 20, PtgDiv).
fn real_shared_name() -> Vec<u8> {
    cat(&[&ref_n(0x6C, 0, -2), &ref_n(0x6C, 0, -5), &[0x41, 0x14, 0x00, 0x06]])
}

/// Names in file order; B1 = SHARED_FORMULA_0 (PtgName, value class, index 1).
fn names() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("SHARED_FORMULA_0", real_shared_name()),
        ("RefN_ref", ref_n(0x2C, 0, -1)),
        ("RefN_value", ref_n(0x4C, 2, 3)),
        ("RefN_array", ref_n(0x6C, 0, 1)),
        ("AreaN_ref", area_n(0x2D)),
        ("AreaN_value", area_n(0x4D)),
        ("AreaN_array", area_n(0x6D)),
        ("Mixed", cat(&[&ref3d(0, 0), &ref_n(0x4C, 0, -1), &[0x03]])),
        ("Plain", ref3d(2, 1)),
        ("Two", vec![0x1E, 2, 0]),
        // WINDOW.RESTORE(): PtgFuncVar with fCeFunc and Cetab 0x00A3, a macro command.
        ("Restore", vec![0x42, 0, 0xA3, 0x80]),
    ]
}

fn workbook() -> gridcraft_model::Workbook {
    let globals = |pos: u32| {
        let mut parts = vec![
            bof(0x0005),
            rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            rec(0x01AE, &u16s(&[1, 0x0401])),
            rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
        ];
        for (n, rgce) in names() {
            parts.push(lbl(n, &rgce));
        }
        parts.push(eof());
        parts.concat()
    };
    let name_ptg = cat(&[&[0x43], &1u32.to_le_bytes()]);
    let formula = rec(0x0006, &cat(&[&cell(0, 1, 0), &0f64.to_le_bytes(), &[0; 6], &(name_ptg.len() as u16).to_le_bytes(), &name_ptg]));
    let sheet = cat(&[&bof(0x0010), &rec(0x0203, &cat(&[&cell(0, 0, 0), &10.0f64.to_le_bytes()])), &formula, &eof()]);
    let pos = globals(0).len() as u32;
    read_xls(&compound("Workbook", &cat(&[&globals(pos), &sheet]))).unwrap().0
}

fn formula_of(wb: &gridcraft_model::Workbook, name: &str) -> Option<String> {
    wb.names.iter().find(|n| n.name == name && n.scope.is_none()).map(|n| n.formula.clone())
}

#[test]
fn names_with_relative_offset_references_open_as_na() {
    let wb = workbook();
    for name in ["SHARED_FORMULA_0", "RefN_ref", "RefN_value", "RefN_array", "AreaN_ref", "AreaN_value", "AreaN_array", "Mixed"] {
        assert_eq!(formula_of(&wb, name).as_deref(), Some("#N/A"), "{name}: {:?}", wb.names);
    }
}

#[test]
fn other_names_are_unchanged() {
    let wb = workbook();
    assert_eq!(formula_of(&wb, "Plain").as_deref(), Some("Sheet1!$B$3"));
    assert_eq!(formula_of(&wb, "Two").as_deref(), Some("2"));
    assert_eq!(wb.names.iter().filter(|n| n.scope.is_none()).count(), names().len(), "every name is kept: {:?}", wb.names);
    assert!(wb.names.iter().all(|n| !n.hidden), "{:?}", wb.names);
}

#[test]
fn a_cell_using_such_a_name_keeps_its_formula() {
    let wb = workbook();
    let f = wb.sheets[0].cell(CellRef::parse("B1").unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(f.as_deref().map(|t| t.trim_start_matches('=')), Some("SHARED_FORMULA_0"));
}

#[test]
fn saved_xlsx_writes_na_not_an_a1_reference() {
    let out = write_xlsx(&workbook()).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("xl/workbook.xml").unwrap(), &mut xml).unwrap();
    assert!(xml.contains(r#"<definedName name="SHARED_FORMULA_0">#N/A</definedName>"#), "{xml}");
    assert!(!xml.contains("IU1") && !xml.contains("IR1"), "{xml}");
    assert!(xml.contains(r#"<definedName name="Plain">Sheet1!$B$3</definedName>"#), "{xml}");
}

#[test]
fn a_name_holding_a_macro_command_is_still_decoded() {
    let wb = workbook();
    let restore = formula_of(&wb, "Restore");
    assert!(restore.as_deref().is_some_and(|f| f.contains("WINDOW.RESTORE")), "{restore:?}");
}
