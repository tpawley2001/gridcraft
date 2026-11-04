//! XLSB defined names (BrtName) and other-sheet references (PtgRef3d / PtgArea3d resolved
//! through BrtSupSelf / BrtExternSheet), from records assembled in Rust per [MS-XLSB].
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::Workbook;

use super::make_zip;
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

pub(super) fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}

/// BrtRowHdr for row `r` (no spans).
fn row(r: u32) -> Vec<u8> {
    let mut out = r.to_le_bytes().to_vec();
    out.extend([0; 13]);
    record(0, &out)
}
/// BrtCellReal.
fn number(col: u32, n: f64) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(n.to_le_bytes());
    record(5, &data)
}
/// BrtFmlaNum with its cached result and rgce.
fn fmla(col: u32, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(cached.to_le_bytes());
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(9, &data)
}
fn worksheet(rows: &[(u32, Vec<Vec<u8>>)]) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(145, &[]));
    for (r, cells) in rows {
        out.extend(row(*r));
        for c in cells {
            out.extend(c);
        }
    }
    out.extend(record(146, &[]));
    out.extend(record(130, &[]));
    out
}

/// PtgRef3d (reference class): ixti, then RgceLoc with a u32 row.
fn ref3d(ixti: u16, row: u32, col: u16, relative: bool) -> Vec<u8> {
    let mut out = vec![0x3A];
    out.extend(ixti.to_le_bytes());
    out.extend(row.to_le_bytes());
    out.extend((col | if relative { 0xC000 } else { 0 }).to_le_bytes());
    out
}
/// PtgArea3d (reference class): ixti, then RgceArea with u32 rows.
pub(super) fn area3d(ixti: u16, r0: u32, r1: u32, c0: u16, c1: u16, relative: bool) -> Vec<u8> {
    let flag = if relative { 0xC000 } else { 0 };
    let mut out = vec![0x3B];
    out.extend(ixti.to_le_bytes());
    out.extend(r0.to_le_bytes());
    out.extend(r1.to_le_bytes());
    out.extend((c0 | flag).to_le_bytes());
    out.extend((c1 | flag).to_le_bytes());
    out
}
/// PtgName (value class) with a one-based name index.
pub(super) fn name_ptg(index: u32) -> Vec<u8> {
    let mut out = vec![0x43];
    out.extend(index.to_le_bytes());
    out
}
fn int(n: u16) -> Vec<u8> {
    let mut out = vec![0x1E];
    out.extend(n.to_le_bytes());
    out
}
const MUL: u8 = 0x05;
const SUM: [u8; 4] = [0x22, 1, 4, 0];

/// BrtName: flags, chKey, itab, name, NameParsedFormula, then a null comment.
pub(super) fn name_record(flags: u32, itab: u32, name: &str, rgce: &[u8]) -> Vec<u8> {
    let mut data = flags.to_le_bytes().to_vec();
    data.push(0);
    data.extend(itab.to_le_bytes());
    data.extend(wide(name));
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    data.extend(u32::MAX.to_le_bytes());
    record(39, &data)
}
const WORKBOOK: u32 = u32::MAX;

/// The EXTERNALS block: BrtSupSelf (book 0), BrtSupBookSrc (book 1, another workbook), then
/// BrtExternSheet with XTIs 0 Data, 1 Calc, 2 the other book's first sheet, 3 `My Data`.
pub(super) fn externals() -> Vec<u8> {
    let mut out = record(353, &[]);
    out.extend(record(357, &[]));
    out.extend(record(355, &wide("rId9")));
    let mut xti = 4u32.to_le_bytes().to_vec();
    for (book, first, last) in [(0u32, 0i32, 0i32), (0, 1, 1), (1, 0, 0), (0, 2, 2)] {
        xti.extend(book.to_le_bytes());
        xti.extend(first.to_le_bytes());
        xti.extend(last.to_le_bytes());
    }
    out.extend(record(362, &xti));
    out.extend(record(354, &[]));
    out
}

pub(super) fn book(sheets: &[&str], externals: &[u8], names: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    for (i, name) in sheets.iter().enumerate() {
        let mut data = 0u32.to_le_bytes().to_vec();
        data.extend((i as u32 + 1).to_le_bytes());
        data.extend(wide(&format!("s{i}")));
        data.extend(wide(name));
        out.extend(record(156, &data));
    }
    out.extend(record(144, &[]));
    out.extend(externals);
    for n in names {
        out.extend(n);
    }
    out.extend(record(132, &[]));
    out
}

pub(super) fn package(book: &[u8], sheets: &[Vec<u8>]) -> Vec<u8> {
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
    let mut rels = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/>"#,
    );
    let paths: Vec<String> = (0..sheets.len()).map(|i| format!("xl/worksheets/sheet{i}.bin")).collect();
    for i in 0..sheets.len() {
        rels.push_str(&format!(
            r#"<Relationship Id="s{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i}.bin"/>"#
        ));
        types.push_str(&format!(r#"<Override PartName="/xl/worksheets/sheet{i}.bin" ContentType="application/vnd.ms-excel.worksheet"/>"#));
    }
    rels.push_str("</Relationships>");
    types.push_str("</Types>");
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
    ];
    for (path, sheet) in paths.iter().zip(sheets) {
        parts.push((path, sheet));
    }
    make_zip(&parts)
}

/// Data!A1:A10 = 1..10, Data!B1 = 0.5, Data!B2 = 7; `My Data`!B2 = 3; formulas on Calc.
fn names_book(calc: &[Vec<u8>], names: &[Vec<u8>]) -> Vec<u8> {
    let data = worksheet(
        &(0..10u32)
            .map(|r| (r, [vec![number(0, f64::from(r + 1))], [0.5, 7.0].get(r as usize).map(|n| number(1, *n)).into_iter().collect()].concat()))
            .collect::<Vec<_>>(),
    );
    let calc = worksheet(&calc.iter().enumerate().map(|(r, c)| (r as u32, vec![c.clone()])).collect::<Vec<_>>());
    let my_data = worksheet(&[(1, vec![number(1, 3.0)])]);
    package(&book(&["Data", "Calc", "My Data"], &externals(), names), &[data, calc, my_data])
}

/// Calc!A1:A4 and the names Rate, Local and Data's print area.
fn full_book() -> (Workbook, ReadReport) {
    let names = vec![
        name_record(0, WORKBOOK, "Rate", &ref3d(0, 0, 1, false)),
        name_record(1, 1, "Local", &ref3d(1, 0, 0, false)),
        name_record(0x20, 0, "_xlnm.Print_Area", &area3d(0, 0, 4, 0, 2, false)),
    ];
    let calc = vec![
        fmla(0, 14.0, &[ref3d(0, 1, 1, true), int(2), vec![MUL]].concat()),
        fmla(0, 55.0, &[area3d(0, 0, 9, 0, 0, true), SUM.to_vec()].concat()),
        fmla(0, 5.0, &[name_ptg(1), int(10), vec![MUL]].concat()),
        fmla(0, 3.0, &ref3d(3, 1, 1, true)),
    ];
    read_xlsb(&names_book(&calc, &names)).unwrap()
}

fn ftext(wb: &Workbook, sheet: usize, a1: &str) -> Option<String> {
    wb.sheets[sheet].cell(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

#[test]
fn xlsb_other_sheet_references_and_names_decode() {
    let (wb, report) = full_book();
    assert_eq!(ftext(&wb, 1, "A1").as_deref(), Some("Data!B2*2"));
    assert_eq!(ftext(&wb, 1, "A2").as_deref(), Some("SUM(Data!A1:A10)"));
    assert_eq!(ftext(&wb, 1, "A3").as_deref(), Some("Rate*10"));
    assert_eq!(ftext(&wb, 1, "A4").as_deref(), Some("'My Data'!B2"));
    assert!(!report.warnings.iter().any(|w| w.contains("formula(s)")), "{:?}", report.warnings);
}

#[test]
fn xlsb_defined_names_keep_scope_hidden_flag_and_print_area() {
    let (wb, _) = full_book();
    let rate = wb.names.iter().find(|n| n.name == "Rate").unwrap();
    assert_eq!((rate.scope, rate.formula.as_str(), rate.hidden), (None, "Data!$B$1", false));
    let local = wb.names.iter().find(|n| n.name == "Local").unwrap();
    assert_eq!((local.scope, local.formula.as_str(), local.hidden), (Some(1), "Calc!$A$1", true));
    // The built-in print area becomes Data's page setup, not a defined name (as in .xls import).
    assert_eq!(wb.sheets[0].print.print_area, RangeRef::parse("A1:C5"));
    assert!(!wb.names.iter().any(|n| n.name.contains("Print_Area")));
    assert_eq!(wb.names.len(), 2);
}

#[test]
fn xlsb_names_and_formulas_survive_saving_as_xlsx() {
    let (wb, _) = full_book();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for a1 in ["A1", "A2", "A3", "A4"] {
        assert_eq!(ftext(&back, 1, a1), ftext(&wb, 1, a1), "{a1}");
    }
    let key = |w: &Workbook| {
        let mut v: Vec<_> = w.names.iter().map(|n| (n.name.clone(), n.scope, n.formula.clone(), n.hidden)).collect();
        v.sort();
        v
    };
    assert_eq!(key(&back), key(&wb));
    assert_eq!(back.sheets[0].print.print_area, RangeRef::parse("A1:C5"));
}

#[test]
fn xlsb_unresolvable_links_keep_the_saved_value_and_warn_once() {
    let calc = vec![
        // Another workbook's sheet (XTI 2 → BrtSupBookSrc).
        fmla(0, 42.0, &ref3d(2, 0, 0, true)),
        // An XTI index past BrtExternSheet.
        fmla(0, 1.0, &ref3d(99, 0, 0, true)),
        // Name index 0 and past the end.
        fmla(0, 2.0, &name_ptg(0)),
        fmla(0, 3.0, &name_ptg(9)),
        // A truncated PtgArea3d.
        fmla(0, 4.0, &area3d(0, 0, 1, 0, 0, true)[..10]),
    ];
    let names = vec![name_record(0, WORKBOOK, "Elsewhere", &ref3d(2, 0, 0, false))];
    let (wb, report) = read_xlsb(&names_book(&calc, &names)).unwrap();
    for (r, cached) in [42.0, 1.0, 2.0, 3.0, 4.0].into_iter().enumerate() {
        let pos = CellRef::new(r as u32, 0);
        assert_eq!(wb.sheets[1].value(pos), Value::Number(cached));
        assert!(wb.sheets[1].cell(pos).unwrap().formula.is_none());
    }
    let kept: Vec<_> = report.warnings.iter().filter(|w| w.contains("XLSB formula(s)")).collect();
    assert_eq!(kept.len(), 1, "{:?}", report.warnings);
    assert!(kept[0].starts_with("5 "), "{kept:?}");
    assert!(!kept[0].contains("defined names") && !kept[0].contains("other sheets"), "{kept:?}");
    // A name pointing into another workbook is skipped with a warning.
    assert!(wb.names.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("defined names")), "{:?}", report.warnings);
}

#[test]
fn xlsb_malformed_name_records_do_not_shift_name_indexes() {
    // A truncated BrtName keeps its slot, so PtgName 2 still means `Rate`.
    let broken = record(39, &[0, 0, 0, 0, 0]);
    let names = vec![broken, name_record(0, WORKBOOK, "Rate", &ref3d(0, 0, 1, false))];
    let calc = vec![fmla(0, 5.0, &[name_ptg(2), int(10), vec![MUL]].concat()), fmla(0, 6.0, &name_ptg(1))];
    let (wb, report) = read_xlsb(&names_book(&calc, &names)).unwrap();
    assert_eq!(ftext(&wb, 1, "A1").as_deref(), Some("Rate*10"));
    assert_eq!(ftext(&wb, 1, "A2"), None);
    assert_eq!(wb.names.len(), 1);
    assert!(report.warnings.iter().any(|w| w.contains("defined names could not be read")), "{:?}", report.warnings);
}

#[test]
fn xlsb_repairs_sheet_names_before_decoding_references_and_names() {
    let long = "界".repeat(40);
    let other = "界".repeat(35);
    let names = ["A:/?*[]\\B", "ab", "", &long, &other];
    let data = worksheet(&[(0, vec![number(0, 7.0)])]);
    let calc = worksheet(&[(0, vec![fmla(0, 7.0, &ref3d(0, 0, 0, true))])]);
    let defined = name_record(0, WORKBOOK, "Source", &ref3d(0, 0, 0, false));
    let bytes = package(&book(&names, &externals(), &[defined]), &[data.clone(), calc, data.clone(), data.clone(), data]);
    let (wb, report) = read_xlsb(&bytes).unwrap();
    let repaired: Vec<_> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(repaired, ["AB", "ab (2)", "Sheet3", &"界".repeat(31), &format!("{} (2)", "界".repeat(25))]);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("sheet name")).count(), 1);
    assert_eq!(ftext(&wb, 1, "A1").as_deref(), Some("AB!A1"));
    assert_eq!(wb.names.iter().find(|n| n.name == "Source").unwrap().formula, "AB!$A$1");
    let (roundtrip, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(ftext(&roundtrip, 1, "A1").as_deref(), Some("AB!A1"));
    assert_eq!(roundtrip.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), repaired);
}
