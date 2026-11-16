//! Builds an Excel 97-2003 (`.xls`, BIFF8) workbook linked to another workbook, records assembled
//! per [MS-XLS]: an external SupBook (2.4.271) naming `other.xls` and its sheet `Data`, one
//! EXTERNNAME (2.4.105) `Pair` defined as `Data!$B$2:$B$3`, and the cells saved with the link
//! (XCT 2.4.353, CRN 2.4.65): Data!B2 = 42, Data!B3 = 8.
//!
//! Sheet1's formulas were saved with 0 as their results, so only a recalculation from the link's
//! cells gives their values: A1 = `[1]Data!B2*2` (84), B1 = `SUM([1]!Pair)` (50).

use super::xls_pivot::{bof, eof, rec, u16s, xlstr};

/// External SupBook: `path` (XLUnicodeStringNoCch, 8-bit) then the sheet names.
fn supbook(path: &str, sheets: &[&str]) -> Vec<u8> {
    let mut p = [u16s(&[sheets.len() as u16, path.len() as u16]), vec![0], path.as_bytes().to_vec()].concat();
    for s in sheets {
        p.extend(xlstr(s));
    }
    rec(0x01AE, &p)
}

/// EXTERNNAME with an ExternDocName body: a workbook-level name and its definition `rgce`.
fn extern_name(name: &str, rgce: &[u8]) -> Vec<u8> {
    rec(0x0023, &[&u16s(&[0, 0, 0])[..], &[name.len() as u8, 0], name.as_bytes(), &(rgce.len() as u16).to_le_bytes(), rgce].concat())
}

/// CRN: one cached number at (`row`, `col`).
fn crn_number(row: u16, col: u8, x: f64) -> Vec<u8> {
    rec(0x005A, &[&[col, col][..], &row.to_le_bytes(), &[0x01], &x.to_le_bytes()].concat())
}

/// FORMULA record with a numeric cached result.
fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &[&u16s(&[r, c, 0])[..], &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce].concat())
}

/// The workbook described above.
pub fn workbook() -> Vec<u8> {
    // PtgArea3d (reference class, absolute) through XTI 1 (the link's sheet Data): $B$2:$B$3.
    let pair = [&[0x3Bu8][..], &u16s(&[1, 1, 2, 1, 1])].concat();
    let globals = |pos: u32| {
        [
            bof(0x0005),
            rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 6, 0], b"Sheet1"].concat()),
            // This workbook's SupBook, then the link's.
            rec(0x01AE, &u16s(&[1, 0x0401])),
            supbook("other.xls", &["Data"]),
            extern_name("Pair", &pair),
            rec(0x0059, &u16s(&[2, 0])),
            crn_number(1, 1, 42.0),
            crn_number(2, 1, 8.0),
            // EXTERNSHEET: XTI 0 = Sheet1, XTI 1 = the link's sheet Data.
            rec(0x0017, &u16s(&[2, 0, 0, 0, 1, 0, 0])),
            eof(),
        ]
        .concat()
    };
    // [1]Data!B2*2: PtgRef3d (relative) through XTI 1, PtgInt 2, PtgMul.
    let times_two = [&[0x3Au8][..], &u16s(&[1, 1, 1 | 0xC000]), &[0x1E, 2, 0, 0x05]].concat();
    // SUM([1]!Pair): PtgNameX through XTI 1 (the link's first name), PtgFuncVar SUM.
    let sum_pair = [&[0x39u8][..], &1u16.to_le_bytes(), &1u32.to_le_bytes(), &[0x22, 1, 4, 0]].concat();
    let sheet = [bof(0x0010), formula(0, 0, 0.0, &times_two), formula(0, 1, 0.0, &sum_pair), eof()].concat();
    let pos = globals(0).len() as u32;
    super::compound_file(&[("Workbook", &[globals(pos), sheet].concat())])
}
