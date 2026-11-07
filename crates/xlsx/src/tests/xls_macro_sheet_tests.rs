//! `.xls` Excel 4.0 macro sheets ([MS-XLS] BoundSheet8 `dt = 1`, BOF `dt = 0x0040`) are kept as
//! `xl/macrosheets/*.xml` parts: written back in place into a macro-enabled save, left out of a
//! plain `.xlsx`. The BIFF8 streams are built in code.

use std::collections::HashMap;
use std::io::Read;

use gridcraft_core::Value;
use gridcraft_model::Visibility;

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s};
use crate::{XlsxKind, read_xls, read_xlsx, write_xlsx_as};

const KEPT: &str = "macro sheet \"Macro1\" is kept as it is but can't be shown or edited; kept only when saved as a macro-enabled workbook";
const UNREAD: &str = "Some Excel 4.0 macro formulas could not be read and were kept as values.";

fn boundsheet(pos: u32, state: u8, dt: u8, name: &str) -> Vec<u8> {
    rec(0x0085, &cat(&[&pos.to_le_bytes(), &[state, dt, name.len() as u8, 0], name.as_bytes()]))
}

fn number(r: u16, c: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&cell(r, c, 0), &n.to_le_bytes()]))
}

fn formula(r: u16, c: u16, cached: f64, rgce: &[u8]) -> Vec<u8> {
    rec(0x0006, &cat(&[&cell(r, c, 0), &cached.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), rgce]))
}

/// PtgFuncVar (value class) calling Ftab entry `tab` with no arguments.
fn func_var(tab: u16) -> Vec<u8> {
    cat(&[&[0x42, 0], &tab.to_le_bytes()])
}

/// Data (worksheet), Macro1 (macro sheet, hidden) holding `macro_records`, Last (worksheet), and
/// a workbook name `Start` = Macro1!$A$1 (with `names`).
fn book(macro_records: &[u8], names: bool) -> Vec<u8> {
    let start = cat(&[&[0x3A], &1u16.to_le_bytes(), &u16s(&[0, 0])]);
    let lbl = rec(0x0018, &cat(&[&u16s(&[0]), &[0, 5], &u16s(&[start.len() as u16, 0, 0]), &[0; 4], &[0], b"Start", &start]));
    let globals = |pos: [u32; 3]| {
        let mut g = cat(&[&bof(0x0005), &boundsheet(pos[0], 0, 0, "Data"), &boundsheet(pos[1], 1, 1, "Macro1"), &boundsheet(pos[2], 0, 0, "Last")]);
        if names {
            g.extend(rec(0x01AE, &u16s(&[3, 0x0401])));
            g.extend(rec(0x0017, &cat(&[&u16s(&[2]), &u16s(&[0, 0, 0]), &u16s(&[0, 1, 1])])));
            g.extend(lbl.clone());
        }
        g.extend(eof());
        g
    };
    let data = cat(&[&bof(0x0010), &number(0, 0, 7.0), &eof()]);
    let macro_sheet = cat(&[&bof(0x0040), macro_records, &eof()]);
    let last = cat(&[&bof(0x0010), &number(0, 0, 9.0), &eof()]);
    let first = globals([0; 3]).len() as u32;
    let pos = [first, first + data.len() as u32, first + (data.len() + macro_sheet.len()) as u32];
    compound("Workbook", &cat(&[&globals(pos), &data, &macro_sheet, &last]))
}

fn returning_macro() -> Vec<u8> {
    cat(&[&number(0, 0, 1.0), &formula(1, 0, 0.0, &func_var(55))])
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

fn macro_parts(p: &HashMap<String, String>) -> Vec<(&String, &String)> {
    p.iter().filter(|(k, _)| k.starts_with("xl/macrosheets/") && k.ends_with(".xml")).collect()
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

fn entries(v: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
    v.iter().map(|(n, s)| (n.to_string(), s.map(str::to_string))).collect()
}

#[test]
fn xls_macro_sheet_is_kept_in_a_macro_enabled_save() {
    let (wb, report) = read_xls(&book(&returning_macro(), true)).unwrap();
    assert_eq!(wb.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Data", "Last"]);
    assert_eq!(report.warnings.iter().filter(|w| w.as_str() == KEPT).count(), 1, "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("skipped") && w.contains("macro")), "{:?}", report.warnings);
    // The name into the macro sheet survives.
    assert!(wb.names.iter().any(|n| n.name == "Start" && n.formula == "Macro1!$A$1"), "{:?}", wb.names);

    let out = write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap();
    let p = unpack(&out);
    let parts = macro_parts(&p);
    assert_eq!(parts.len(), 1, "{:?}", p.keys().collect::<Vec<_>>());
    let (name, xml) = parts[0];
    assert!(xml.contains("<xm:macrosheet") && xml.contains("xmlns:xm=\"http://schemas.microsoft.com/office/excel/2006/main\""), "{xml}");
    assert!(xml.contains("<c r=\"A1\"><v>1</v></c>"), "{xml}");
    assert!(xml.contains("<c r=\"A2\"><f>RETURN()</f><v>0</v></c>"), "{xml}");
    assert_eq!(sheet_entries(&p), entries(&[("Data", None), ("Macro1", Some("hidden")), ("Last", None)]));
    let rels = &p["xl/_rels/workbook.xml.rels"];
    let target = name.strip_prefix("xl/").unwrap();
    assert!(rels.contains("Type=\"http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet\"") && rels.contains(target), "{rels}");
    let ct = &p["[Content_Types].xml"];
    assert!(ct.contains(&format!("PartName=\"/{name}\" ContentType=\"application/vnd.ms-excel.macrosheet+xml\"")), "{ct}");
    assert!(p["xl/workbook.xml"].contains("Macro1!$A$1"), "name Start lost");

    // Reading the .xlsm back keeps the macro sheet again, and the worksheets are unchanged.
    let (wb2, report2) = read_xlsx(&out).unwrap();
    assert_eq!(wb2.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Data", "Last"]);
    assert_eq!(super::xls_tests::v(&wb2, 0, "A1"), Value::Number(7.0));
    assert_eq!(super::xls_tests::v(&wb2, 1, "A1"), Value::Number(9.0));
    let kept = wb2.preserved.as_deref().unwrap();
    let ms: Vec<_> = kept.sheets.iter().map(|s| (s.name.as_str(), s.visibility, s.before)).collect();
    assert_eq!(ms, [("Macro1", Visibility::Hidden, 1)]);
    assert!(report2.warnings.iter().any(|w| w == KEPT), "{:?}", report2.warnings);
    let p2 = unpack(&write_xlsx_as(&wb2, XlsxKind::MacroWorkbook).unwrap());
    assert_eq!(sheet_entries(&p2), sheet_entries(&p));
    assert!(macro_parts(&p2).iter().any(|(_, x)| x.contains("<f>RETURN()</f>")));
}

#[test]
fn xls_macro_sheet_is_dropped_from_a_plain_xlsx() {
    let (wb, report) = read_xls(&book(&returning_macro(), false)).unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.contains("macro sheet")).count(), 1, "{:?}", report.warnings);
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::Workbook).unwrap());
    assert!(macro_parts(&p).is_empty(), "{:?}", p.keys().collect::<Vec<_>>());
    assert!(!p.values().any(|x| x.contains("macrosheet") || x.contains("xlMacrosheet")));
    assert_eq!(sheet_entries(&p), entries(&[("Data", None), ("Last", None)]));
}

#[test]
fn xls_macro_formula_that_cant_be_read_keeps_its_value() {
    // PtgFuncVar with fCeFunc set and a value missing from [MS-XLS] Cetab (0x00A5, 1000), twice: one warning.
    let records =
        cat(&[&formula(0, 0, 5.0, &func_var(0x8000 | 0x00A5)), &formula(1, 0, 6.0, &func_var(0x8000 | 1000)), &formula(2, 0, 0.0, &func_var(55))]);
    let (wb, report) = read_xls(&book(&records, false)).unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.as_str() == UNREAD).count(), 1, "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.starts_with("Some formulas use")), "{:?}", report.warnings);
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    let parts = macro_parts(&p);
    let xml = parts[0].1;
    assert!(xml.contains("<c r=\"A1\"><v>5</v></c>") && xml.contains("<c r=\"A2\"><v>6</v></c>"), "{xml}");
    assert!(xml.contains("<c r=\"A3\"><f>RETURN()</f><v>0</v></c>"), "{xml}");
}

#[test]
fn xls_without_macro_sheets_has_no_macro_warnings() {
    let globals = |pos: u32| cat(&[&bof(0x0005), &boundsheet(pos, 0, 0, "Data"), &eof()]);
    let data = cat(&[&bof(0x0010), &number(0, 0, 7.0), &eof()]);
    let (wb, report) = read_xls(&compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &data]))).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("macro") || w.contains("Macro")), "{:?}", report.warnings);
    assert!(wb.preserved.as_deref().is_none_or(|p| p.sheets.is_empty()));
}

/// PtgFuncVar (value class) with `cparams` (argument count, bit 7 = fPrompt) and `tab`.
fn func_var_n(cparams: u8, tab: u16) -> Vec<u8> {
    cat(&[&[0x42, cparams], &tab.to_le_bytes()])
}

/// PtgInt `n`.
fn int(n: u16) -> Vec<u8> {
    cat(&[&[0x1E], &n.to_le_bytes()])
}

/// Command-equivalent functions ([MS-XLS] PtgFuncVar fCeFunc + Cetab): A2..A5 of the macro sheet.
fn command_macro() -> Vec<u8> {
    cat(&[
        &number(0, 0, 1.0),
        &formula(1, 0, 0.0, &func_var_n(0, 0x8000 | 0x00A4)),
        &formula(2, 0, 0.0, &cat(&[&int(1), &func_var_n(1, 0x8000)])),
        &formula(3, 0, 0.0, &func_var_n(0x80, 0x8000 | 0x0005)),
        &formula(4, 0, 0.0, &func_var_n(0, 0x8000 | 0x00A3)),
        &formula(5, 0, 0.0, &func_var(55)),
    ])
}

fn command_macro_xml() -> (String, Vec<String>) {
    let (wb, report) = read_xls(&book(&command_macro(), false)).unwrap();
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    let parts = macro_parts(&p);
    assert_eq!(parts.len(), 1, "{:?}", p.keys().collect::<Vec<_>>());
    (parts[0].1.clone(), report.warnings)
}

#[test]
fn xls_macro_command_functions_are_read_without_a_warning() {
    let (_, warnings) = command_macro_xml();
    assert!(!warnings.iter().any(|w| w.as_str() == UNREAD), "{warnings:?}");
    assert!(!warnings.iter().any(|w| w.starts_with("Some formulas use")), "{warnings:?}");
}

#[test]
fn xls_macro_command_functions_are_written_by_name() {
    let (xml, _) = command_macro_xml();
    assert!(xml.contains("<c r=\"A2\"><f>WINDOW.MAXIMIZE()</f>"), "{xml}");
    assert!(xml.contains("<c r=\"A5\"><f>WINDOW.RESTORE()</f>"), "{xml}");
    assert!(xml.contains("<c r=\"A6\"><f>RETURN()</f>"), "{xml}");
}

#[test]
fn xls_macro_command_function_arguments_are_kept() {
    let (xml, _) = command_macro_xml();
    assert!(xml.contains("<c r=\"A3\"><f>BEEP(1)</f>"), "{xml}");
}

#[test]
fn xls_macro_command_dialog_form_gets_a_question_mark() {
    // fPrompt (cparams bit 7) on a command equivalent = its dialog-box form; 0 arguments.
    let (xml, _) = command_macro_xml();
    assert!(xml.contains("<c r=\"A4\"><f>SAVE.AS?()</f>"), "{xml}");
}
