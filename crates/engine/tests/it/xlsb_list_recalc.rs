//! Structured references read from an .xlsb file (`PtgList` tokens, a table's custom totals
//! formula) recalculate, and still do after saving as .xlsx and reopening. Records assembled per
//! [MS-XLSB]; every saved result is 0, so the values below come from recalculation.

use std::io::Write;

use gridcraft_engine::calc::Calc;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_model::Workbook;

const ID: u32 = 3;

fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = if id < 128 { vec![id as u8] } else { vec![(id as u8 & 0x7f) | 0x80, (id >> 7) as u8] };
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
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
fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
/// `PtgList` to table `ID`: rowType `rows`, columns kind `cols`, colFirst / colLast.
fn list(rows: u16, cols: u16, c0: u16, c1: u16) -> Vec<u8> {
    [&[0x18u8, 0x19, 0, 0][..], &(cols | (rows << 2)).to_le_bytes(), &ID.to_le_bytes(), &c0.to_le_bytes(), &c1.to_le_bytes()].concat()
}
fn formula_record(id: u16, col: u32, value: &[u8], rgce: &[u8]) -> Vec<u8> {
    let mut data = u32s(&[col, 0]);
    data.extend(value);
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(id, &data)
}
fn fmla(col: u32, rgce: &[u8]) -> Vec<u8> {
    formula_record(9, col, &0f64.to_le_bytes(), rgce)
}
fn text(col: u32, s: &str) -> Vec<u8> {
    record(6, &[u32s(&[col, 0]), wide(s)].concat())
}
fn number(col: u32, n: f64) -> Vec<u8> {
    record(5, &[u32s(&[col, 0]), n.to_le_bytes().to_vec()].concat())
}
fn row(r: u32) -> Vec<u8> {
    record(0, &[&r.to_le_bytes()[..], &[0; 13]].concat())
}
fn this_row_product() -> Vec<u8> {
    [list(0x10, 1, 1, 1), list(0x10, 1, 2, 2), vec![0x05]].concat()
}
/// `SUBTOTAL(109,[Qty])*2`.
fn custom_total() -> Vec<u8> {
    [&[0x1E, 109, 0][..], &list(0, 1, 1, 1), &[0x42, 2, 0x58, 0x01, 0x1E, 2, 0, 0x05]].concat()
}

/// Table `Sales` (Item, Qty, Price, Total = Qty*Price) over A1:D4, plus a totals row with Qty's
/// custom total in B5 when `totals`; E1:E4 hold formulas using structured references.
fn xlsb(totals: bool) -> Vec<u8> {
    let null = [0xffu8; 4];
    let mut begin = u32s(&[0, 3 + u32::from(totals), 0, 3, 0, ID, 1, u32::from(totals), u32::from(totals)]);
    begin.extend(null.repeat(6));
    begin.extend(u32s(&[0]));
    begin.extend([wide("Sales"), wide("Sales")].concat());
    begin.extend(null.repeat(4));
    let mut table = [record(343, &begin), record(345, &u32s(&[4]))].concat();
    for (i, name) in ["Item", "Qty", "Price", "Total"].iter().enumerate() {
        let mut col = u32s(&[i as u32 + 1, if i == 1 { 9 } else { 0 }]);
        col.extend(null.repeat(3));
        col.extend(u32s(&[0]));
        col.extend(null);
        col.extend(wide(name));
        col.extend(null.repeat(4));
        table.extend(record(347, &col));
        if i == 1 {
            let mut f = vec![0, 0];
            f.extend((custom_total().len() as u32).to_le_bytes());
            f.extend(custom_total());
            f.extend(0u32.to_le_bytes());
            table.extend(record(352, &f));
        }
        table.extend(record(348, &[]));
    }
    table.extend([record(346, &[]), record(344, &[])].concat());

    let e = [
        [list(0, 1, 1, 1), vec![0x42, 1, 4, 0]].concat(),
        [list(0, 1, 1, 1), list(0, 1, 2, 2), vec![0x42, 2, 228, 0]].concat(),
        [list(0x01, 0, 0, 0), vec![0x41, 76, 0]].concat(),
        [list(0x02, 2, 0, 2), vec![0x42, 1, 169, 0]].concat(),
    ];
    let mut data = [row(0), text(0, "Item"), text(1, "Qty"), text(2, "Price"), text(3, "Total"), fmla(4, &e[0])].concat();
    for (r, (item, qty, price)) in [("a", 2.0, 3.0), ("b", 4.0, 5.0), ("c", 6.0, 7.0)].into_iter().enumerate() {
        data.extend([row(r as u32 + 1), text(0, item), number(1, qty), number(2, price), fmla(3, &this_row_product())].concat());
        data.extend(e.get(r + 1).map(|rgce| fmla(4, rgce)).unwrap_or_default());
    }
    if totals {
        data.extend([row(4), fmla(1, &custom_total())].concat());
    }
    let parts_rec = [record(660, &u32s(&[1])), record(661, &wide("t1")), record(662, &[])].concat();
    let sheet = [record(129, &[]), record(145, &[]), data, record(146, &[]), parts_rec, record(130, &[])].concat();
    let mut sheet_entry = u32s(&[0, 1]);
    sheet_entry.extend([wide("s1"), wide("Data")].concat());
    let book = [record(131, &[]), record(143, &[]), record(156, &sheet_entry), record(144, &[]), record(132, &[])].concat();
    let rel = |id: &str, kind: &str, target: &str| {
        format!(r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#)
    };
    let rels =
        |body: String| format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#);
    let parts: [(&str, Vec<u8>); 7] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/s1.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/t1.bin" ContentType="application/vnd.ms-excel.table"/></Types>"#.to_vec(),
        ),
        ("_rels/.rels", rels(rel("b", "officeDocument", "xl/workbook.bin")).into_bytes()),
        ("xl/workbook.bin", book),
        ("xl/_rels/workbook.bin.rels", rels(rel("s1", "worksheet", "s1.bin")).into_bytes()),
        ("xl/s1.bin", sheet),
        ("xl/_rels/s1.bin.rels", rels(rel("t1", "table", "t1.bin")).into_bytes()),
        ("xl/t1.bin", table),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn values(wb: &mut Workbook, cells: &[&str]) -> Vec<Value> {
    Calc::new().recalc_all(wb);
    cells.iter().map(|c| wb.sheets[0].value(CellRef::parse(c).unwrap())).collect()
}

#[test]
fn xlsb_structured_references_recalculate_before_and_after_xlsx_save() {
    let n = Value::Number;
    // Qty 2+4+6 = 12; Qty·Price 2·3+4·5+6·7 = 68; rows A1:D4 = 4; headers Item:Price = 3;
    // this-row Qty·Price in D2:D4 = 6, 20, 42.
    let cells = ["E1", "E2", "E3", "E4", "D2", "D3", "D4"];
    let expected = vec![n(12.0), n(68.0), n(4.0), n(3.0), n(6.0), n(20.0), n(42.0)];
    let (mut wb, _) = gridcraft_xlsx::read_xlsb(&xlsb(false)).unwrap();
    assert_eq!(values(&mut wb, &cells), expected);
    let (mut back, _) = gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(values(&mut back, &cells), expected);

    // With the totals row: A1:D5 is 5 rows, and Qty's custom total is SUBTOTAL(109,…)*2 = 24.
    let cells = ["E3", "B5"];
    let (mut wb, _) = gridcraft_xlsx::read_xlsb(&xlsb(true)).unwrap();
    assert_eq!(wb.sheets[0].tables[0].columns[1].totals_label.as_deref(), Some("SUBTOTAL(109,[Qty])*2"));
    assert_eq!(values(&mut wb, &cells), [n(5.0), n(24.0)]);
    let (mut back, _) = gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].tables[0].columns[1].totals_label.as_deref(), Some("SUBTOTAL(109,[Qty])*2"));
    assert_eq!(values(&mut back, &cells), [n(5.0), n(24.0)]);
}
