//! `.xls` names defined in a linked workbook ([MS-XLS] EXTERNNAME 2.4.105 with an ExternDocName
//! body 2.5.100, used through PtgNameX): formulas keep their text as `[1]!Name` (or
//! `[1]Sheet!Name` for a sheet-level name) with their saved results, and the names are kept in the
//! link's `externalLink` part (ECMA-376 Part 1 §18.14.5 `definedNames`, §18.14.6 `definedName`)
//! on Save As `.xlsx`. The linked file is never opened.
use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

/// XLUnicodeString with 8-bit characters.
fn xlstr(s: &str) -> Vec<u8> {
    cat(&[&(s.len() as u16).to_le_bytes(), &[0], s.as_bytes()])
}

/// External SupBook: `path` (8-bit) then the sheet names.
fn supbook(path: &str, sheets: &[&str]) -> Vec<u8> {
    let mut p = cat(&[&u16s(&[sheets.len() as u16, path.len() as u16]), &[0], path.as_bytes()]);
    for s in sheets {
        p.extend(xlstr(s));
    }
    rec(0x01AE, &p)
}

/// ShortXLUnicodeString body: 8-bit (Latin-1) when every char fits, else UTF-16LE.
fn short_str(s: &str) -> Vec<u8> {
    if s.chars().all(|c| (c as u32) < 0x100) {
        let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
        cat(&[&[bytes.len() as u8, 0], &bytes])
    } else {
        let units: Vec<u16> = s.encode_utf16().collect();
        cat(&[&[units.len() as u8, 1], &u16s(&units)])
    }
}

/// EXTERNNAME with an ExternDocName body: flags, ixals, reserved, name, cce + rgce.
fn extern_name(flags: u16, ixals: u16, name: &str, rgce: &[u8]) -> Vec<u8> {
    rec(0x0023, &cat(&[&u16s(&[flags, ixals, 0]), &short_str(name), &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// PtgNameX, value class: XTI index and 1-based EXTERNNAME index.
fn namex(ixti: u16, index: u32) -> Vec<u8> {
    cat(&[&[0x59], &ixti.to_le_bytes(), &index.to_le_bytes()])
}

/// FORMULA record with a numeric cached result.
fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// PtgErr #REF!: a name definition Excel keeps for names of a linked workbook.
const REF_ERR: [u8; 2] = [0x1C, 0x17];

/// The link's names, in record order: 1 Rate (workbook, `=#REF!`), 2 Limit (sheet Totals, no
/// definition), 3 Größe (8-bit), 4 Été written as UTF-16.
fn names() -> Vec<u8> {
    // Force the UTF-16 form for a name that would fit in 8 bits.
    let units: Vec<u16> = "Été".encode_utf16().collect();
    let ete = rec(0x0023, &cat(&[&u16s(&[0, 0, 0]), &[units.len() as u8, 1], &u16s(&units), &0u16.to_le_bytes()]));
    cat(&[&extern_name(0, 0, "Rate", &REF_ERR), &extern_name(0, 2, "Limit", &[]), &extern_name(0, 0, "Größe", &[]), &ete])
}

/// Sheet1: A1 = [1]!Rate*2 (84), B1 = [1]Totals!Limit+1 (6), C1 = SUM([1]!Größe) (3),
/// D1 = [1]!Été (7), plus `extra` cells. XTI 0 = Sheet1, 1 = the link, workbook level.
fn file_with(link_names: &[u8], extra: &[u8]) -> Vec<u8> {
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &supbook("other.xls", &["Data", "Totals"]),
            link_names,
            &rec(0x0017, &cat(&[&u16s(&[2]), &u16s(&[0, 0, 0]), &u16s(&[1, 0xFFFE, 0xFFFE])])),
            &eof(),
        ])
    };
    let sheet = cat(&[
        &bof(0x0010),
        &formula(0, 0, 84.0, &cat(&[&namex(1, 1), &[0x1E, 2, 0], &[0x05]])),
        &formula(0, 1, 6.0, &cat(&[&namex(1, 2), &[0x1E, 1, 0], &[0x03]])),
        &formula(0, 2, 3.0, &cat(&[&[0x39], &1u16.to_le_bytes(), &3u32.to_le_bytes(), &[0x42, 1, 4, 0]])),
        &formula(0, 3, 7.0, &namex(1, 4)),
        extra,
        &eof(),
    ]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn file() -> Vec<u8> {
    file_with(&names(), &[])
}

fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

fn saved_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).unwrap_or_else(|_| panic!("{name} missing"));
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    s
}

#[test]
fn formulas_using_names_of_a_linked_workbook_keep_their_text_and_value() {
    let (wb, report) = read_xls(&file()).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("[1]!Rate*2"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(84.0));
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("[1]Totals!Limit+1"), "a sheet-level name: {:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("B1")), Value::Number(6.0));
    assert_eq!(ftext(&wb, "C1").as_deref(), Some("SUM([1]!Größe)"), "an 8-bit (Latin-1) name: {:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("C1")), Value::Number(3.0));
    assert_eq!(ftext(&wb, "D1").as_deref(), Some("[1]!Été"), "a UTF-16 name: {:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("D1")), Value::Number(7.0));
}

#[test]
fn names_are_kept_in_the_saved_link_part() {
    let (wb, _) = read_xls(&file()).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    assert!(link.contains(r#"<definedName name="Rate" refersTo="=#REF!"/>"#), "{link}");
    assert!(link.contains(r#"<definedName name="Limit" sheetId="1"/>"#), "sheet-level, no definition: {link}");
    assert!(link.contains(r#"<definedName name="Größe"/>"#), "{link}");
    assert!(link.contains(r#"<definedName name="Été"/>"#), "{link}");
    let (names, sheets) = (link.find("<definedNames>").expect("definedNames"), link.find("<sheetNames>").expect("sheetNames"));
    assert!(sheets < names, "sheetNames come first: {link}");
    if let Some(data) = link.find("<sheetDataSet>") {
        assert!(names < data, "definedNames before sheetDataSet: {link}");
    }
}

#[test]
fn saved_workbook_reads_back_with_the_same_formulas() {
    let (wb, _) = read_xls(&file()).unwrap();
    let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let f = |a1: &str| back.sheet(0).unwrap().cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(f("A1").as_deref(), Some("[1]!Rate*2"), "{:?}", report.warnings);
    assert_eq!(f("B1").as_deref(), Some("[1]Totals!Limit+1"), "{:?}", report.warnings);
    assert_eq!(f("C1").as_deref(), Some("SUM([1]!Größe)"), "{:?}", report.warnings);
    assert_eq!(f("D1").as_deref(), Some("[1]!Été"), "{:?}", report.warnings);
}

#[test]
fn an_empty_name_is_not_written_and_its_formula_keeps_its_value() {
    // Name 5 is empty; E1 = [1]!<empty> with cached 5.
    let link_names = cat(&[&names(), &extern_name(0, 0, "", &[])]);
    let (wb, report) = read_xls(&file_with(&link_names, &formula(0, 4, 5.0, &namex(1, 5)))).unwrap();
    assert_eq!(wb.sheets[0].value(at("E1")), Value::Number(5.0));
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("[1]!Rate*2"), "the other names still work: {:?}", report.warnings);
    let link = saved_part(&write_xlsx(&wb).unwrap(), "xl/externalLinks/externalLink1.xml");
    assert!(!link.contains(r#"name="""#), "{link}");
    assert!(link.contains(r#"name="Rate""#), "{link}");
}

#[test]
fn a_truncated_name_record_keeps_the_file_the_link_and_the_value() {
    // Name 1 says 40 characters but the record ends after 4.
    let bad = rec(0x0023, &cat(&[&u16s(&[0, 0, 0]), &[40, 0], b"Rate"]));
    let rest = cat(&[&extern_name(0, 2, "Limit", &[]), &extern_name(0, 0, "Größe", &[]), &extern_name(0, 0, "Été", &[])]);
    let (wb, _) = read_xls(&file_with(&cat(&[&bad, &rest]), &[])).unwrap();
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(84.0));
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("[1]Totals!Limit+1"), "later names keep their index");
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    assert!(link.contains(r#"<sheetName val="Data"/>"#), "{link}");
    read_xlsx(&saved).unwrap();
}

#[test]
fn a_link_without_names_writes_no_defined_names() {
    let (wb, _) = read_xls(&file_with(&[], &[])).unwrap();
    let link = saved_part(&write_xlsx(&wb).unwrap(), "xl/externalLinks/externalLink1.xml");
    assert!(!link.contains("definedName"), "{link}");
}
