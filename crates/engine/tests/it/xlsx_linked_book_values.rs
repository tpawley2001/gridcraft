//! Formulas into a linked workbook calculate from the cells saved with the link
//! (ECMA-376 Part 1, 18.14 `externalLink` / `externalBook` / `sheetDataSet`), the way Excel does
//! when the linked file is closed. The linked file is never opened. Every formula's own cached
//! value is 0, so the values asserted come from recalculation.
//!
//! Linked book `prices.xlsx`, sheet `Prices`: A2 "a", A3 "b", A4 "c", B2 142, B3 10, B4 "x";
//! its defined name `NR_Global_B2` = `Prices!$B$2`. This workbook's name `Gas` = `[1]Prices!$B$2:$B$4`.

use std::io::Write;

use gridcraft_engine::calc::Calc;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_model::Workbook;

fn cell(r: &str, f: &str) -> String {
    format!(r#"<c r="{r}"><f>{}</f><v>0</v></c>"#, f.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;"))
}

fn xlsx() -> Vec<u8> {
    let rows: String = [
        ("A1", "[1]Prices!B2"),
        ("A2", "SUM([1]Prices!B2:B4)"),
        ("A3", "SUM(Gas)"),
        ("A4", "[1]!NR_Global_B2"),
        ("A5", "VLOOKUP(\"b\",[1]Prices!A2:B4,2,FALSE)"),
        ("A6", "[1]Prices!B4"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (r, f))| format!(r#"<row r="{}">{}</row>"#, i + 1, cell(r, f)))
    .collect();
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let link = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><externalBook r:id="rId1"><sheetNames><sheetName val="Prices"/></sheetNames><definedNames><definedName name="NR_Global_B2" refersTo="=Prices!$B$2"/></definedNames><sheetDataSet><sheetData sheetId="0"><row r="2"><cell r="A2" t="str"><v>a</v></cell><cell r="B2"><v>142</v></cell></row><row r="3"><cell r="A3" t="str"><v>b</v></cell><cell r="B3"><v>10</v></cell></row><row r="4"><cell r="A4" t="str"><v>c</v></cell><cell r="B4" t="str"><v>x</v></cell></row></sheetData></sheetDataSet></externalBook></externalLink>"#;
    let link_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath" Target="prices.xlsx" TargetMode="External"/></Relationships>"#;
    let workbook = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Uses" sheetId="1" r:id="rId1"/></sheets><externalReferences><externalReference r:id="rId2"/></externalReferences><definedNames><definedName name="Gas">[1]Prices!$B$2:$B$4</definedName></definedNames><calcPr calcId="191029"/></workbook>"#;
    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink1.xml"/></Relationships>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/externalLinks/externalLink1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml"/></Types>"#;
    let parts: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", types.as_bytes().to_vec()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("xl/workbook.xml", workbook.as_bytes().to_vec()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml", sheet.into_bytes()),
        ("xl/externalLinks/externalLink1.xml", link.as_bytes().to_vec()),
        ("xl/externalLinks/_rels/externalLink1.xml.rels", link_rels.as_bytes().to_vec()),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

const CELLS: [&str; 6] = ["A1", "A2", "A3", "A4", "A5", "A6"];

fn values(wb: &mut Workbook) -> Vec<Value> {
    Calc::new().recalc_all(wb);
    CELLS.iter().map(|c| wb.sheets[0].value(CellRef::parse(c).unwrap())).collect()
}

fn expected() -> Vec<Value> {
    vec![Value::Number(142.0), Value::Number(152.0), Value::Number(152.0), Value::Number(142.0), Value::Number(10.0), Value::Text("x".into())]
}

#[test]
fn formulas_into_a_linked_book_calculate_from_its_saved_cells() {
    let (mut wb, _) = gridcraft_xlsx::read_xlsx(&xlsx()).unwrap();
    assert_eq!(values(&mut wb), expected());
}

#[test]
fn linked_book_formulas_still_calculate_after_save_and_reopen() {
    let (mut wb, _) = gridcraft_xlsx::read_xlsx(&xlsx()).unwrap();
    Calc::new().recalc_all(&mut wb);
    let (mut back, _) = gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(values(&mut back), expected());
}
