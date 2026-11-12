//! A cell format's quote prefix (`quotePrefix="1"` on `cellXfs/xf`, ECMA-376 Part 1 §18.8.45: the
//! text was typed with a leading apostrophe) survives open + save. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// cellXfs: 0 default; 1 quote prefix; 2 Text format + quote prefix; 3 bold; 4 bold + quote prefix;
/// 5 `quotePrefix="0"`.
const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="6"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" quotePrefix="1"/><xf numFmtId="49" fontId="0" fillId="0" borderId="0" xfId="0" quotePrefix="1" applyNumberFormat="1"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" quotePrefix="1" applyFont="1"/><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" quotePrefix="0"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#;

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// A1 `0123` (xf 1), A2 `=A1` as text (xf 2), A3 bold text (xf 3), A4 `007` bold (xf 4),
/// A5 plain text (xf 0), A6 text with `quotePrefix="0"` (xf 5).
fn package() -> Vec<u8> {
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="{CT}.styles+xml"/></Types>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let cell = |r: &str, s: u32, t: &str| format!(r#"<row r="{0}"><c r="A{0}" s="{s}" t="inlineStr"><is><t>{t}</t></is></c></row>"#, r);
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData>{}{}{}{}{}{}</sheetData></worksheet>"#,
        cell("1", 1, "0123"),
        cell("2", 2, "=A1"),
        cell("3", 3, "bold"),
        cell("4", 4, "007"),
        cell("5", 0, "plain"),
        cell("6", 5, "zero"),
    );
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "styles", "styles.xml")]);
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/styles.xml", STYLES.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

fn part(z: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> El {
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    xml::parse(s.as_bytes()).unwrap()
}

/// Open + save once: the saved package, its sheet root and its styles root.
fn round_trip(pkg: &[u8]) -> (Vec<u8>, El, El) {
    let out = write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out.clone())).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let (sh, st) = (part(&mut z, &name), part(&mut z, "xl/styles.xml"));
    (out, sh, st)
}

/// What the saved format of cell `a1` says: (quotePrefix, numFmtId, bold font).
fn format_of(a1: &str, sheet: &El, styles: &El) -> (Option<String>, u32, bool) {
    let cell = sheet
        .child("sheetData")
        .expect("sheetData")
        .kids("row")
        .flat_map(|r| r.kids("c"))
        .find(|c| c.attr("r") == Some(a1))
        .unwrap_or_else(|| panic!("no cell {a1}"));
    let s = cell.attr_u32("s").unwrap_or(0) as usize;
    let xfs: Vec<&El> = styles.child("cellXfs").expect("cellXfs").kids("xf").collect();
    let xf = xfs.get(s).unwrap_or_else(|| panic!("{a1}: s={s} out of range"));
    let fonts: Vec<&El> = styles.child("fonts").expect("fonts").kids("font").collect();
    let bold = fonts.get(xf.attr_u32("fontId").unwrap_or(0) as usize).is_some_and(|f| f.child("b").is_some());
    (xf.attr("quotePrefix").map(str::to_string), xf.attr_u32("numFmtId").unwrap_or(0), bold)
}

fn check(sheet: &El, styles: &El) {
    // (cell, quote prefix, numFmtId, bold, what it is)
    let cells = [
        ("A1", true, 0, false, "quote prefix on a General cell"),
        ("A2", true, 49, false, "quote prefix on a Text-format cell"),
        ("A3", false, 0, true, "bold without a quote prefix"),
        ("A4", true, 0, true, "bold with a quote prefix"),
        ("A5", false, 0, false, "plain cell"),
        ("A6", false, 0, false, "quotePrefix=\"0\" is not written back"),
    ];
    // The rest of each format is unchanged.
    for (a1, _, fmt, bold, what) in cells {
        let (_, f, b) = format_of(a1, sheet, styles);
        assert_eq!((f, b), (fmt, bold), "{a1}: {what} (numFmtId, bold)");
    }
    for (a1, prefix, _, _, what) in cells {
        let (q, _, _) = format_of(a1, sheet, styles);
        assert_eq!(q.as_deref(), prefix.then_some("1"), "{a1}: {what} (quotePrefix)");
    }
    // Named cell styles never carry the flag, and it is never written as 0.
    for xf in styles.child("cellStyleXfs").expect("cellStyleXfs").kids("xf") {
        assert_eq!(xf.attr("quotePrefix"), None, "cellStyleXfs");
    }
    for xf in styles.child("cellXfs").expect("cellXfs").kids("xf") {
        assert_ne!(xf.attr("quotePrefix"), Some("0"));
    }
}

#[test]
fn quote_prefix_survives_a_round_trip() {
    let (_, sheet, styles) = round_trip(&package());
    check(&sheet, &styles);
}

#[test]
fn quote_prefix_is_stable_over_a_second_save() {
    let (once, _, _) = round_trip(&package());
    let (_, sheet, styles) = round_trip(&once);
    check(&sheet, &styles);
}

#[test]
fn a_workbook_without_quote_prefixes_writes_none() {
    let pkg = package();
    let plain = STYLES.replace(r#" quotePrefix="1""#, "").replace(r#" quotePrefix="0""#, "");
    // Same package with the attribute removed from every xf.
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(pkg)).unwrap();
    let mut parts: Vec<(String, Vec<u8>)> = vec![];
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = vec![];
        f.read_to_end(&mut b).unwrap();
        let name = f.name().to_string();
        parts.push(if name == "xl/styles.xml" { (name, plain.clone().into_bytes()) } else { (name, b) });
    }
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
    let (_, _, styles) = round_trip(&make_zip(&refs));
    for xf in styles.child("cellXfs").expect("cellXfs").kids("xf") {
        assert_eq!(xf.attr("quotePrefix"), None);
    }
}
