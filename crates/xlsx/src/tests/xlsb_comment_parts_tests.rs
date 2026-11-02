//! XLSB notes' VML shapes (BrtLegacyDrawing) and Excel 365 threaded comments, which an .xlsb
//! keeps in the same XML parts as an .xlsx. Each case is checked against the .xlsx reader given
//! the same parts.
use gridcraft_core::CellRef;
use gridcraft_model::Comment;

use super::make_zip;
use super::xlsb_tests::{ROOT, book, cell, record, row, sst, wide, worksheet};
use crate::{ReadReport, read_xlsb, read_xlsx};

const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const TC_REL: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
const PERSON_REL: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/><Override PartName="/notes/comments1.bin" ContentType="application/vnd.ms-excel.comments"/></Types>"#;

const VML: &str = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1025" type="#_x0000_t202" style="position:absolute;margin-left:80pt;margin-top:2pt;width:150pt;height:70pt;z-index:1"><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:Anchor>2, 15, 0, 10, 4, 15, 4, 4</x:Anchor><x:Visible/><x:Row>1</x:Row><x:Column>1</x:Column></x:ClientData></v:shape></xml>"##;
const THREADS: &str = r#"<ThreadedComments xmlns="http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments"><threadedComment ref="C3" dT="2026-03-04T05:06:07.00" personId="{A1}" id="{T1}" done="1"><text>Is this right?</text></threadedComment><threadedComment ref="C3" dT="2026-03-05T08:00:00.00" personId="{B2}" id="{T2}" parentId="{T1}"><text>Yes, checked.</text></threadedComment></ThreadedComments>"#;
const PERSONS: &str = r#"<personList xmlns="http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments"><person displayName="Ann Lee" id="{A1}" userId="ann" providerId="None"/><person displayName="Bo Kim" id="{B2}" userId="bo" providerId="None"/></personList>"#;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rels(entries: &[(&str, &str, &str)]) -> String {
    let mut out = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in entries {
        out.push_str(&format!(r#"<Relationship Id="{id}" Type="{ty}" Target="{target}"/>"#));
    }
    out.push_str("</Relationships>");
    out
}

/// A binary comments part with one note by `author` on `cell`.
fn binary_comments(author: &str, cell_ref: &str, text: &str) -> Vec<u8> {
    let c = at(cell_ref);
    let mut out = record(628, &[]);
    out.extend(record(630, &[]));
    out.extend(record(632, &wide(author)));
    out.extend(record(631, &[]));
    out.extend(record(633, &[]));
    let mut begin = 0u32.to_le_bytes().to_vec();
    begin.extend([c.row, c.row, c.col, c.col].iter().flat_map(|n| n.to_le_bytes()));
    begin.extend([0x5a; 16]);
    out.extend(record(635, &begin));
    let mut rich = vec![0];
    rich.extend(wide(text));
    out.extend(record(637, &rich));
    out.extend(record(636, &[]));
    out.extend(record(634, &[]));
    out.extend(record(629, &[]));
    out
}

/// The parts both packages share: an optional VML drawing, threaded comments and persons part.
#[derive(Default)]
struct Parts {
    vml: Option<&'static str>,
    threads: Option<&'static str>,
    persons: Option<&'static str>,
}

/// An .xlsb with one note (`author`, `cell`, `text`) and `parts`; `drawing` is the
/// BrtLegacyDrawing relationship id, whose relationship points at `notes/vmlDrawing1.vml`.
fn xlsb(note: (&str, &str, &str), parts: &Parts, drawing: Option<&str>) -> (Vec<(CellRef, Comment)>, ReadReport) {
    let mut data = row(0);
    data.extend(cell(5, 0, &7.0f64.to_le_bytes()));
    let tail = drawing.map(|id| record(551, &wide(id))).unwrap_or_default();
    let sheet = worksheet(&data, &tail);
    let mut sheet_rels = vec![("c1", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments", "../notes/comments1.bin")];
    if drawing.is_some() {
        sheet_rels.push(("vml1", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing", "../notes/vmlDrawing1.vml"));
    }
    if parts.threads.is_some() {
        sheet_rels.push(("tc1", TC_REL, "../threadedComments/threadedComment1.xml"));
    }
    let sheet_rels = rels(&sheet_rels);
    let ws = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
    let mut book_rels = vec![
        ("one", ws, "../data/one.bin"),
        ("two", ws, "../data/two.bin"),
        ("strings", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings", "../text/strings.bin"),
    ];
    if parts.persons.is_some() {
        book_rels.push(("p1", PERSON_REL, "../persons/person.xml"));
    }
    let book_rels = rels(&book_rels);
    let (book, strings, two, comments) = (book(false), sst(), worksheet(&[], &[]), binary_comments(note.0, note.1, note.2));
    let mut zip: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", book_rels.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/_rels/one.bin.rels", sheet_rels.as_bytes()),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
        ("notes/comments1.bin", &comments),
    ];
    if let Some(v) = parts.vml {
        zip.push(("notes/vmlDrawing1.vml", v.as_bytes()));
    }
    if let Some(t) = parts.threads {
        zip.push(("threadedComments/threadedComment1.xml", t.as_bytes()));
    }
    if let Some(p) = parts.persons {
        zip.push(("persons/person.xml", p.as_bytes()));
    }
    let (wb, report) = read_xlsb(&make_zip(&zip)).unwrap();
    (wb.sheets[0].comments.iter().map(|(c, n)| (*c, n.clone())).collect(), report)
}

/// The same note and parts in an .xlsx package, read with the .xlsx reader.
fn xlsx(note: (&str, &str, &str), parts: &Parts) -> Vec<(CellRef, Comment)> {
    let (author, cell_ref, text) = note;
    let comments = format!(
        r#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>{author}</author></authors><commentList><comment ref="{cell_ref}" authorId="0"><text><t>{text}</t></text></comment></commentList></comments>"#
    );
    let mut sheet_rels = vec![("c1", format!("{REL_NS}/comments"), "../comments1.xml")];
    if parts.vml.is_some() {
        sheet_rels.push(("vml1", format!("{REL_NS}/vmlDrawing"), "../drawings/vmlDrawing1.vml"));
    }
    if parts.threads.is_some() {
        sheet_rels.push(("tc1", TC_REL.to_string(), "../threadedComments/threadedComment1.xml"));
    }
    let sheet_rels = rels(&sheet_rels.iter().map(|(a, b, c)| (*a, b.as_str(), *c)).collect::<Vec<_>>());
    let mut extra = vec![("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_str()), ("xl/comments1.xml", comments.as_str())];
    if let Some(v) = parts.vml {
        extra.push(("xl/drawings/vmlDrawing1.vml", v));
    }
    if let Some(t) = parts.threads {
        extra.push(("xl/threadedComments/threadedComment1.xml", t));
    }
    let mut wb_rels = String::new();
    if let Some(p) = parts.persons {
        extra.push(("xl/persons/person.xml", p));
        wb_rels = format!(r#"<Relationship Id="p1" Type="{PERSON_REL}" Target="persons/person.xml"/>"#);
    }
    let body = if parts.vml.is_some() { r#"<sheetData/><legacyDrawing r:id="vml1"/>"# } else { "<sheetData/>" };
    let (wb, _) = read_xlsx(&super::minimal(body, &extra, &wb_rels, "")).unwrap();
    wb.sheets[0].comments.iter().map(|(c, n)| (*c, n.clone())).collect()
}

/// Warnings about the notes' XML parts (the general XLSB import warning is not one).
fn part_warnings(report: &ReadReport) -> Vec<&String> {
    report.warnings.iter().filter(|w| w.contains("notes") || w.contains("comments") || w.contains("drawing")).collect()
}

#[test]
fn xlsb_shown_note_keeps_its_box_from_the_vml_drawing() {
    let note = ("Ann Lee", "B2", "Check the total");
    let parts = Parts { vml: Some(VML), ..Parts::default() };
    let (got, report) = xlsb(note, &parts, Some("vml1"));
    let want = xlsx(note, &parts);
    assert_eq!(got, want);
    let (cell, c) = &got[0];
    assert_eq!(*cell, at("B2"));
    assert!(c.visible);
    assert!(c.note_box.is_some());
    assert!(part_warnings(&report).is_empty(), "{:?}", report.warnings);
}

#[test]
fn xlsb_note_without_a_drawing_stays_hidden_without_a_warning() {
    let note = ("Ann Lee", "B2", "Plain");
    let (got, report) = xlsb(note, &Parts::default(), None);
    assert_eq!(got, xlsx(note, &Parts::default()));
    assert!(!got[0].1.visible);
    assert_eq!(got[0].1.note_box, None);
    assert!(part_warnings(&report).is_empty(), "{:?}", report.warnings);
}

#[test]
fn xlsb_missing_drawing_part_keeps_the_plain_note_with_one_warning() {
    let note = ("Ann Lee", "B2", "Plain");
    let (got, report) = xlsb(note, &Parts::default(), Some("vml1"));
    assert_eq!(got, xlsx(note, &Parts::default()));
    assert_eq!(part_warnings(&report).len(), 1, "{:?}", report.warnings);
    // A drawing id with no relationship at all is the same.
    let (got, report) = xlsb(note, &Parts::default(), Some("nope"));
    assert_eq!(got, xlsx(note, &Parts::default()));
    assert_eq!(part_warnings(&report).len(), 1, "{:?}", report.warnings);
    // A drawing that isn't XML too.
    let (got, report) = xlsb(note, &Parts { vml: Some("<xml><v:shape"), ..Parts::default() }, Some("vml1"));
    assert_eq!(got, xlsx(note, &Parts::default()));
    assert_eq!(part_warnings(&report).len(), 1, "{:?}", report.warnings);
}

#[test]
fn xlsb_threaded_comment_imports_as_a_thread() {
    let note = ("tc={T1}", "C3", "[Threaded comment] Is this right? Reply: Yes, checked.");
    let parts = Parts { threads: Some(THREADS), persons: Some(PERSONS), ..Parts::default() };
    let (got, report) = xlsb(note, &parts, None);
    assert_eq!(got, xlsx(note, &parts));
    let (cell, c) = &got[0];
    assert_eq!(*cell, at("C3"));
    assert!(c.threaded && c.resolved);
    assert_eq!(c.author, "Ann Lee");
    assert_eq!(c.text, "Is this right?");
    assert_eq!(c.replies, vec![("Bo Kim".to_string(), "Yes, checked.".to_string())]);
    assert_eq!(c.dates.len(), 2);
    assert!(part_warnings(&report).is_empty(), "{:?}", report.warnings);
}

#[test]
fn xlsb_damaged_threaded_part_keeps_the_placeholder_note_with_one_warning() {
    let note = ("tc={T1}", "C3", "[Threaded comment] Is this right?");
    let parts = Parts { threads: Some("<ThreadedComments"), persons: Some(PERSONS), ..Parts::default() };
    let (got, report) = xlsb(note, &parts, None);
    assert!(!got[0].1.threaded);
    assert_eq!(got[0].1.text, note.2);
    assert_eq!(part_warnings(&report).len(), 1, "{:?}", report.warnings);
}
