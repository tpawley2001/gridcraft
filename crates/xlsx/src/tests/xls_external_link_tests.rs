//! `.xls` links to other workbooks ([MS-XLS] SupBook 2.4.271, XCT 2.4.353, CRN 2.4.65): formulas
//! into a linked workbook keep their text as `[n]Sheet!A1` with their saved results, and the link
//! (path, sheet names, cached cells) is kept on Save As `.xlsx` as an `externalLink` part, as the
//! `.xlsx` and `.xlsb` readers keep it. The linked file is never opened.
use gridcraft_core::{CellRef, Value};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

const PATH_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath";

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

fn xct(ccrn: u16, itab: u16) -> Vec<u8> {
    rec(0x0059, &u16s(&[ccrn, itab]))
}

/// CRN for one row starting at `col`, with SerAr values already encoded.
fn crn(row: u16, col: u8, values: &[Vec<u8>]) -> Vec<u8> {
    let last = col + values.len() as u8 - 1;
    rec(0x005A, &cat(&[&[last, col], &row.to_le_bytes(), &values.concat()]))
}

fn ser_num(x: f64) -> Vec<u8> {
    cat(&[&[0x01], &x.to_le_bytes()])
}
fn ser_str(s: &str) -> Vec<u8> {
    cat(&[&[0x02], &xlstr(s)])
}
fn ser_bool(b: bool) -> Vec<u8> {
    cat(&[&[0x04, u8::from(b)], &[0; 7]])
}
fn ser_err(e: u8) -> Vec<u8> {
    cat(&[&[0x10, e], &[0; 7]])
}

/// PtgRef3d (reference class), relative row and column.
fn ref3d(ixti: u16, r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &u16s(&[ixti, r, c | 0xC000])])
}

/// FORMULA record with a numeric cached result.
fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// The linked workbook's cached cells: Data!B2 = 42, Data!C2 = "hi", Data!D2 = #N/A, Totals!B2 = TRUE.
fn cache() -> Vec<u8> {
    cat(&[&xct(1, 0), &crn(1, 1, &[ser_num(42.0), ser_str("hi"), ser_err(0x2A)]), &xct(1, 1), &crn(1, 1, &[ser_bool(true)])])
}

/// Sheet1 with A1 = [1]Data!B2*2 (84), B1 = Sheet1!C5 (0), C1 = SUM([1]Data:Totals!B2) (42).
/// XTI 0 = Sheet1, 1 = link Data, 2 = link Data:Totals.
fn file_with(path: &str, link_cache: &[u8]) -> Vec<u8> {
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &supbook(path, &["Data", "Totals"]),
            link_cache,
            &rec(0x0017, &cat(&[&u16s(&[3]), &u16s(&[0, 0, 0]), &u16s(&[1, 0, 0]), &u16s(&[1, 0, 1])])),
            &eof(),
        ])
    };
    let sheet = cat(&[
        &bof(0x0010),
        &formula(0, 0, 84.0, &cat(&[&ref3d(1, 1, 1), &[0x1E, 2, 0], &[0x05]])),
        &formula(0, 1, 0.0, &ref3d(0, 4, 2)),
        &formula(0, 2, 42.0, &cat(&[&ref3d(2, 1, 1), &[0x42, 1, 4, 0]])),
        &eof(),
    ]);
    let pos = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(pos), &sheet]))
}

fn file() -> Vec<u8> {
    file_with("other.xls", &cache())
}

fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

fn saved_text(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut all = String::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut s = String::new();
        if std::io::Read::read_to_string(&mut f, &mut s).is_ok() {
            all.push_str(f.name());
            all.push('\n');
            all.push_str(&s);
            all.push('\n');
        }
    }
    all
}

fn saved_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).unwrap_or_else(|_| panic!("{name} missing"));
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    s
}

/// The `Target` of the saved link's path relationship.
fn link_target(saved: &[u8]) -> String {
    let rels = saved_part(saved, "xl/externalLinks/_rels/externalLink1.xml.rels");
    assert!(rels.contains(PATH_REL), "{rels}");
    rels.split("Target=\"").nth(1).and_then(|s| s.split('"').next()).expect("Target").to_string()
}

#[test]
fn formula_into_a_linked_workbook_keeps_its_text_and_value() {
    let (wb, report) = read_xls(&file()).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("[1]Data!B2*2"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(84.0));
    assert_eq!(ftext(&wb, "C1").as_deref(), Some("SUM([1]Data:Totals!B2)"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(at("C1")), Value::Number(42.0));
    assert!(report.warnings.iter().any(|w| w.contains("external workbooks were kept")), "{:?}", report.warnings);
}

#[test]
fn internal_reference_next_to_a_link_is_unchanged() {
    let (wb, _) = read_xls(&file()).unwrap();
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("Sheet1!C5"));
}

#[test]
fn link_survives_save_as_xlsx() {
    let (wb, _) = read_xls(&file()).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let text = saved_text(&saved);
    assert!(text.contains("externalReference"), "workbook.xml lists the link: {text}");
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    assert!(link.contains(r#"<sheetName val="Data"/>"#) && link.contains(r#"<sheetName val="Totals"/>"#), "{link}");
    assert_eq!(link_target(&saved), "other.xls");
    let rels = saved_part(&saved, "xl/externalLinks/_rels/externalLink1.xml.rels");
    assert!(rels.contains(r#"TargetMode="External""#), "{rels}");
    let (back, _) = read_xlsx(&saved).unwrap();
    let f = |a1: &str| back.sheet(0).unwrap().cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(f("A1").as_deref(), Some("[1]Data!B2*2"));
    assert_eq!(f("C1").as_deref(), Some("SUM([1]Data:Totals!B2)"));
}

#[test]
fn cached_cells_are_saved_with_their_types() {
    let (wb, _) = read_xls(&file()).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    let data = link.split("<sheetData sheetId=\"0\"").nth(1).and_then(|s| s.split("</sheetData>").next()).expect("Data cache");
    assert!(data.contains(r#"<cell r="B2"><v>42</v></cell>"#), "{link}");
    assert!(data.contains(r#"<cell r="C2" t="str"><v>hi</v></cell>"#), "{link}");
    assert!(data.contains(r#"<cell r="D2" t="e"><v>#N/A</v></cell>"#), "{link}");
    let totals = link.split("<sheetData sheetId=\"1\"").nth(1).and_then(|s| s.split("</sheetData>").next()).expect("Totals cache");
    assert!(totals.contains(r#"<cell r="B2" t="b"><v>1</v></cell>"#), "{link}");
}

#[test]
fn encoded_paths_decode_to_plain_paths() {
    // 0x01 = encoded; 0x01 'C' = drive C:, 0x03 = separator, 0x04 = up one level.
    for (raw, want) in [("\u{1}\u{1}C\u{3}data\u{3}other.xls", r"C:\data\other.xls"), ("\u{1}\u{4}sub\u{3}y.xls", r"..\sub\y.xls")] {
        let (wb, report) = read_xls(&file_with(raw, &cache())).unwrap();
        assert_eq!(ftext(&wb, "A1").as_deref(), Some("[1]Data!B2*2"), "{raw:?}: {:?}", report.warnings);
        let target = link_target(&write_xlsx(&wb).unwrap());
        assert!(target.ends_with(want), "{raw:?} -> {target}");
        assert!(!target.chars().any(|c| (c as u32) < 0x20), "{raw:?} -> {target:?}");
    }
}

#[test]
fn truncated_cache_keeps_the_file_and_the_formulas() {
    // The CRN says three values but holds one and a half.
    let bad = cat(&[&xct(1, 0), &rec(0x005A, &cat(&[&[3, 1], &1u16.to_le_bytes(), &ser_num(42.0), &[0x01, 0, 0]]))]);
    let (wb, report) = read_xls(&file_with("other.xls", &bad)).unwrap();
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(84.0));
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("Sheet1!C5"));
    assert!(!report.warnings.is_empty());
    // Whatever was kept still saves as a workbook that opens again.
    read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
}

#[test]
fn workbook_without_links_saves_no_link_part() {
    let (wb, report) = read_xls(&super::xls_tests::one_sheet_with(&[])).unwrap();
    let text = saved_text(&write_xlsx(&wb).unwrap());
    assert!(!text.contains("externalLink"), "{text}");
    assert!(!report.warnings.iter().any(|w| w.contains("external workbooks")), "{:?}", report.warnings);
}

#[test]
fn a_links_cached_cells_and_names_feed_the_workbook_model() {
    // Calculation (`[1]Data!B2*2` = 84, `SUM([1]!Pair)` = 50) is tested in the engine.
    let (wb, report) = read_xls(&crate::testing::xls_links::workbook()).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("[1]Data!B2*2"), "{:?}", report.warnings);
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("SUM([1]!Pair)"), "{:?}", report.warnings);
    let [book] = wb.external_books.as_slice() else { panic!("one linked workbook: {:?}", wb.external_books) };
    let data = book.sheet("Data").expect("the link's sheet");
    assert_eq!((data.value(at("B2")), data.value(at("B3")), data.value(at("B4"))), (Value::Number(42.0), Value::Number(8.0), Value::Empty));
    assert_eq!(book.name("Pair", None).map(|n| n.refers_to.as_str()), Some("Data!$B$2:$B$3"));
    // The definition is kept in the link part saved as .xlsx, which reads back the same model.
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    assert!(link.contains(r#"<definedName name="Pair" refersTo="=Data!$B$2:$B$3"/>"#), "{link}");
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(back.external_books, wb.external_books);
}
