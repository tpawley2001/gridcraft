//! XLSB hyperlinks (BrtHLink) and notes (the comments part), built record by record from the
//! public [MS-XLSB] layout.
use gridcraft_core::{CellRef, Value};
use gridcraft_model::Hyperlink;

use super::xlsb_tests::{RELS, ROOT, book, cell, record, row, sst, wide, worksheet};
use super::{make_zip, minimal};
use crate::{read_xlsb, read_xlsx};

const HLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const COMMENTS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/><Override PartName="/notes/comments1.bin" ContentType="application/vnd.ms-excel.comments"/></Types>"#;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}
fn rfx(r0: u32, r1: u32, c0: u32, c1: u32) -> Vec<u8> {
    [r0, r1, c0, c1].iter().flat_map(|n| n.to_le_bytes()).collect()
}
fn null_wide() -> Vec<u8> {
    u32::MAX.to_le_bytes().to_vec()
}
fn hlink(r: u32, c: u32, rel: Option<&str>, location: &str, tooltip: &str) -> Vec<u8> {
    let mut out = rfx(r, r, c, c);
    out.extend(rel.map_or_else(null_wide, wide));
    out.extend(wide(location));
    out.extend(wide(tooltip));
    out.extend(wide(""));
    record(494, &out)
}
/// A comments part: BrtBeginComments, the authors, then one comment per `(iauthor, cell, text)`.
fn comments(authors: &[&str], notes: &[(u32, &str, &str)]) -> Vec<u8> {
    let mut out = record(628, &[]);
    out.extend(record(630, &[]));
    for a in authors {
        out.extend(record(632, &wide(a)));
    }
    out.extend(record(631, &[]));
    out.extend(record(633, &[]));
    for (author, cell, text) in notes {
        let c = at(cell);
        let mut begin = author.to_le_bytes().to_vec();
        begin.extend(rfx(c.row, c.row, c.col, c.col));
        begin.extend([0x5a; 16]);
        out.extend(record(635, &begin));
        let mut rich = vec![0];
        rich.extend(wide(text));
        out.extend(record(637, &rich));
        out.extend(record(636, &[]));
    }
    out.extend(record(634, &[]));
    out.extend(record(629, &[]));
    out
}
fn sheet(tail: &[u8]) -> Vec<u8> {
    let mut data = row(0);
    data.extend(cell(5, 0, &7.0f64.to_le_bytes()));
    worksheet(&data, tail)
}
/// The standard test package plus the first sheet's relationships and an optional comments part.
fn package_with_sheet_rels(sheet: &[u8], sheet_rels: &str, comments: Option<&[u8]>) -> Vec<u8> {
    let (book, strings, two) = (book(false), sst(), worksheet(&[], &[]));
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", sheet),
        ("data/_rels/one.bin.rels", sheet_rels.as_bytes()),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
    ];
    if let Some(c) = comments {
        parts.push(("notes/comments1.bin", c));
    }
    make_zip(&parts)
}
fn rels(entries: &[(&str, &str, &str, bool)]) -> String {
    let mut out = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target, external) in entries {
        let mode = if *external { r#" TargetMode="External""# } else { "" };
        out.push_str(&format!(r#"<Relationship Id="{id}" Type="{ty}" Target="{target}"{mode}/>"#));
    }
    out.push_str("</Relationships>");
    out
}
const NOTES_RELS: &[(&str, &str, &str, bool)] = &[("c1", COMMENTS_REL, "../notes/comments1.bin", false)];

#[test]
fn xlsb_external_hyperlink_matches_the_xlsx_reader() {
    let url = "https://example.com/a?b=1";
    let bytes = package_with_sheet_rels(&sheet(&hlink(1, 1, Some("rId1"), "", "Open it")), &rels(&[("rId1", HLINK, url, true)]), None);
    let (wb, _) = read_xlsb(&bytes).unwrap();
    let got = wb.sheets[0].hyperlinks.get(&at("B2")).unwrap();
    assert_eq!(got, &Hyperlink { target: url.into(), tooltip: Some("Open it".into()), external: true, display: None });

    let body = r#"<sheetData/><hyperlinks><hyperlink ref="B2" r:id="rId1" tooltip="Open it"/></hyperlinks>"#;
    let xml_rels = rels(&[("rId1", HLINK, url, true)]);
    let (xwb, _) = read_xlsx(&minimal(body, &[("xl/worksheets/_rels/sheet1.xml.rels", &xml_rels)], "", "")).unwrap();
    assert_eq!(xwb.sheet(0).unwrap().hyperlinks.get(&at("B2")), Some(got));
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(7.0));
}

#[test]
fn xlsb_internal_hyperlink_uses_its_location() {
    let (wb, _) = read_xlsb(&package_with_sheet_rels(&sheet(&hlink(2, 2, None, "Sheet2!A1", "")), &rels(&[]), None)).unwrap();
    assert_eq!(
        wb.sheets[0].hyperlinks.get(&at("C3")),
        Some(&Hyperlink { target: "Sheet2!A1".into(), tooltip: None, external: false, display: None })
    );
}

#[test]
fn xlsb_hyperlink_with_unknown_relationship_is_skipped_with_a_warning() {
    let mut tail = hlink(0, 0, Some("missing"), "", "");
    tail.extend(hlink(3, 0, Some("rId1"), "", ""));
    tail.extend(hlink(4, 0, None, "Data!B2", ""));
    tail.extend(record(494, &[1, 2, 3])); // Truncated: skipped, not fatal.
    let bytes = package_with_sheet_rels(&sheet(&tail), &rels(&[("rId1", HLINK, "mailto:me@example.com", true)]), None);
    let (wb, report) = read_xlsb(&bytes).unwrap();
    let links = &wb.sheets[0].hyperlinks;
    assert_eq!(links.len(), 2);
    assert_eq!(links.get(&at("A4")).unwrap().target, "mailto:me@example.com");
    assert_eq!(links.get(&at("A5")).unwrap().target, "Data!B2");
    assert!(report.warnings.iter().any(|w| w.contains("missing relationship")), "{:?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w.contains("Malformed XLSB hyperlinks")), "{:?}", report.warnings);
}

#[test]
fn xlsb_comments_import_authors_and_text() {
    let part = comments(&["Ann", "Bo"], &[(0, "A1", "Ann:\nCheck this"), (1, "D4", "Second note")]);
    let (wb, report) = read_xlsb(&package_with_sheet_rels(&sheet(&[]), &rels(NOTES_RELS), Some(&part))).unwrap();
    let notes = &wb.sheets[0].comments;
    assert_eq!(notes.len(), 2);
    let a1 = notes.get(&at("A1")).unwrap();
    assert_eq!((a1.author.as_str(), a1.text.as_str(), a1.threaded), ("Ann", "Check this", false));
    let d4 = notes.get(&at("D4")).unwrap();
    assert_eq!((d4.author.as_str(), d4.text.as_str()), ("Bo", "Second note"));
    assert!(!report.warnings.iter().any(|w| w.contains("comments")), "{:?}", report.warnings);
}

#[test]
fn xlsb_comment_with_unknown_author_keeps_its_text() {
    let part = comments(&["Ann"], &[(9, "B2", "Orphan")]);
    let (wb, report) = read_xlsb(&package_with_sheet_rels(&sheet(&[]), &rels(NOTES_RELS), Some(&part))).unwrap();
    let note = wb.sheets[0].comments.get(&at("B2")).unwrap();
    assert_eq!((note.author.as_str(), note.text.as_str()), ("", "Orphan"));
    assert!(report.warnings.iter().any(|w| w.contains("unknown author")), "{:?}", report.warnings);
}

#[test]
fn xlsb_damaged_comments_part_keeps_the_sheet() {
    let mut part = comments(&["Ann"], &[(0, "A1", "Note")]);
    part.truncate(part.len() - 6);
    part.extend([0x82, 0x05, 0x7f]); // A record that claims more bytes than remain.
    for bytes in [
        package_with_sheet_rels(&sheet(&[]), &rels(NOTES_RELS), Some(&part)),
        package_with_sheet_rels(&sheet(&[]), &rels(NOTES_RELS), None),
        package_with_sheet_rels(&sheet(&[]), "<not xml", None),
    ] {
        let (wb, report) = read_xlsb(&bytes).unwrap();
        assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(7.0));
        assert!(wb.sheets[0].comments.is_empty());
        assert!(
            report.warnings.iter().any(|w| w.contains("XLSB") && (w.contains("comments") || w.contains("relationships"))),
            "{:?}",
            report.warnings
        );
    }
}
