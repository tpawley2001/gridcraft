//! `.xls` links to other workbooks whose SupBook ([MS-XLS] 2.4.271) lists sheet names Excel does not
//! accept in an `.xlsx` (empty, over 31 characters, or holding `[ ] : * ? / \`): Save As `.xlsx`
//! writes valid stand-in names in the `externalLink` part (ECMA-376 Part 1 §18.14.6 `sheetName`) and
//! formulas into those sheets name the same stand-ins, so Excel opens the file without a repair.
use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

const LONG: &str = "A linked sheet name well over thirty-one characters";

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

/// XLUnicodeString with 8-bit characters.
fn xlstr(s: &str) -> Vec<u8> {
    cat(&[&(s.len() as u16).to_le_bytes(), &[0], s.as_bytes()])
}

/// External SupBook: `path` (XLUnicodeStringNoCch, 8-bit) then the sheet names.
fn supbook(path: &str, sheets: &[&str]) -> Vec<u8> {
    let mut p = cat(&[&u16s(&[sheets.len() as u16, path.len() as u16]), &[0], path.as_bytes()]);
    for s in sheets {
        p.extend(xlstr(s));
    }
    rec(0x01AE, &p)
}

/// PtgRef3d (reference class), relative row and column.
fn ref3d(ixti: u16, r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &u16s(&[ixti, r, c | 0xC000])])
}

/// FORMULA record with a numeric cached result.
fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// One link to `other.xls` with sheets `Data`, ``, `Q1:2024/Plan`, LONG, `Data`; a cached
/// `Q1:2024/Plan`!B2 = 42. Sheet1: A1 = [1]<sheet 2>!B2*2 (84), B1 = [1]<sheet 1>!B2 (0),
/// C1 = [1]<sheet 0>!B2 (0), D1 = Sheet1!C5 (0).
/// XTI 0 = Sheet1, 1 = link sheet 2, 2 = link sheet 1, 3 = link sheet 0.
fn file() -> Vec<u8> {
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &supbook("other.xls", &["Data", "", "Q1:2024/Plan", LONG, "Data"]),
            &rec(0x0059, &u16s(&[1, 2])),
            &rec(0x005A, &cat(&[&[1, 1], &1u16.to_le_bytes(), &[0x01], &42.0f64.to_le_bytes()])),
            &rec(0x0017, &cat(&[&u16s(&[4]), &u16s(&[0, 0, 0]), &u16s(&[1, 2, 2]), &u16s(&[1, 1, 1]), &u16s(&[1, 0, 0])])),
            &eof(),
        ])
    };
    let sheet = cat(&[
        &bof(0x0010),
        &formula(0, 0, 84.0, &cat(&[&ref3d(1, 1, 1), &[0x1E, 2, 0], &[0x05]])),
        &formula(0, 1, 0.0, &ref3d(2, 1, 1)),
        &formula(0, 2, 0.0, &ref3d(3, 1, 1)),
        &formula(0, 3, 0.0, &ref3d(0, 4, 2)),
        &eof(),
    ]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn saved_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).unwrap_or_else(|_| panic!("{name} missing"));
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    s
}

/// The `val`s of the part's `<sheetName>` elements, in order (XML entities left as written).
fn sheet_names(link: &str) -> Vec<String> {
    link.split("<sheetName val=\"").skip(1).map(|s| s.split('"').next().unwrap_or_default().to_string()).collect()
}

fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

#[test]
fn saved_link_sheet_names_are_valid_excel_sheet_names() {
    let (wb, report) = read_xls(&file()).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    let names = sheet_names(&link);
    assert_eq!(names, ["Data", "Sheet2", "Q12024Plan", "A linked sheet name well over t", "Data (2)"], "{link}\n{:?}", report.warnings);
    let mut seen = std::collections::HashSet::new();
    for n in &names {
        assert!(!n.is_empty() && n.chars().count() <= 31, "{n:?}");
        assert!(!n.contains(['[', ']', ':', '*', '?', '/', '\\']), "{n:?}");
        assert!(seen.insert(n.to_lowercase()), "duplicate {n:?}");
    }
}

#[test]
fn formulas_name_the_same_stand_ins_and_keep_their_values() {
    let (wb, report) = read_xls(&file()).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("[1]Q12024Plan!B2*2"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(84.0));
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("[1]Sheet2!B2"), "{:?}", report.warnings);
    assert_eq!(ftext(&wb, "C1").as_deref(), Some("[1]Data!B2"), "{:?}", report.warnings);
    assert_eq!(ftext(&wb, "D1").as_deref(), Some("Sheet1!C5"));
}

#[test]
fn stand_ins_survive_the_round_trip() {
    let (wb, _) = read_xls(&file()).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    // The cache stays keyed by the sheet's position.
    let cache = link.split("<sheetData sheetId=\"2\"").nth(1).and_then(|s| s.split("</sheetData>").next()).expect("cache of sheet 2");
    assert!(cache.contains(r#"<cell r="B2"><v>42</v></cell>"#), "{link}");
    let (back, _) = read_xlsx(&saved).unwrap();
    let f = |a1: &str| back.sheet(0).unwrap().cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(f("A1").as_deref(), Some("[1]Q12024Plan!B2*2"));
    assert_eq!(f("B1").as_deref(), Some("[1]Sheet2!B2"));
    assert_eq!(f("C1").as_deref(), Some("[1]Data!B2"));
}
