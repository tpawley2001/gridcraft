//! Dialog sheets and Excel 4.0 macro sheets survive open + save in their place among the
//! workbook's sheets, the way chart sheets do (#727); macro sheets only in macro-enabled saves.
//! Packages are built in code.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use super::make_zip;
use crate::{XlsxKind, read_xlsx, write_xlsx_as};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const MACRO_REL: &str = "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet";

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.ms-excel.sheet.macroEnabled.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/dialogsheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml"/><Override PartName="/xl/macrosheets/sheet1.xml" ContentType="application/vnd.ms-excel.macrosheet+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

/// Sheets in order: Data (worksheet), Dialog1 (dialog sheet), Macro1 (macro sheet, hidden),
/// Last (worksheet). A sheet-scoped name on Last (`localSheetId` 3) and a formula on Data that
/// reads the macro sheet.
fn package() -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Dialog1" sheetId="2" r:id="rId2"/><sheet name="Macro1" sheetId="3" state="hidden" r:id="rId3"/><sheet name="Last" sheetId="4" r:id="rId4"/></sheets><definedNames><definedName name="LastCell" localSheetId="3">Last!$A$1</definedName></definedNames></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{NS_R}/dialogsheet" Target="dialogsheets/sheet1.xml"/><Relationship Id="rId3" Type="{MACRO_REL}" Target="macrosheets/sheet1.xml"/><Relationship Id="rId4" Type="{NS_R}/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><f>Macro1!A1</f><v>0</v></c></row></sheetData></worksheet>"#
    );
    let sheet2 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>4</v></c></row></sheetData></worksheet>"#
    );
    let dialog = format!(
        r#"<?xml version="1.0"?><dialogsheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetPr codeName="DIALOG-MARKER"/><sheetViews><sheetView workbookViewId="0"/></sheetViews><sheetFormatPr defaultRowHeight="15"/><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></dialogsheet>"#
    );
    let macro_sheet = format!(
        r#"<?xml version="1.0"?><xm:macrosheet xmlns="{NS_MAIN}" xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main" xmlns:r="{NS_R}"><sheetPr codeName="MACRO-MARKER"/><dimension ref="A1:A2"/><sheetData><row r="1"><c r="A1" t="str"><f>ECHO(FALSE)</f><v></v></c></row><row r="2"><c r="A2" t="str"><f>RETURN()</f><v></v></c></row></sheetData></xm:macrosheet>"#
    );
    make_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
        ("xl/dialogsheets/sheet1.xml", dialog.as_bytes()),
        ("xl/macrosheets/sheet1.xml", macro_sheet.as_bytes()),
    ])
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

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    Some(s[i..i + s[i..].find(b)?].to_string())
}

/// `<sheet>` entries of the saved workbook: (name, state, relationship type, part name, part text).
fn sheets(p: &HashMap<String, String>) -> Vec<(String, Option<String>, String, String)> {
    let wb = &p["xl/workbook.xml"];
    let rels = &p["xl/_rels/workbook.xml.rels"];
    let mut out = Vec::new();
    for (i, _) in wb.match_indices("<sheet ") {
        let tag = &wb[i..i + wb[i..].find('>').unwrap()];
        let name = between(tag, "name=\"", "\"").unwrap();
        let state = between(tag, "state=\"", "\"");
        let rid = between(tag, "r:id=\"", "\"").unwrap();
        let ri = rels.find(&format!("Id=\"{rid}\"")).unwrap_or_else(|| panic!("no workbook relationship {rid}"));
        let start = rels[..ri].rfind('<').unwrap();
        let r = &rels[start..start + rels[start..].find("/>").unwrap()];
        let ty = between(r, "Type=\"", "\"").unwrap();
        let target = between(r, "Target=\"", "\"").unwrap();
        let part = target.strip_prefix('/').map(str::to_string).unwrap_or_else(|| format!("xl/{target}"));
        out.push((name, state, ty, part));
    }
    out
}

fn names_types(p: &HashMap<String, String>) -> Vec<(String, String)> {
    sheets(p).into_iter().map(|(n, _, t, _)| (n, t.rsplit('/').next().unwrap().to_string())).collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn macro_enabled_save_keeps_both_in_place() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let out = write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap();
    let p = unpack(&out);
    assert_eq!(names_types(&p), pairs(&[("Data", "worksheet"), ("Dialog1", "dialogsheet"), ("Macro1", "xlMacrosheet"), ("Last", "worksheet")]));
    let all = sheets(&p);
    assert_eq!(all[2].1.as_deref(), Some("hidden"), "macro sheet keeps its hidden state");
    let ct = &p["[Content_Types].xml"];
    for (marker, ct_kind, idx) in [("DIALOG-MARKER", "spreadsheetml.dialogsheet+xml", 1), ("MACRO-MARKER", "ms-excel.macrosheet+xml", 2)] {
        let part = &all[idx].3;
        assert!(p.get(part).is_some_and(|x| x.contains(marker)), "{part} missing or changed");
        assert!(ct.contains(&format!("PartName=\"/{part}\" ContentType=\"application/vnd.")) && ct.contains(ct_kind), "content type for {part}");
    }
    // a sheet-scoped name still points at the fourth sheet
    let wbx = &p["xl/workbook.xml"];
    let dn = between(wbx, "<definedName name=\"LastCell\"", ">").expect("name LastCell lost");
    assert!(dn.contains("localSheetId=\"3\""), "LastCell scope moved: {dn}");
    // and a second save of the reopened file keeps them too
    let (wb2, _) = read_xlsx(&out).unwrap();
    let p2 = unpack(&write_xlsx_as(&wb2, XlsxKind::MacroWorkbook).unwrap());
    assert_eq!(names_types(&p2), names_types(&p));
}

#[test]
fn plain_xlsx_save_keeps_the_dialog_sheet_and_drops_the_macro_sheet() {
    let (wb, _) = read_xlsx(&package()).unwrap();
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::Workbook).unwrap());
    assert_eq!(names_types(&p), pairs(&[("Data", "worksheet"), ("Dialog1", "dialogsheet"), ("Last", "worksheet")]));
    assert!(!p.values().any(|x| x.contains("MACRO-MARKER")), "macro sheet written into an .xlsx");
    let wbx = &p["xl/workbook.xml"];
    let dn = between(wbx, "<definedName name=\"LastCell\"", ">").expect("name LastCell lost");
    assert!(dn.contains("localSheetId=\"2\""), "LastCell must follow its sheet to index 2: {dn}");
}

#[test]
fn worksheet_renamed_to_a_kept_sheet_name_wins() {
    let (mut wb, _) = read_xlsx(&package()).unwrap();
    let last = wb.sheets.len() - 1;
    Arc::make_mut(&mut wb.sheets[last]).name = "Dialog1".into();
    let p = unpack(&write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap());
    let nt = names_types(&p);
    let dialogs: Vec<_> = nt.iter().filter(|(n, _)| n == "Dialog1").collect();
    assert_eq!(dialogs.len(), 1, "two sheets named Dialog1: {nt:?}");
    assert_eq!(dialogs[0].1, "worksheet", "the edited worksheet keeps the name: {nt:?}");
}
