//! `.xlsb` Excel 4.0 macro sheets ([MS-XLSB] Macro Sheet part, relationship `xlMacrosheet` or
//! `xlIntlMacrosheet`) are kept as `xl/macrosheets/*.xml` parts: written back in place into a
//! macro-enabled save, left out of a plain `.xlsx`. The packages are built in code.

use std::collections::HashMap;
use std::io::Read;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::Visibility;

use super::make_zip;
use super::xlsb_tests::{ROOT, cell, ptg_int, record, row, sst, wide, worksheet};
use crate::{XlsxKind, read_xlsb, read_xlsx, write_xlsx_as};

const KEPT: &str = "macro sheet \"Macro1\" is kept as it is but can't be shown or edited; kept only when saved as a macro-enabled workbook";
const UNREAD: &str = "Some Excel 4.0 macro formulas could not be read and were kept as values.";
const SKIPPED: &str = "Excel 4.0 macro sheets that can't be read were skipped.";
const REL_MACRO: &str = "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet";
const REL_INTL: &str = "http://schemas.microsoft.com/office/2006/relationships/xlIntlMacrosheet";

/// A formula cell record (9 = BrtFmlaNum) with its cached number and rgce.
fn fmla_num(col: u32, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(cached.to_le_bytes());
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(9, &data)
}

/// Macro1's records: A1 = 5, A2 = "x", A3 = `=1+1` (cached 2).
fn macro_cells() -> Vec<u8> {
    let mut d = row(0);
    d.extend(cell(5, 0, &5.0f64.to_le_bytes()));
    d.extend(row(1));
    d.extend(cell(6, 0, &wide("x")));
    d.extend(row(2));
    d.extend(fmla_num(0, 2.0, &[ptg_int(1), ptg_int(1), vec![0x03]].concat()));
    d
}

fn one_sheet(n: f64) -> Vec<u8> {
    let mut d = row(0);
    d.extend(cell(5, 0, &n.to_le_bytes()));
    worksheet(&d, &[])
}

/// Sheet1 (worksheet, A1 = 1), Macro1 (macro sheet `kind`, state `state`) and Sheet2.
fn package(kind: &str, state: u32, macro_part: &[u8]) -> Vec<u8> {
    let mut book = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    book.extend(record(153, &props));
    book.extend(record(143, &[]));
    for (id, rel, name, st) in [(1u32, "one", "Sheet1", 0u32), (2, "mac", "Macro1", state), (3, "two", "Sheet2", 0)] {
        let mut data = st.to_le_bytes().to_vec();
        data.extend(id.to_le_bytes());
        data.extend(wide(rel));
        data.extend(wide(name));
        book.extend(record(156, &data));
    }
    book.extend(record(144, &[]));
    book.extend(record(132, &[]));
    let ws = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="{ws}" Target="../data/one.bin"/><Relationship Id="mac" Type="http://schemas.microsoft.com/office/2006/relationships/{kind}" Target="../data/mac.bin"/><Relationship Id="two" Type="{ws}" Target="../data/two.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/></Relationships>"#
    );
    let ct = if kind == "xlIntlMacrosheet" { "intlmacrosheet" } else { "macrosheet" };
    let types = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/mac.bin" ContentType="application/vnd.ms-excel.{ct}"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/></Types>"#
    );
    let (one, two, strings) = (one_sheet(1.0), one_sheet(2.0), sst());
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", rels.as_bytes()),
        ("data/one.bin", &one),
        ("data/mac.bin", macro_part),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
    ])
}

fn plain() -> Vec<u8> {
    package("xlMacrosheet", 0, &worksheet(&macro_cells(), &[]))
}

fn unpack(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = vec![];
        f.read_to_end(&mut v).unwrap();
        parts.insert(f.name().to_string(), String::from_utf8_lossy(&v).into_owned());
    }
    parts
}

/// `(name, state)` of every `<sheet>` of the saved workbook.
fn sheet_entries(p: &HashMap<String, String>) -> Vec<(String, Option<String>)> {
    let wb = &p["xl/workbook.xml"];
    wb.match_indices("<sheet ")
        .map(|(i, _)| {
            let tag = &wb[i..i + wb[i..].find('>').unwrap()];
            let attr = |a: &str| tag.find(a).map(|j| tag[j + a.len()..].split('"').next().unwrap().to_string());
            (attr("name=\"").unwrap(), attr("state=\""))
        })
        .collect()
}

/// The one kept macro sheet part: `(name, xml)`.
fn macro_part(p: &HashMap<String, String>) -> (&String, &String) {
    let parts: Vec<_> = p.iter().filter(|(k, _)| k.starts_with("xl/macrosheets/") && k.ends_with(".xml")).collect();
    assert_eq!(parts.len(), 1, "{:?}", p.keys().collect::<Vec<_>>());
    parts[0]
}

fn names(p: &HashMap<String, String>) -> Vec<String> {
    sheet_entries(p).into_iter().map(|(n, _)| n).collect()
}

fn sheet_names(wb: &gridcraft_model::Workbook) -> Vec<&str> {
    wb.sheets.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn xlsb_macro_sheet_is_kept_in_a_macro_enabled_save() {
    let (wb, report) = read_xlsb(&plain()).unwrap();
    assert_eq!(sheet_names(&wb), ["Sheet1", "Sheet2"]);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("Macro1")).count(), 1, "{:?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w == KEPT), "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("skipped") && w.contains("sheets")), "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w == UNREAD), "{:?}", report.warnings);

    let out = write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap();
    let p = unpack(&out);
    let (name, xml) = macro_part(&p);
    assert!(xml.contains("<xm:macrosheet"), "{xml}");
    assert!(xml.contains("<c r=\"A1\"><v>5</v></c>"), "{xml}");
    assert!(xml.contains("<c r=\"A2\" t=\"inlineStr\"><is><t xml:space=\"preserve\">x</t></is></c>"), "{xml}");
    assert!(xml.contains("<c r=\"A3\"><f>1+1</f><v>2</v></c>"), "{xml}");
    let ct = &p["[Content_Types].xml"];
    assert!(ct.contains(&format!("PartName=\"/{name}\" ContentType=\"application/vnd.ms-excel.macrosheet+xml\"")), "{ct}");
    let rels = &p["xl/_rels/workbook.xml.rels"];
    assert!(rels.contains(&format!("Type=\"{REL_MACRO}\"")) && rels.contains(name.strip_prefix("xl/").unwrap()), "{rels}");
    assert_eq!(sheet_entries(&p), [("Sheet1".to_string(), None), ("Macro1".to_string(), None), ("Sheet2".to_string(), None)]);

    // Reading the .xlsm back keeps the macro sheet again (the .xlsx reader's path).
    let (wb2, report2) = read_xlsx(&out).unwrap();
    assert_eq!(sheet_names(&wb2), ["Sheet1", "Sheet2"]);
    assert_eq!(wb2.sheets[0].value(CellRef::new(0, 0)), Value::Number(1.0));
    assert_eq!(wb2.sheets[1].value(CellRef::new(0, 0)), Value::Number(2.0));
    let kept = wb2.preserved.as_deref().unwrap();
    let ms: Vec<_> = kept.sheets.iter().map(|s| (s.name.as_str(), s.visibility, s.before)).collect();
    assert_eq!(ms, [("Macro1", Visibility::Visible, 1)]);
    assert!(report2.warnings.iter().any(|w| w == KEPT), "{:?}", report2.warnings);
    let p2 = unpack(&write_xlsx_as(&wb2, XlsxKind::MacroWorkbook).unwrap());
    assert_eq!(names(&p2), ["Sheet1", "Macro1", "Sheet2"]);
    assert!(p2.iter().any(|(k, x)| k.starts_with("xl/macrosheets/") && x.contains("<f>1+1</f>")));
}

#[test]
fn xlsb_macro_sheet_is_dropped_from_a_plain_xlsx() {
    let (wb, _) = read_xlsb(&plain()).unwrap();
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::Workbook).unwrap());
    assert!(!p.keys().any(|k| k.starts_with("xl/macrosheets/")), "{:?}", p.keys().collect::<Vec<_>>());
    assert_eq!(names(&p), ["Sheet1", "Sheet2"]);
}

#[test]
fn xlsb_hidden_macro_sheet_stays_hidden() {
    let (wb, _) = read_xlsb(&package("xlMacrosheet", 1, &worksheet(&macro_cells(), &[]))).unwrap();
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    assert_eq!(sheet_entries(&p)[1], ("Macro1".to_string(), Some("hidden".to_string())));
}

#[test]
fn xlsb_international_macro_sheet_keeps_its_types() {
    let (wb, report) = read_xlsb(&package("xlIntlMacrosheet", 0, &worksheet(&macro_cells(), &[]))).unwrap();
    assert_eq!(sheet_names(&wb), ["Sheet1", "Sheet2"]);
    assert!(report.warnings.iter().any(|w| w == KEPT), "{:?}", report.warnings);
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    let (name, xml) = macro_part(&p);
    assert!(xml.contains("<xm:macrosheet") && xml.contains("<f>1+1</f>"), "{xml}");
    let ct = &p["[Content_Types].xml"];
    assert!(ct.contains(&format!("PartName=\"/{name}\" ContentType=\"application/vnd.ms-excel.intlmacrosheet+xml\"")), "{ct}");
    let rels = &p["xl/_rels/workbook.xml.rels"];
    assert!(rels.contains(&format!("Type=\"{REL_INTL}\"")) && !rels.contains(REL_MACRO), "{rels}");
    assert_eq!(names(&p), ["Sheet1", "Macro1", "Sheet2"]);
}

#[test]
fn xlsb_truncated_macro_sheet_is_skipped() {
    let full = worksheet(&macro_cells(), &[]);
    // Cut inside the A3 formula cell record.
    let cut = full.len() - 30;
    let (wb, report) = read_xlsb(&package("xlMacrosheet", 0, &full[..cut])).unwrap();
    assert_eq!(sheet_names(&wb), ["Sheet1", "Sheet2"]);
    assert_eq!(report.warnings.iter().filter(|w| w.as_str() == SKIPPED).count(), 1, "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w == KEPT), "{:?}", report.warnings);
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    assert!(!p.keys().any(|k| k.starts_with("xl/macrosheets/")));
    assert_eq!(names(&p), ["Sheet1", "Sheet2"]);
}

#[test]
fn xlsb_macro_formula_that_cant_be_read_keeps_its_value() {
    // PtgFuncVar with fCeFunc set (a command-equivalent function), twice: one warning.
    let ce = |tab: u16| vec![0x42, 0, (tab & 0xff) as u8, ((tab >> 8) as u8) | 0x80];
    let mut d = row(0);
    d.extend(fmla_num(0, 5.0, &ce(1)));
    d.extend(row(1));
    d.extend(fmla_num(0, 6.0, &ce(2)));
    let (wb, report) = read_xlsb(&package("xlMacrosheet", 0, &worksheet(&d, &[]))).unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.as_str() == UNREAD).count(), 1, "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("XLSB formula(s)")), "{:?}", report.warnings);
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    let (_, xml) = macro_part(&p);
    assert!(xml.contains("<c r=\"A1\"><v>5</v></c>") && xml.contains("<c r=\"A2\"><v>6</v></c>"), "{xml}");
}
