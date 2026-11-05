//! Synthetic acceptance tests, planned by Codex.

use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;

use super::xls_chart_tests::{bof, cat, cells, eof, obj, rec, u16s};
use super::xls_tests::compound;
fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}
fn text_object(text: &str, unicode: bool) -> Vec<u8> {
    let sp = art(2, 202, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 0, 2, 0, 3, 0, 6, 0]));
    let data = art(0, 0, 0xF011, &[]);
    let drawing = rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &anchor, &data])));
    let textbox = rec(0x00EC, &art(0, 0, 0xF00D, &[]));
    let units: Vec<_> = text.encode_utf16().collect();
    let mut txo = vec![0u8; 18];
    txo[0..2].copy_from_slice(&0x0012u16.to_le_bytes());
    txo[10..12].copy_from_slice(&(units.len() as u16).to_le_bytes());
    txo[12..14].copy_from_slice(&16u16.to_le_bytes());
    let mut chars = vec![u8::from(unicode)];
    if unicode {
        for c in &units {
            chars.extend(c.to_le_bytes());
        }
    } else {
        chars.extend(text.as_bytes());
    }
    let mut runs = vec![0u8; 16];
    runs[8..10].copy_from_slice(&(units.len() as u16).to_le_bytes());
    let cut = 1 + if unicode { 2 * (units.len() / 2) } else { text.len() / 2 };
    let mut rest = vec![u8::from(unicode)];
    rest.extend_from_slice(&chars[cut..]);
    cat(&[&drawing, &obj(6), &textbox, &rec(0x01B6, &txo), &rec(0x003C, &chars[..cut]), &rec(0x003C, &rest), &rec(0x003C, &runs)])
}
fn xls(text: &str, unicode: bool) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), &text_object(text, unicode), &eof()]);
    let globals = |pos: u32| cat(&[&bof(0x0005), &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])), &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}
fn drawing_text(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    fn collect(e: &El, out: &mut String) {
        if e.name == "t" {
            out.push_str(&e.text);
        }
        for c in &e.children {
            collect(c, out);
        }
    }
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        if f.name().starts_with("xl/drawings/") && f.name().ends_with(".xml") && !f.name().contains("/_rels/") {
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            let root = parse(&b).unwrap();
            assert_eq!(root.name, "wsDr");
            let anchors: Vec<_> = root.kids("twoCellAnchor").collect();
            assert_eq!(anchors.len(), 1);
            let anchor = anchors[0];
            assert_eq!(anchor.path(&["from", "col"]).unwrap().text, "1");
            assert_eq!(anchor.path(&["from", "row"]).unwrap().text, "2");
            let body = anchor.path(&["sp", "txBody"]).unwrap();
            collect(body, &mut text);
        }
    }
    text
}
#[test]
fn compressed_text_and_native_drawing_survive() {
    let (wb, _) = crate::read_xls(&xls("Budget & notes", false)).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.shapes.len(), 1);
    assert_eq!(sheet.shapes[0].anchor.cell, gridcraft_core::CellRef::new(2, 1));
    assert_eq!(sheet.shapes[0].anchor.dx, 0.0);
    assert_eq!(sheet.shapes[0].anchor.dy, 0.0);
    let out = write_xlsx(&wb).unwrap();
    assert_eq!(drawing_text(&out), "Budget & notes");
    let (back, _) = read_xlsx(&out).unwrap();
    assert_eq!(back.sheet(0).unwrap().shapes.len(), 1);
}
#[test]
fn unicode_text_is_not_confused_with_formatting_runs() {
    let (wb, _) = crate::read_xls(&xls("予算 <Q4>", true)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_eq!(drawing_text(&out), "予算 <Q4>");
    let (back, _) = read_xlsx(&out).unwrap();
    let out = write_xlsx(&back).unwrap();
    assert_eq!(drawing_text(&out), "予算 <Q4>");
}
