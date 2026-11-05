//! Synthetic acceptance tests, planned by Codex.

use super::make_zip;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rec(id: u16, b: &[u8]) -> Vec<u8> {
    let mut out = if id < 128 { vec![id as u8] } else { vec![(id as u8 & 127) | 128, (id >> 7) as u8] };
    let mut n = b.len();
    loop {
        let v = (n & 127) as u8;
        n >>= 7;
        out.push(if n == 0 { v } else { v | 128 });
        if n == 0 {
            break;
        }
    }
    out.extend(b);
    out
}
fn wide(s: &str) -> Vec<u8> {
    let u: Vec<_> = s.encode_utf16().collect();
    let mut b = (u.len() as u32).to_le_bytes().to_vec();
    for c in u {
        b.extend(c.to_le_bytes())
    }
    b
}
fn xlsb_parameter(refresh: bool, row: u32) -> Vec<u8> {
    let mut book = rec(131, &[]);
    book.extend(rec(143, &[]));
    let mut sh = 0u32.to_le_bytes().to_vec();
    sh.extend(1u32.to_le_bytes());
    sh.extend(wide("s0"));
    sh.extend(wide("Data"));
    book.extend(rec(156, &sh));
    book.extend(rec(144, &[]));
    book.extend(rec(353, &[]));
    book.extend(rec(357, &[]));
    let mut xti = 1u32.to_le_bytes().to_vec();
    xti.extend([0; 12]);
    book.extend(rec(362, &xti));
    book.extend(rec(354, &[]));
    book.extend(rec(132, &[]));
    let sheet = [rec(129, &[]), rec(145, &[]), rec(146, &[]), rec(130, &[])].concat();
    let mut conn = vec![6, 0, 0, 0];
    conn.extend(0u16.to_le_bytes());
    conn.extend(0u16.to_le_bytes());
    conn.extend(8u16.to_le_bytes());
    conn.extend(1u32.to_le_bytes());
    conn.extend(1u32.to_le_bytes());
    conn.extend(1u32.to_le_bytes());
    conn.push(0);
    conn.extend(wide("Sales"));
    let mut db = 2u32.to_le_bytes().to_vec();
    db.push(2);
    db.extend(wide("DSN=Sales;"));
    db.extend(wide("SELECT * FROM t WHERE id=?"));
    let mut p = (2u16 | if refresh { 8 } else { 0 }).to_le_bytes().to_vec();
    p.extend(4u16.to_le_bytes());
    p.extend(0u32.to_le_bytes());
    p.extend(wide("Customer"));
    let mut rgce = vec![0x3A];
    rgce.extend(0u16.to_le_bytes());
    rgce.extend(row.to_le_bytes());
    rgce.extend(0u16.to_le_bytes());
    p.extend((rgce.len() as u32).to_le_bytes());
    p.extend(rgce);
    p.extend(0u32.to_le_bytes());
    let conns = [
        rec(429, &[]),
        rec(201, &conn),
        rec(203, &db),
        rec(204, &[]),
        rec(265, &1u32.to_le_bytes()),
        rec(267, &p),
        rec(268, &[]),
        rec(266, &[]),
        rec(202, &[]),
        rec(430, &[]),
    ]
    .concat();
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet0.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/connections.bin" ContentType="application/vnd.ms-excel.connections"/></Types>"#;
    let root = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="b" Type="{REL}/officeDocument" Target="xl/workbook.bin"/></Relationships>"#
    );
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s0" Type="{REL}/worksheet" Target="worksheets/sheet0.bin"/><Relationship Id="c" Type="{REL}/connections" Target="connections.bin"/></Relationships>"#
    );
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/worksheets/sheet0.bin", &sheet),
        ("xl/connections.bin", &conns),
    ])
}
fn connection_xml(bytes: &[u8]) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        if f.name().ends_with(".xml") {
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            let x = parse(&b).unwrap();
            if x.name == "connections" {
                return x;
            }
        }
    }
    panic!("saved XLSX lost its connections part")
}
#[test]
fn cell_reference_and_refresh_on_change_survive() {
    let (wb, _) = crate::read_xlsb(&xlsb_parameter(true, 1)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let xml = connection_xml(&out);
    let c = xml.child("connection").unwrap();
    assert_eq!(c.attr_u32("id"), Some(1));
    let p = c.path(&["parameters", "parameter"]).unwrap();
    assert_eq!(p.attr("parameterType"), Some("cell"));
    assert_eq!(p.attr("cell"), Some("Data!$A$2"));
    assert_eq!(p.attr("name"), Some("Customer"));
    assert!(p.flag("refreshOnChange", false));
    assert_eq!(p.attr("sqlType"), Some("4"));
}
#[test]
fn cell_parameter_without_refresh_survives_second_save() {
    let (wb, _) = crate::read_xlsb(&xlsb_parameter(false, 4)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&out).unwrap();
    let out = write_xlsx(&back).unwrap();
    let xml = connection_xml(&out);
    let p = xml.path(&["connection", "parameters", "parameter"]).unwrap();
    assert_eq!(p.attr("cell"), Some("Data!$A$5"));
    assert!(!p.flag("refreshOnChange", false));
}
