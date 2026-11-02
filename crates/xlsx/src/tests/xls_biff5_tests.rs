//! Excel 5.0/95 (BIFF5) `.xls` import. Every workbook is assembled here from the BIFF5 record
//! layouts in a `Book` stream; no Excel-produced file is used.

use gridcraft_core::{CellRef, Value};

use super::xls_tests::compound;
use crate::{Format, read_xls, sniff};

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(vers: u16, dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[vers, dt, 0x0DBB, 0x07CC]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

fn cell(r: u16, c: u16, xf: u16) -> Vec<u8> {
    u16s(&[r, c, xf])
}

/// Label: a 2-byte count and 8-bit characters (already in the workbook's code page).
fn label(r: u16, c: u16, bytes: &[u8]) -> Vec<u8> {
    rec(0x0204, &cat(&[&cell(r, c, 0), &(bytes.len() as u16).to_le_bytes(), bytes]))
}

fn number(r: u16, c: u16, xf: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&cell(r, c, xf), &n.to_le_bytes()]))
}

fn formula(r: u16, c: u16, cached: [u8; 8], rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached, &[0, 0], &[0; 4], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// A BIFF5 cell reference token: row with the column/row relative flags in bits 14/15, 8-bit column.
fn ptg_ref(r: u16, c: u8, col_rel: bool, row_rel: bool) -> Vec<u8> {
    let rw = r | if col_rel { 0x4000 } else { 0 } | if row_rel { 0x8000 } else { 0 };
    cat(&[&[0x24], &rw.to_le_bytes(), &[c]])
}

/// A BIFF5 PtgRef3d: ixals, 8 reserved bytes, first and last sheet index, row field, column.
fn ptg_ref3d(ixals: i16, itab: i16, r: u16, c: u8, rel: bool) -> Vec<u8> {
    let rw = r | if rel { 0xC000 } else { 0 };
    cat(&[&[0x3A], &ixals.to_le_bytes(), &[0; 8], &itab.to_le_bytes(), &itab.to_le_bytes(), &rw.to_le_bytes(), &[c]])
}

fn ptg_int(n: u16) -> Vec<u8> {
    cat(&[&[0x1E], &n.to_le_bytes()])
}

fn num(n: f64) -> [u8; 8] {
    n.to_le_bytes()
}

/// A Book stream: globals (CodePage, the extra records, BoundSheets) and one worksheet substream per sheet.
fn book(codepage: u16, extra: &[u8], sheets: &[(&[u8], Vec<u8>)]) -> Vec<u8> {
    let boundsheet = |pos: u32, name: &[u8]| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, name.len() as u8], name]));
    let head = cat(&[&bof(0x0500, 0x0005), &rec(0x0042, &codepage.to_le_bytes()), extra]);
    let bs_len: usize = sheets.iter().map(|(n, _)| boundsheet(0, n).len()).sum();
    let mut pos = (head.len() + bs_len + eof().len()) as u32;
    let mut globals = head;
    let mut bodies = vec![];
    for (name, records) in sheets {
        globals.extend(boundsheet(pos, name));
        let body = cat(&[&bof(0x0500, 0x0010), records, &eof()]);
        pos += body.len() as u32;
        bodies.extend(body);
    }
    globals.extend(eof());
    compound("Book", &cat(&[&globals, &bodies]))
}

fn v(wb: &gridcraft_model::Workbook, sheet: usize, a1: &str) -> Value {
    wb.sheets[sheet].value(CellRef::parse(a1).unwrap())
}

fn ftext(wb: &gridcraft_model::Workbook, sheet: usize, a1: &str) -> Option<String> {
    wb.sheets[sheet].cell(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

/// Two XFs: General, then custom format 164 "0.00%".
fn percent_format() -> Vec<u8> {
    let xf = |ifmt: u16| rec(0x00E0, &cat(&[&u16s(&[0, ifmt]), &[0; 12]]));
    cat(&[&rec(0x041E, &cat(&[&164u16.to_le_bytes(), &[5], b"0.00%"])), &xf(0), &xf(164)])
}

#[test]
fn reads_values_of_two_sheets_in_code_page_1252() {
    let data = cat(&[
        &number(0, 0, 0, 1.5),
        &rec(0x027E, &cat(&[&cell(1, 0, 0), &((7u32 << 2) | 2).to_le_bytes()])),
        &label(2, 0, &[b'G', b'r', 0xF6, 0xDF, b'e']),
        &rec(0x0205, &cat(&[&cell(3, 0, 0), &[1, 0]])),
        &rec(0x0205, &cat(&[&cell(4, 0, 0), &[0x07, 1]])),
        &rec(0x0201, &cell(5, 0, 1)),
        &number(0, 2, 1, 0.25),
    ]);
    let calc = label(0, 0, b"ok");
    let bytes = book(1252, &percent_format(), &[(b"Data", data), (b"Calc", calc)]);
    assert_eq!(sniff(&bytes), Format::Xls);
    let (wb, report) = read_xls(&bytes).unwrap();
    assert!(report.warnings[0].contains("Excel 5.0/95"), "{:?}", report.warnings);
    assert_eq!(wb.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Data", "Calc"]);
    assert_eq!(v(&wb, 0, "A1"), Value::number(1.5));
    assert_eq!(v(&wb, 0, "A2"), Value::number(7.0));
    assert_eq!(v(&wb, 0, "A3"), Value::from("Größe"));
    assert_eq!(v(&wb, 0, "A4"), Value::Bool(true));
    assert_eq!(v(&wb, 0, "A5"), Value::Error(gridcraft_core::CellError::Div0));
    assert_eq!(v(&wb, 0, "A6"), Value::Empty);
    assert_eq!(v(&wb, 1, "A1"), Value::from("ok"));
    // The custom number format reaches both the styled blank and the number.
    let sh = &wb.sheets[0];
    for a1 in ["A6", "C1"] {
        assert_eq!(wb.styles.get(sh.style_id(CellRef::parse(a1).unwrap())).num_fmt.as_str(), "0.00%", "{a1}");
    }
    assert_eq!(wb.styles.get(sh.style_id(CellRef::parse("A1").unwrap())).num_fmt.as_str(), "General");
}

#[test]
fn decodes_labels_in_code_page_1251() {
    // "Привет" in Windows-1251.
    let bytes = book(1251, &[], &[(b"Data", label(0, 0, &[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]))]);
    let (wb, report) = read_xls(&bytes).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::from("Привет"));
    assert!(!report.warnings.iter().any(|w| w.contains("code page")), "{:?}", report.warnings);
    // An unsupported code page reads as Windows-1252, with one warning.
    let bytes = book(10000, &[], &[(b"Data", cat(&[&label(0, 0, &[0xE9]), &label(1, 0, &[0xE9])]))]);
    let (wb, report) = read_xls(&bytes).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::from("é"));
    assert_eq!(report.warnings.iter().filter(|w| w.contains("code page 10000")).count(), 1, "{:?}", report.warnings);
}

#[test]
fn reads_formulas_with_relative_and_absolute_refs() {
    // A1*2+SUM($A$1:A3) at B1: the relative flags live in the row fields.
    let area = cat(&[&[0x25], &u16s(&[0, 2 | 0xC000]), &[0, 0]]);
    let rgce = cat(&[&ptg_ref(0, 0, true, true), &ptg_int(2), &[0x05], &area, &[0x22, 1, 4, 0], &[0x03]]);
    // $A1 (column absolute, row relative) and A$1.
    let mixed = cat(&[&ptg_ref(0, 0, false, true), &ptg_ref(0, 0, true, false), &[0x03]]);
    let sheet = cat(&[&number(0, 0, 0, 1.0), &formula(0, 1, num(3.0), &rgce), &formula(1, 1, num(2.0), &mixed)]);
    let (wb, _) = read_xls(&book(1252, &[], &[(b"Data", sheet)])).unwrap();
    assert_eq!(ftext(&wb, 0, "B1").as_deref(), Some("A1*2+SUM($A$1:A3)"));
    assert_eq!(ftext(&wb, 0, "B2").as_deref(), Some("$A1+A$1"));
    assert_eq!(v(&wb, 0, "B1"), Value::number(3.0));
}

#[test]
fn reads_text_constants_and_string_results() {
    // "ab"&A1: PtgStr is a 1-byte count and 8-bit characters, no flags byte.
    let rgce = cat(&[&[0x17, 3, b'a', 0xE9, b'b'], &ptg_ref(0, 0, true, true), &[0x08]]);
    let string_result = [0, 0, 0, 0, 0, 0, 0xFF, 0xFF];
    let sheet =
        cat(&[&number(0, 0, 0, 5.0), &formula(0, 1, string_result, &rgce), &rec(0x0207, &cat(&[&4u16.to_le_bytes(), &[b'a', 0xE9, b'b', b'5']]))]);
    let (wb, _) = read_xls(&book(1252, &[], &[(b"Data", sheet)])).unwrap();
    assert_eq!(ftext(&wb, 0, "B1").as_deref(), Some("\"aéb\"&A1"));
    assert_eq!(v(&wb, 0, "B1"), Value::from("aéb5"));
}

#[test]
fn reads_3d_refs_and_defined_names() {
    // EXTERNSHEET entry 1: sheet "Data" of this workbook.
    let externsheet = rec(0x0017, &[5, 0x03, b'D', b'a', b't', b'a']);
    // NAME "Rate" = Data!$B$1, through the EXTERNSHEET entry.
    let name_rgce = ptg_ref3d(1, -1, 0, 1, false);
    let name = rec(
        0x0018,
        &cat(&[
            &0u16.to_le_bytes(),
            &[0, 4],
            &(name_rgce.len() as u16).to_le_bytes(),
            &0u16.to_le_bytes(),
            &0u16.to_le_bytes(),
            &[0; 4],
            b"Rate",
            &name_rgce,
        ]),
    );
    let data = cat(&[&number(0, 0, 0, 10.0), &number(0, 1, 0, 0.5)]);
    // Data!A1+1 (a sheet of this workbook by index) and Rate*2 (PtgName: index, 12 reserved bytes).
    let ref3d = cat(&[&ptg_ref3d(-1, 0, 0, 0, true), &ptg_int(1), &[0x03]]);
    let use_name = cat(&[&[0x23], &1u16.to_le_bytes(), &[0; 12], &ptg_int(2), &[0x05]]);
    let calc = cat(&[&formula(0, 0, num(11.0), &ref3d), &formula(1, 0, num(1.0), &use_name)]);
    let (wb, report) = read_xls(&book(1252, &cat(&[&externsheet, &name]), &[(b"Data", data), (b"Calc", calc)])).unwrap();
    assert_eq!(ftext(&wb, 1, "A1").as_deref(), Some("Data!A1+1"), "{:?}", report.warnings);
    assert_eq!(ftext(&wb, 1, "A2").as_deref(), Some("Rate*2"), "{:?}", report.warnings);
    let rate = wb.names.iter().find(|n| n.name == "Rate").unwrap();
    assert_eq!(rate.formula, "Data!$B$1");
    assert_eq!(rate.scope, None);
}

#[test]
fn unknown_tokens_keep_the_saved_result() {
    let sheet = formula(0, 0, num(42.0), &[0xFF]);
    let (wb, report) = read_xls(&book(1252, &[], &[(b"Data", sheet)])).unwrap();
    assert_eq!(ftext(&wb, 0, "A1"), None);
    assert_eq!(v(&wb, 0, "A1"), Value::number(42.0));
    assert_eq!(report.warnings.iter().filter(|w| w.contains("unknown formula token")).count(), 1, "{:?}", report.warnings);
}

#[test]
fn biff4_and_earlier_stay_refused() {
    let stream = cat(&[&bof(0x0400, 0x0005), &eof()]);
    let err = read_xls(&compound("Book", &stream)).unwrap_err().to_string();
    assert!(err.contains("unsupported BIFF version"), "{err}");
}

#[test]
fn hostile_biff5_records_never_panic() {
    // Truncated strings, names and 3-D tokens: errors or saved values, never a crash.
    let cases: Vec<Vec<u8>> = vec![
        rec(0x0204, &cat(&[&cell(0, 0, 0), &500u16.to_le_bytes(), b"x"])),
        formula(0, 0, num(1.0), &[0x17, 9, b'a']),
        formula(0, 0, num(1.0), &[0x3A, 1, 0]),
        formula(0, 0, num(1.0), &[0x23, 1]),
        formula(0, 0, num(1.0), &cat(&[&ptg_ref3d(7, -1, 0, 0, false)])),
        formula(0, 0, num(1.0), &cat(&[&[0x39], &[0; 23]])),
    ];
    for c in cases {
        let _ = read_xls(&book(1252, &[], &[(b"Data", c)]));
    }
    let globals = [rec(0x0017, &[200, 3]), rec(0x0018, &[0, 0, 0, 200, 0, 0]), rec(0x041E, &[164, 0, 50]), rec(0x0023, &[0, 0, 0, 0, 0, 0, 9])];
    for g in globals {
        let _ = read_xls(&book(1252, &g, &[(b"Data", number(0, 0, 0, 1.0))]));
    }
}
