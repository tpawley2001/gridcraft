//! Imported XLSB arrays recalculate through the production engine.
use gridcraft_calc::Calc;
use gridcraft_core::{CellRef, Value};
use std::io::Write;
fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = if id < 128 { vec![id as u8] } else { vec![(id as u8 & 0x7f) | 0x80, (id >> 7) as u8] };
    let mut len = bytes.len();
    loop {
        let b = (len & 127) as u8;
        len >>= 7;
        out.push(b | if len > 0 { 128 } else { 0 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    units.iter().for_each(|u| out.extend(u.to_le_bytes()));
    out
}

fn parsed(tokens: &[u8], extra: &[u8]) -> Vec<u8> {
    [(tokens.len() as u32).to_le_bytes().to_vec(), tokens.to_vec(), (extra.len() as u32).to_le_bytes().to_vec(), extra.to_vec()].concat()
}
fn array(values: &[Vec<u8>], rows: u32, cols: u32) -> Vec<u8> {
    [rows.to_le_bytes().to_vec(), cols.to_le_bytes().to_vec(), values.concat()].concat()
}
fn num(n: f64) -> Vec<u8> {
    [vec![0], n.to_le_bytes().to_vec()].concat()
}
fn string(s: u8) -> Vec<u8> {
    vec![1, 1, 0, s, 0]
}
fn arr() -> Vec<u8> {
    [vec![0x60], vec![0; 14]].concat()
}
fn fmla(col: u32, tokens: &[u8], extra: &[u8]) -> Vec<u8> {
    record(9, &[col.to_le_bytes().to_vec(), vec![0; 4], 99.0f64.to_le_bytes().to_vec(), vec![0; 2], parsed(tokens, extra)].concat())
}
fn row(r: u32) -> Vec<u8> {
    record(0, &[r.to_le_bytes().to_vec(), vec![0; 13]].concat())
}
fn package(sheet: Vec<u8>) -> Vec<u8> {
    let mut book = record(131, &[]);
    book.extend(record(143, &[]));
    for (i, name) in ["Data", "Calc"].iter().enumerate() {
        book.extend(record(156, &[vec![0; 4], (i as u32 + 1).to_le_bytes().to_vec(), wide(&format!("s{i}")), wide(name)].concat()));
    }
    book.extend(record(144, &[]));
    let name = [
        vec![0; 5],
        u32::MAX.to_le_bytes().to_vec(),
        wide("Consts"),
        parsed(&arr(), &array(&[num(1.0), num(2.0), num(3.0)], 1, 3)),
        u32::MAX.to_le_bytes().to_vec(),
    ]
    .concat();
    book.extend(record(39, &name));
    book.extend(record(132, &[]));
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
        ("xl/s0.bin", sheet),
        ("xl/s1.bin", [record(129, &[]), record(145, &[]), record(146, &[]), record(130, &[])].concat()),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

#[test]
fn xlsb_arrays_recalculate_in_cells_groups_names_and_intersections() {
    let sum = [arr(), vec![0x22, 1, 4, 0]].concat();
    let mut data = row(0);
    let cases = [
        (sum.clone(), array(&[num(1.0), num(2.0), num(3.0)], 1, 3)),
        ([arr(), vec![0x1E, 2, 0, 0x22, 2, 29, 0]].concat(), array(&[num(10.0), num(20.0), num(30.0)], 1, 3)),
        (
            [vec![0x17, 1, 0, b'b', 0], arr(), vec![0x1E, 2, 0, 0x1D, 0, 0x22, 4, 102, 0]].concat(),
            array(&[string(b'a'), num(1.0), string(b'b'), num(2.0)], 2, 2),
        ),
        ([arr(), vec![0x22, 1, 36, 0]].concat(), array(&[vec![2, 1], vec![2, 0]], 1, 2)),
        ([arr(), vec![0x1E, 1, 0, 0x22, 2, 29, 0, 0x21, 2, 0]].concat(), array(&[vec![4, 0x2A, 0, 0, 0]], 1, 1)),
        (vec![0x43, 1, 0, 0, 0, 0x22, 1, 4, 0], vec![]),
    ];
    for (col, (tokens, extra)) in cases.iter().enumerate() {
        data.extend(fmla(col as u32, tokens, extra));
    }
    // G1: SUM(A1:B2 B2:C3), whose sole intersecting cell B2 is 7.
    let area = |r0: u32, r1: u32, c0: u16, c1: u16| {
        [vec![0x25], r0.to_le_bytes().to_vec(), r1.to_le_bytes().to_vec(), (c0 | 0xC000).to_le_bytes().to_vec(), (c1 | 0xC000).to_le_bytes().to_vec()]
            .concat()
    };
    let tokens = [vec![0x26, 0, 0, 0, 0, 27, 0], area(0, 1, 0, 1), area(1, 2, 1, 2), vec![0x0F, 0x22, 1, 4, 0]].concat();
    data.extend(fmla(6, &tokens, &[1u32.to_le_bytes().to_vec(), vec![0; 16]].concat()));
    data.extend(row(1));
    data.extend(record(5, &[1u32.to_le_bytes().to_vec(), vec![0; 4], 7.0f64.to_le_bytes().to_vec()].concat()));
    // H3:H5 shared SUM(I3,{1,2}), with I3:I5 = 1,2,3.
    for r in 2..5u32 {
        data.extend(row(r));
        data.extend(fmla(7, &[1, 2, 0, 0, 0], &7u32.to_le_bytes()));
        if r == 2 {
            let range: Vec<u8> = [2u32, 4, 7, 7].iter().flat_map(|v| v.to_le_bytes()).collect();
            let tokens = [vec![0x2C, 0, 0, 0, 0, 1, 0xC0], arr(), vec![0x22, 2, 4, 0]].concat();
            data.extend(record(427, &[range, parsed(&tokens, &array(&[num(1.0), num(2.0)], 1, 2))].concat()));
        }
        data.extend(record(5, &[8u32.to_le_bytes().to_vec(), vec![0; 4], f64::from(r - 1).to_le_bytes().to_vec()].concat()));
    }
    // A6:A7 legacy array formula SUM({1,2,3}).
    for r in 5..7 {
        data.extend(row(r));
        data.extend(fmla(0, &[1, 5, 0, 0, 0], &0u32.to_le_bytes()));
        if r == 5 {
            let range: Vec<u8> = [5u32, 6, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
            data.extend(record(426, &[range, vec![0], parsed(&sum, &cases[0].1)].concat()));
        }
    }
    let sheet = [record(129, &[]), record(145, &[]), data, record(146, &[]), record(130, &[])].concat();
    let (mut wb, _) = gridcraft_xlsx::read_xlsb(&package(sheet)).unwrap();
    Calc::new().recalc_all(&mut wb);
    for (col, expected) in
        [Value::Number(6.0), Value::Number(20.0), Value::Number(2.0), Value::Bool(false), Value::Bool(true), Value::Number(6.0), Value::Number(7.0)]
            .into_iter()
            .enumerate()
    {
        assert_eq!(wb.sheets[0].value(CellRef::new(0, col as u32)), expected, "column {col}");
    }
    for r in 2..5 {
        assert_eq!(wb.sheets[0].value(CellRef::new(r, 7)), Value::Number(f64::from(r + 2)));
    }
    for r in 5..7 {
        assert_eq!(wb.sheets[0].value(CellRef::new(r, 0)), Value::Number(6.0));
    }
}
