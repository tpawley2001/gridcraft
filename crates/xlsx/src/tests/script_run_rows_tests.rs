//! A cell whose shared or inline string has a superscript or subscript run (`<r><rPr><vertAlign>`,
//! ECMA-376 Part 1, 18.4.7 / 18.4.14) is recorded at import, so printing can fit its non-custom row
//! like a superscript cell font (Excel stores such an Arial 10 row at 14.25 pt, 19 px). The print
//! height itself is checked in the engine (`cmd::print::fit_sup_border_tests`), which this crate
//! cannot reach; this checks the record the fit reads.
use gridcraft_core::CellRef;

use super::minimal;
use crate::read_xlsx;

const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="10"/><name val="Arial"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="1"><xf/></cellXfs></styleSheet>"#;

const SST: &str = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>plain</t></si><si><r><t>m</t></r><r><rPr><vertAlign val="superscript"/><sz val="10"/><rFont val="Arial"/></rPr><t>2</t></r></si><si><r><rPr><vertAlign val="baseline"/></rPr><t>b</t></r></si></sst>"#;

const RELS: &str = r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

#[test]
fn superscript_and_subscript_runs_are_recorded_per_cell() {
    let body = r#"<sheetFormatPr defaultRowHeight="12.75"/><sheetData>
      <row r="1" ht="12.75"><c r="A1" t="s"><v>0</v></c></row>
      <row r="2" ht="12.75"><c r="A2" t="s"><v>1</v></c></row>
      <row r="3" ht="12.75"><c r="A3" t="s"><v>2</v></c></row>
      <row r="4" ht="12.75"><c r="A4" t="inlineStr"><is><r><t>H</t></r><r><rPr><vertAlign val="subscript"/></rPr><t>2</t></r></is></c></row>
      <row r="5" ht="12.75"><c r="A5" t="s"><v>99</v></c></row>
    </sheetData>"#;
    let bytes = minimal(body, &[("xl/styles.xml", STYLES), ("xl/sharedStrings.xml", SST)], RELS, "");
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let s = wb.sheet(0).unwrap();
    assert!(s.fit_auto_row_heights_in_print);
    assert!(!s.rows.get(&1).is_some_and(|i| i.custom), "row 2 is not custom-height");
    let got: Vec<CellRef> = s.script_run_cells.iter().copied().collect();
    assert_eq!(got, [at("A2"), at("A4")]);
    assert_eq!(s.value(at("A2")), gridcraft_core::Value::text("m2"));
}

#[test]
fn script_run_fonts_are_recorded_per_cell() {
    use gridcraft_model::ScriptRunFont;
    let body = r#"<sheetFormatPr defaultRowHeight="12.75"/><sheetData>
      <row r="1"><c r="A1" t="s"><v>1</v></c></row>
      <row r="2"><c r="A2" t="inlineStr"><is><r><t>H</t></r><r><rPr><vertAlign val="superscript"/><sz val="14"/><rFont val="Arial"/></rPr><t>1</t></r><r><rPr><vertAlign val="subscript"/><sz val="-3"/></rPr><t>2</t></r></is></c></row>
    </sheetData>"#;
    let bytes = minimal(body, &[("xl/styles.xml", STYLES), ("xl/sharedStrings.xml", SST)], RELS, "");
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let s = wb.sheet(0).unwrap();
    assert!(s.rows.get(&0).is_none_or(|i| i.size.is_none()), "a row without ht stores no height");
    let sup = |name: Option<&str>, size: Option<f32>, subscript: bool| ScriptRunFont { name: name.map(str::to_string), size, subscript };
    assert_eq!(s.script_run_fonts.get(&at("A1")), Some(&vec![sup(Some("Arial"), Some(10.0), false)]));
    assert_eq!(s.script_run_fonts.get(&at("A2")), Some(&vec![sup(Some("Arial"), Some(14.0), false), sup(None, None, true)]));
}
