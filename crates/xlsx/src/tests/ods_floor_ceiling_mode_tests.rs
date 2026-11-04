//! OpenFormula FLOOR/CEILING with a third Mode argument (ODF 1.2 part 2, 6.17) open as
//! FLOOR.MATH/CEILING.MATH, which is how Excel computes them (FLOOR(37;-1;0) = 37,
//! CEILING(-37.2;1;0) = -37, FLOOR(2.5;1;0) = 2). Two-argument and COM.MICROSOFT. forms are unchanged.
//! Original ODS packages assembled in Rust.

use gridcraft_core::CellRef;

use super::make_zip;
use crate::read_ods;

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;

/// One row of formula cells (each cached as 0, so a kept formula is visible as a formula).
fn ods_row(formulas: &[&str]) -> Vec<u8> {
    let cells: String =
        formulas.iter().map(|f| format!(r#"<table:table-cell table:formula="{f}" office:value-type="float" office:value="0"/>"#)).collect();
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2"><office:body><office:spreadsheet><table:table table:name="Data"><table:table-row>{cells}</table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    make_zip(&[("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_bytes())])
}

fn opened(formulas: &[&str]) -> Vec<Option<String>> {
    let (wb, _) = read_ods(&ods_row(formulas)).unwrap();
    (0..formulas.len() as u32).map(|c| wb.sheets[0].cell(CellRef::new(0, c)).and_then(|x| x.formula.as_ref().map(|f| f.text.clone()))).collect()
}

#[test]
fn three_argument_floor_and_ceiling_become_the_math_forms() {
    let got = opened(&["of:=FLOOR(37; -1; 0)", "of:=CEILING(-37.2;1;0)", "of:=FLOOR(2.5;1;0)", "of:=ceiling(2.5;1;1)"]);
    assert_eq!(got[0].as_deref(), Some("FLOOR.MATH(37,-1,0)"));
    assert_eq!(got[1].as_deref(), Some("CEILING.MATH(-37.2,1,0)"));
    assert_eq!(got[2].as_deref(), Some("FLOOR.MATH(2.5,1,0)"));
    assert_eq!(got[3].as_deref(), Some("CEILING.MATH(2.5,1,1)"));
}

#[test]
fn two_argument_and_prefixed_forms_are_unchanged() {
    let got = opened(&["of:=CEILING(2.5;1)", "of:=COM.MICROSOFT.FLOOR(2.5;1)", "of:=COM.MICROSOFT.CEILING(2.5;1)", "of:=FLOOR([.A2];1)"]);
    assert_eq!(got[0].as_deref(), Some("CEILING(2.5,1)"));
    assert_eq!(got[1].as_deref(), Some("FLOOR(2.5,1)"));
    assert_eq!(got[2].as_deref(), Some("CEILING(2.5,1)"));
    assert_eq!(got[3].as_deref(), Some("FLOOR(A2,1)"));
}

#[test]
fn arguments_are_counted_at_the_call_depth() {
    // Separators inside nested calls, arrays and strings don't make a two-argument call a three-argument one.
    let got = opened(&[
        "of:=FLOOR(SUM(1;2;3);1)",
        "of:=CEILING(MAX({1;2;3});1)",
        "of:=FLOOR(LEN(&quot;a;b&quot;);1)",
        "of:=SUM(FLOOR(7;2;0);CEILING(7;2))",
    ]);
    assert_eq!(got[0].as_deref(), Some("FLOOR(SUM(1,2,3),1)"));
    assert_eq!(got[1].as_deref(), Some("CEILING(MAX({1,2,3}),1)"));
    assert_eq!(got[2].as_deref(), Some("FLOOR(LEN(\"a;b\"),1)"));
    assert_eq!(got[3].as_deref(), Some("SUM(FLOOR.MATH(7,2,0),CEILING(7,2))"));
}
