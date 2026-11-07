//! Text results of a What-If data table are saved the way Excel saves them: every text cell of the
//! table (the top-left cell holding `<f t="dataTable">` and the other result cells) is a formula
//! result string, `t="str"` with the text inline in `<v>`, not a shared string (`t="s"`, an index
//! into the shared string table). ECMA-376 Part 1, 18.18.11 ST_CellType: `str` = "Cell containing a
//! formula string".

use super::minimal;
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

/// A one-variable row table C4:G4 and a two-variable table C5:D6, all text results, as Excel
/// writes them; A1 is ordinary text and E5 an ordinary number.
const BODY: &str = r#"<sheetData>
    <row r="1"><c r="A1" t="inlineStr"><is><t>plain</t></is></c><c r="B1"><v>1</v></c><c r="B2"><v>2</v></c></row>
    <row r="4"><c r="C4" t="str"><f t="dataTable" ref="C4:G4" dt2D="0" dtr="1" r1="B1"/><v>AA</v></c><c r="D4" t="str"><v>BB</v></c><c r="E4" t="str"><v>CC</v></c><c r="F4" t="str"><v>DD</v></c><c r="G4" t="str"><v>EE</v></c></row>
    <row r="5"><c r="C5" t="str"><f t="dataTable" ref="C5:D6" dt2D="1" dtr="1" r1="B2" r2="B1"/><v>A2</v></c><c r="D5" t="str"><v>A3</v></c><c r="E5"><v>7</v></c></row>
    <row r="6"><c r="C6" t="str"><v>B2</v></c><c r="D6"><v>4</v></c></row>
</sheetData>"#;

fn saved() -> (String, String) {
    let (wb, _) = read_xlsx(&minimal(BODY, &[], "", "")).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    let sheet = String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    let sst = pkg.read("xl/sharedStrings.xml").unwrap().map(|b| String::from_utf8(b).unwrap()).unwrap_or_default();
    (sheet, sst)
}

/// The `<c r="…">…</c>` element of one cell.
fn cell<'a>(xml: &'a str, a1: &str) -> &'a str {
    let open = format!("<c r=\"{a1}\"");
    let start = xml.find(&open).unwrap_or_else(|| panic!("{a1} not written: {xml}"));
    let end = xml[start..].find("</c>").map(|e| start + e + 4).or_else(|| xml[start..].find("/>").map(|e| start + e + 2)).unwrap();
    &xml[start..end]
}

#[test]
fn every_text_result_of_a_data_table_is_an_inline_formula_string() {
    let (xml, _) = saved();
    for (a1, text) in [("C4", "AA"), ("D4", "BB"), ("E4", "CC"), ("F4", "DD"), ("G4", "EE"), ("C5", "A2"), ("D5", "A3"), ("C6", "B2")] {
        let c = cell(&xml, a1);
        assert!(c.contains(" t=\"str\""), "{a1}: {c}");
        assert!(c.contains(&format!("<v>{text}</v>")), "{a1}: {c}");
    }
    // The formula stays on the top-left cells only.
    assert!(cell(&xml, "C4").contains("t=\"dataTable\""));
    assert!(cell(&xml, "C5").contains("t=\"dataTable\""));
    assert!(!cell(&xml, "D4").contains("<f"));
}

#[test]
fn data_table_text_is_not_added_to_the_shared_strings() {
    let (_, sst) = saved();
    for text in ["AA", "BB", "CC", "DD", "EE", "A2", "A3", "B2"] {
        assert!(!sst.contains(&format!("<t>{text}</t>")), "{text} in sharedStrings: {sst}");
    }
}

#[test]
fn numbers_in_a_table_and_text_outside_it_are_unchanged() {
    let (xml, sst) = saved();
    let d6 = cell(&xml, "D6");
    assert!(!d6.contains(" t=") && d6.contains("<v>4</v>"), "{d6}");
    let e5 = cell(&xml, "E5");
    assert!(!e5.contains(" t=") && e5.contains("<v>7</v>"), "{e5}");
    // Ordinary text outside a data table keeps its shared string.
    assert!(cell(&xml, "A1").contains(" t=\"s\""), "{}", cell(&xml, "A1"));
    assert!(sst.contains("<t>plain</t>"), "{sst}");
}

#[test]
fn the_text_results_read_back_unchanged() {
    let (wb, _) = read_xlsx(&minimal(BODY, &[], "", "")).unwrap();
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let (a, b) = (wb.sheet(0).unwrap(), again.sheet(0).unwrap());
    for a1 in ["C4", "D4", "G4", "C5", "D5", "C6", "D6"] {
        let c = gridcraft_core::CellRef::parse(a1).unwrap();
        assert_eq!(a.value(c), b.value(c), "{a1}");
    }
    assert_eq!(a.data_tables, b.data_tables);
}
