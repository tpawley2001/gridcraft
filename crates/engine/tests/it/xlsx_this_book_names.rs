//! `[0]!Name` in a cell formula is the book prefix for "this workbook" (ECMA-376 Part 1, 18.17):
//! it names the workbook's own defined name and calculates like `Name`. Excel's answer for the
//! file shape below is 142 (Defines!B2). External prefixes (`[1]!Name`) are out of scope here.
//! Every cached value is 0, so the values asserted come from recalculation.

use std::io::Write;

use gridcraft_engine::calc::Calc;
use gridcraft_engine::core::{CellError, CellRef, Value};
use gridcraft_model::Workbook;

fn cell(r: &str, f: &str) -> String {
    format!(r#"<c r="{r}"><f>{}</f><v>0</v></c>"#, f.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"))
}

fn xlsx() -> Vec<u8> {
    let uses = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="3">{}</row><row r="5">{}</row><row r="7">{}</row><row r="9">{}</row><row r="11">{}</row></sheetData></worksheet>"#,
        cell("D3", "[0]!NR_Global_B2"),
        cell("D5", "IF(Defines!$B$2=142,[0]!NR_Global_B2,-1)"),
        cell("D7", "SUM([0]!NR_Global_B2,1)"),
        cell("D9", "NR_Global_B2*2"),
        cell("D11", "[0]!NoSuchName"),
    );
    let defines = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="2"><c r="B2"><v>142</v></c></row></sheetData></worksheet>"#;
    let workbook = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Uses" sheetId="1" r:id="rId1"/><sheet name="Defines" sheetId="2" r:id="rId2"/></sheets><definedNames><definedName name="NR_Global_B2">Defines!$B$2</definedName></definedNames><calcPr calcId="191029"/></workbook>"#;
    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let parts: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", types.as_bytes().to_vec()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("xl/workbook.xml", workbook.as_bytes().to_vec()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml", uses.into_bytes()),
        ("xl/worksheets/sheet2.xml", defines.as_bytes().to_vec()),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

const CELLS: [&str; 5] = ["D3", "D5", "D7", "D9", "D11"];

fn values(wb: &mut Workbook) -> Vec<Value> {
    Calc::new().recalc_all(wb);
    CELLS.iter().map(|c| wb.sheets[0].value(CellRef::parse(c).unwrap())).collect()
}

fn expected() -> Vec<Value> {
    vec![Value::Number(142.0), Value::Number(142.0), Value::Number(143.0), Value::Number(284.0), Value::Error(CellError::Name)]
}

#[test]
fn this_book_prefixed_names_calculate_like_the_plain_name() {
    let (mut wb, _) = gridcraft_xlsx::read_xlsx(&xlsx()).unwrap();
    assert_eq!(values(&mut wb), expected());
}

#[test]
fn this_book_prefixed_names_still_calculate_after_save_and_reopen() {
    let (mut wb, _) = gridcraft_xlsx::read_xlsx(&xlsx()).unwrap();
    Calc::new().recalc_all(&mut wb);
    let (mut back, _) = gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(values(&mut back), expected());
}
