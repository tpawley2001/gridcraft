//! A defined name and an other-sheet reference read from an .xlsb file stay live: editing the
//! source cell recalculates the formulas that use them. Records assembled per [MS-XLSB].

use std::io::Write;

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use serde_json::json;

fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = if id < 128 { vec![id as u8] } else { vec![(id as u8 & 0x7f) | 0x80, (id >> 7) as u8] };
    assert!(bytes.len() < 128);
    out.push(bytes.len() as u8);
    out.extend(bytes);
    out
}
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    units.iter().for_each(|u| out.extend(u.to_le_bytes()));
    out
}
/// A worksheet whose first row holds `cell` (a BrtCellReal or BrtFmlaNum record).
fn sheet(cell: Vec<u8>) -> Vec<u8> {
    let row = [&0u32.to_le_bytes()[..], &[0; 13]].concat();
    [record(129, &[]), record(145, &[]), record(0, &row), cell, record(146, &[]), record(130, &[])].concat()
}
fn cell(id: u16, value: f64, rgce: Option<&[u8]>) -> Vec<u8> {
    let mut data = [&0u32.to_le_bytes()[..], &[0; 4], &value.to_le_bytes()].concat();
    if let Some(rgce) = rgce {
        data.extend([0; 2]);
        data.extend((rgce.len() as u32).to_le_bytes());
        data.extend(rgce);
        data.extend(0u32.to_le_bytes());
    }
    record(id, &data)
}

fn xlsb() -> Vec<u8> {
    // PtgRef3d to Data!$A$1 through XTI 0 (BrtSupSelf, sheet 0).
    let data_a1 = [&[0x3Au8][..], &0u16.to_le_bytes(), &0u32.to_le_bytes(), &0u16.to_le_bytes()].concat();
    let mut book = record(131, &[]);
    book.extend(record(143, &[]));
    for (i, name) in ["Data", "Calc"].iter().enumerate() {
        book.extend(record(156, &[&0u32.to_le_bytes()[..], &(i as u32 + 1).to_le_bytes(), &wide(&format!("s{i}")), &wide(name)].concat()));
    }
    book.extend(record(144, &[]));
    book.extend(record(353, &[]));
    book.extend(record(357, &[]));
    book.extend(record(362, &[&1u32.to_le_bytes()[..], &[0; 12]].concat()));
    book.extend(record(354, &[]));
    let mut name = [&0u32.to_le_bytes()[..], &[0], &u32::MAX.to_le_bytes(), &wide("Rate"), &(data_a1.len() as u32).to_le_bytes()].concat();
    name.extend(&data_a1);
    name.extend(0u32.to_le_bytes());
    name.extend(u32::MAX.to_le_bytes());
    book.extend(record(39, &name));
    book.extend(record(132, &[]));
    // Calc!A1 = Rate*10 + Data!A1 (PtgName 1, PtgInt 10, PtgMul, PtgRef3d, PtgAdd).
    let rgce = [&[0x43u8, 1, 0, 0, 0, 0x1E, 10, 0, 0x05][..], &data_a1, &[0x03]].concat();
    let parts: [(&str, Vec<u8>); 6] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/s0.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/s1.bin" ContentType="application/vnd.ms-excel.worksheet"/></Types>"#.to_vec(),
        ),
        (
            "_rels/.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="b" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#.to_vec(),
        ),
        ("xl/workbook.bin", book),
        (
            "xl/_rels/workbook.bin.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s0" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="s0.bin"/><Relationship Id="s1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="s1.bin"/></Relationships>"#.to_vec(),
        ),
        ("xl/s0.bin", sheet(cell(5, 2.0, None))),
        ("xl/s1.bin", sheet(cell(9, 22.0, Some(&rgce)))),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn calc_a1(s: &Session) -> Value {
    s.doc().unwrap().wb.sheets[1].value(CellRef::new(0, 0))
}

#[test]
fn xlsb_names_and_other_sheet_refs_recalculate() {
    let mut s = Session::new();
    let b64 = gridcraft_engine::io::base64_encode(&xlsb());
    s.execute("file.open", json!({"name": "book.xlsb", "base64": b64})).unwrap();
    let doc = s.doc().unwrap();
    assert_eq!(doc.wb.sheets[1].cell(CellRef::new(0, 0)).unwrap().formula.as_ref().unwrap().text, "Rate*10+Data!$A$1");
    assert_eq!(doc.wb.names.iter().map(|n| (n.name.as_str(), n.formula.as_str())).collect::<Vec<_>>(), [("Rate", "Data!$A$1")]);
    assert_eq!(calc_a1(&s), Value::Number(22.0));
    // Data!A1 = 3 → Rate*10 + Data!A1 = 33.
    s.execute("sheet.activate", json!({"sheet": "Data"})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "3"})).unwrap();
    assert_eq!(calc_a1(&s), Value::Number(33.0));
}
