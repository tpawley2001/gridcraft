//! `.xls` defined names that refer to other workbooks ([MS-XLS] Lbl 2.4.150 whose `rgce` holds a
//! PtgRef3d or PtgArea3d through an XTI that points at an external SupBook 2.4.271) are kept with
//! the `.xlsx` text `[n]Sheet!$A$1`, as formulas in cells are, instead of being skipped; they
//! survive Save As `.xlsx` beside the `externalLink` part. The linked file is never opened.
use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s, xlstr};
use crate::{read_xls, read_xlsx, write_xlsx};

const SKIP_WARNING: &str = "Some defined names use references to other workbooks (external links) and were skipped.";

/// External SupBook: `path` (XLUnicodeStringNoCch, 8-bit) then the sheet names.
fn supbook(path: &str, sheets: &[&str]) -> Vec<u8> {
    let mut p = cat(&[&u16s(&[sheets.len() as u16, path.len() as u16]), &[0], path.as_bytes()]);
    for s in sheets {
        p.extend(xlstr(s));
    }
    rec(0x01AE, &p)
}

/// XCT + one CRN: the linked sheet `itab`'s cached cell at (`row`, `col`) holding `x`.
fn cached(itab: u16, row: u16, col: u8, x: f64) -> Vec<u8> {
    cat(&[&rec(0x0059, &u16s(&[1, itab])), &rec(0x005A, &cat(&[&[col, col], &row.to_le_bytes(), &[0x01], &x.to_le_bytes()]))])
}

/// PtgRef3d (reference class), absolute row and column.
fn ref3d(ixti: u16, r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &u16s(&[ixti, r, c])])
}

/// PtgArea3d (reference class), absolute rows and columns.
fn area3d(ixti: u16, r0: u16, r1: u16, c0: u16, c1: u16) -> Vec<u8> {
    cat(&[&[0x3B], &u16s(&[ixti, r0, r1, c0, c1])])
}

/// NAME (Lbl) record, as measured on real files: grbit, chKey, cch, cce, reserved, itab
/// (0 = workbook, else 1-based sheet), 4 reserved bytes, the 8-bit name, then `rgce`.
fn lbl(name: &str, itab: u16, rgce: &[u8]) -> Vec<u8> {
    rec(0x0018, &cat(&[&u16s(&[0]), &[0, name.len() as u8], &u16s(&[rgce.len() as u16, 0, itab]), &[0; 4], &[0], name.as_bytes(), rgce]))
}

/// FORMULA record with a numeric cached result.
fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// One sheet "Sheet1" whose A1 = `Rate*2` (PtgName 1, saved result 84). SupBooks: 0 = this
/// workbook, 1 = `other.xls` with sheets Data and `My Data` (Data!B2 cached as 42). XTI 0 =
/// Sheet1, 1 = [1]Data, 2 = [1]My Data, 3 = a SupBook index the file doesn't have.
fn file(names: &[Vec<u8>]) -> Vec<u8> {
    let globals = |pos: u32| {
        let mut g = cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &supbook("other.xls", &["Data", "My Data"]),
            &cached(0, 1, 1, 42.0),
            &rec(0x0017, &cat(&[&u16s(&[4]), &u16s(&[0, 0, 0]), &u16s(&[1, 0, 0]), &u16s(&[1, 1, 1]), &u16s(&[9, 0, 0])])),
        ]);
        for n in names {
            g.extend_from_slice(n);
        }
        g.extend(eof());
        g
    };
    let rate_x2 = cat(&[&[0x23], &1u32.to_le_bytes(), &[0x1E, 2, 0], &[0x05]]);
    let sheet = cat(&[&bof(0x0010), &formula(0, 0, 84.0, &rate_x2), &eof()]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn names(wb: &gridcraft_model::Workbook) -> Vec<(String, Option<usize>, String)> {
    wb.names.iter().map(|n| (n.name.clone(), n.scope, n.formula.clone())).collect()
}

fn saved_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).unwrap_or_else(|_| panic!("{name} missing"));
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    s
}

#[test]
fn a_name_into_a_linked_workbook_is_kept_with_its_link_text() {
    let (wb, report) = read_xls(&file(&[lbl("Rate", 0, &ref3d(1, 1, 1))])).unwrap();
    assert_eq!(names(&wb), [("Rate".to_string(), None, "[1]Data!$B$2".to_string())], "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w == SKIP_WARNING), "{:?}", report.warnings);
}

#[test]
fn ranges_quoted_sheets_and_mixed_formulas_keep_the_xlsx_text() {
    let mixed = cat(&[&ref3d(0, 0, 0), &ref3d(1, 1, 1), &[0x03]]);
    let file = file(&[
        lbl("Block", 0, &area3d(1, 0, 2, 0, 1)),
        lbl("Spaced", 0, &ref3d(2, 1, 1)),
        lbl("Mixed", 0, &mixed),
        lbl("Local", 1, &ref3d(1, 1, 1)),
    ]);
    let (wb, report) = read_xls(&file).unwrap();
    assert_eq!(
        names(&wb),
        [
            ("Block".to_string(), None, "[1]Data!$A$1:$B$3".to_string()),
            ("Spaced".to_string(), None, "'[1]My Data'!$B$2".to_string()),
            ("Mixed".to_string(), None, "Sheet1!$A$1+[1]Data!$B$2".to_string()),
            ("Local".to_string(), Some(0), "[1]Data!$B$2".to_string()),
        ],
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_cell_using_the_name_keeps_its_formula_and_saved_result() {
    let (wb, report) = read_xls(&file(&[lbl("Rate", 0, &ref3d(1, 1, 1))])).unwrap();
    let a1 = CellRef::new(0, 0);
    let text = wb.sheets[0].cell(a1).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(text.as_deref(), Some("Rate*2"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(a1), Value::Number(84.0));
}

#[test]
fn the_name_survives_save_as_xlsx_beside_the_link() {
    let (wb, _) = read_xls(&file(&[lbl("Rate", 0, &ref3d(1, 1, 1))])).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let book = saved_part(&saved, "xl/workbook.xml");
    assert!(book.contains(r#"<definedName name="Rate">[1]Data!$B$2</definedName>"#), "{book}");
    assert!(saved_part(&saved, "xl/externalLinks/externalLink1.xml").contains("Data"));
    let (back, report) = read_xlsx(&saved).unwrap();
    assert_eq!(names(&back), [("Rate".to_string(), None, "[1]Data!$B$2".to_string())], "{:?}", report.warnings);
}

/// Guard: a name whose XTI names a SupBook the file doesn't have is still skipped with the
/// existing warning, and names into this workbook read as before.
#[test]
fn names_into_a_missing_link_are_still_skipped_and_local_names_unchanged() {
    let (wb, report) = read_xls(&file(&[lbl("Gone", 0, &ref3d(3, 1, 1)), lbl("Here", 0, &ref3d(0, 4, 2))])).unwrap();
    assert_eq!(names(&wb), [("Here".to_string(), None, "Sheet1!$C$5".to_string())], "{:?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w.contains("defined names") && w.contains("were skipped")), "{:?}", report.warnings);
}
