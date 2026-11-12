//! A note's formatted text runs (`<comment><text><r><rPr>`, ECMA-376 Part 1 §18.7.7 and §18.4.7)
//! survive open + save: the bold author line, font, size and colour Excel gives every note.
//! Synthetic packages; the run spellings follow what Microsoft 365 Excel writes.

use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use gridcraft_core::CellRef;

/// A one-sheet package with one note on B2 whose `<text>` content is `text`.
fn note(text: &str) -> Vec<u8> {
    let v = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1025" type="#_x0000_t202"><v:textbox><div>note</div></v:textbox><x:ClientData ObjectType="Note"><x:Anchor>1,0,1,0,3,0,4,0</x:Anchor><x:Row>1</x:Row><x:Column>1</x:Column></x:ClientData></v:shape></xml>"##;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="comments" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/><Relationship Id="vml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#;
    let c = format!(
        r#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>Example</author></authors><commentList><comment ref="B2" authorId="0" shapeId="0"><text>{text}</text></comment></commentList></comments>"#
    );
    minimal(
        r#"<sheetData/><legacyDrawing r:id="vml"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/comments1.xml", &c), ("xl/drawings/vmlDrawing1.vml", v)],
        "",
        "",
    )
}

/// The author line and the body as Excel formats them: bold Tahoma 9 for the name, then regular.
const EXCEL_RUNS: &str = r#"<r><rPr><b/><sz val="9"/><color indexed="81"/><rFont val="Tahoma"/><family val="2"/></rPr><t>Example:</t></r><r><rPr><sz val="9"/><color indexed="81"/><rFont val="Tahoma"/><family val="2"/></rPr><t>Some text</t></r>"#;

/// The saved comments part's `<text>` element of the note on B2.
fn saved_text(bytes: &[u8]) -> El {
    let mut pkg = Package::open(bytes).unwrap();
    // Resolve the part through its relationship rather than assuming the writer's numbering.
    let rels = pkg.rels("xl/worksheets/sheet1.xml").unwrap();
    let rel = rels.iter().find(|r| r.kind == "comments").expect("the sheet keeps its comments part");
    let x = parse(&pkg.read(&rel.target).unwrap().unwrap()).unwrap();
    let list = x.child("commentList").expect("commentList");
    let comment = list.kids("comment").find(|c| c.attr("ref") == Some("B2")).expect("the note on B2");
    comment.child("text").expect("text").clone()
}

/// The plain text of a `<text>` element: its own `<t>` or its runs' `<t>`s, concatenated.
fn plain(text: &El) -> String {
    let mut out = text.child("t").map(|t| t.text.clone()).unwrap_or_default();
    for r in text.kids("r") {
        out.push_str(&r.child("t").map(|t| t.text.clone()).unwrap_or_default());
    }
    out
}

fn val<'a>(rpr: &'a El, name: &str) -> Option<&'a str> {
    rpr.child(name).and_then(|e| e.attr("val"))
}

fn is_bold(rpr: &El) -> bool {
    rpr.child("b").is_some_and(|b| !matches!(b.attr("val"), Some("0" | "false")))
}

fn assert_excel_runs(text: &El) {
    let runs: Vec<&El> = text.kids("r").collect();
    assert_eq!(runs.len(), 2, "both runs are kept: {text:?}");
    assert_eq!(runs[0].child("t").map(|t| t.text.as_str()), Some("Example:"));
    assert_eq!(runs[1].child("t").map(|t| t.text.as_str()), Some("Some text"));
    let first = runs[0].child("rPr").expect("the author run keeps its formatting");
    let second = runs[1].child("rPr").expect("the body run keeps its formatting");
    assert!(is_bold(first), "the author line stays bold: {first:?}");
    assert!(!is_bold(second), "the body stays regular: {second:?}");
    for rpr in [first, second] {
        assert_eq!(val(rpr, "sz").and_then(|v| v.parse::<f64>().ok()), Some(9.0), "{rpr:?}");
        assert_eq!(val(rpr, "rFont"), Some("Tahoma"), "{rpr:?}");
        assert_eq!(val(rpr, "family"), Some("2"), "{rpr:?}");
        assert_eq!(rpr.child("color").and_then(|c| c.attr("indexed")), Some("81"), "{rpr:?}");
    }
}

#[test]
fn note_run_formatting_survives_save() {
    let (wb, _) = read_xlsx(&note(EXCEL_RUNS)).unwrap();
    // The model's plain text is unchanged by the formatting.
    let comment = wb.sheets[0].comments.get(&CellRef::parse("B2").unwrap()).expect("the note is read");
    assert_eq!(comment.text, "Example:Some text");
    let saved = write_xlsx(&wb).unwrap();
    assert_excel_runs(&saved_text(&saved));
    // And a second open + save keeps it too.
    let (again, _) = read_xlsx(&saved).unwrap();
    assert_excel_runs(&saved_text(&write_xlsx(&again).unwrap()));
}

#[test]
fn note_run_formatting_keeps_italic_underline_and_rgb_colour() {
    let runs = r#"<r><rPr><i/><u val="double"/><sz val="12"/><color rgb="FFFF0000"/><rFont val="Arial"/></rPr><t>Check </t></r><r><rPr><strike/><sz val="12"/><color theme="4"/><rFont val="Arial"/></rPr><t>this</t></r>"#;
    let (wb, _) = read_xlsx(&note(runs)).unwrap();
    let text = saved_text(&write_xlsx(&wb).unwrap());
    assert_eq!(plain(&text), "Check this");
    let runs: Vec<&El> = text.kids("r").collect();
    assert_eq!(runs.len(), 2, "{text:?}");
    let first = runs[0].child("rPr").expect("first run formatting");
    assert!(first.child("i").is_some(), "{first:?}");
    assert_eq!(val(first, "u"), Some("double"), "{first:?}");
    assert_eq!(first.child("color").and_then(|c| c.attr("rgb")), Some("FFFF0000"), "{first:?}");
    assert_eq!(val(first, "rFont"), Some("Arial"), "{first:?}");
    assert_eq!(val(first, "sz").and_then(|v| v.parse::<f64>().ok()), Some(12.0), "{first:?}");
    let second = runs[1].child("rPr").expect("second run formatting");
    assert!(second.child("strike").is_some(), "{second:?}");
    assert!(second.child("i").is_none() && second.child("u").is_none(), "{second:?}");
    assert_eq!(second.child("color").and_then(|c| c.attr("theme")), Some("4"), "{second:?}");
}

#[test]
fn note_run_formatting_is_not_invented_for_a_plain_note() {
    let (wb, _) = read_xlsx(&note("<t>note</t>")).unwrap();
    let text = saved_text(&write_xlsx(&wb).unwrap());
    assert_eq!(plain(&text), "note");
    assert!(text.kids("r").all(|r| r.child("rPr").is_none()), "a plain note gets no run formatting: {text:?}");
}

#[test]
fn note_run_formatting_does_not_bring_back_text_that_was_edited() {
    let (mut wb, _) = read_xlsx(&note(EXCEL_RUNS)).unwrap();
    let at = CellRef::parse("B2").unwrap();
    wb.sheet_mut(0).unwrap().comments.get_mut(&at).expect("the note is read").text = "Rewritten".to_string();
    let text = saved_text(&write_xlsx(&wb).unwrap());
    assert_eq!(plain(&text), "Rewritten", "the saved note holds the new text only: {text:?}");
}
