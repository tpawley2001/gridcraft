//! T() over a multi-cell reference and SUMPRODUCT over a one-cell text range, in legacy formulas
//! (no dynamic-array `cm`). Expected values are Microsoft Excel's own recalculated answers for the
//! same formulas and inputs.

use std::io::Write;

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_engine::io;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

const METADATA: &str = r#"<?xml version="1.0"?><metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#;

/// Legacy formulas (no `cm`, no `t`), by cell.
const LEGACY: &[(&str, &str)] = &[
    ("M20", "T(H12:J12)"),
    ("N20", "T(L7:N7)"),
    ("O22", "T(J21:J23)"),
    ("J28", "SUMPRODUCT(G30:G30,I30:I30)"),
    ("P20", "T(M7)"),
    ("K28", "SUMPRODUCT(I30:I30,I30:I30)"),
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// Sheet "Data" with the inputs of the Excel measurements and the legacy formulas above.
fn package() -> Vec<u8> {
    let mut cells: std::collections::BTreeMap<u32, Vec<(u32, String)>> = Default::default();
    let mut put = |addr: &str, xml: String| {
        let c = at(addr);
        cells.entry(c.row + 1).or_default().push((c.col, xml));
    };
    for (addr, n) in [("H12", "545"), ("I12", "454"), ("J12", "0"), ("L7", "1.001"), ("I30", "2")] {
        put(addr, format!(r#"<c r="{addr}"><v>{n}</v></c>"#));
    }
    for (addr, t) in [("M7", "alphanum"), ("N7", "blanksIncl"), ("J22", "non-numeric"), ("J23", "non-numeric"), ("G30", "4")] {
        put(addr, format!(r#"<c r="{addr}" t="inlineStr"><is><t>{t}</t></is></c>"#));
    }
    for (addr, f) in LEGACY {
        put(addr, format!(r#"<c r="{addr}"><f>{}</f></c>"#, esc(f)));
    }
    let mut rows = String::new();
    for (r, mut cs) in cells {
        cs.sort_by_key(|(c, _)| *c);
        rows.push_str(&format!(r#"<row r="{r}">{}</row>"#, cs.into_iter().map(|(_, x)| x).collect::<String>()));
    }
    let sheet = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData>{rows}</sheetData></worksheet>"#);
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/><Override PartName="/xl/metadata.xml" ContentType="{CT}.sheetMetadata+xml"/></Types>"#
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            ),
        ),
        (
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{NS_R}/sheetMetadata" Target="metadata.xml"/></Relationships>"#
            ),
        ),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/metadata.xml", METADATA.into()),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "legacy_t.xlsx", "base64": io::base64_encode(bytes)})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    s
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(cell))
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

fn text(t: &str) -> Value {
    Value::Text(t.into())
}

fn check(s: &Session, expected: &[(&str, Value)]) {
    let wrong: Vec<String> = expected
        .iter()
        .filter(|(cell, v)| value(s, cell) != *v)
        .map(|(cell, v)| format!("{cell}: expected {v:?}, got {:?}", value(s, cell)))
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn t_over_a_multi_cell_reference_reads_the_top_left_cell() {
    let s = open(&package());
    // Excel: "" for all three (top-left cell is a number or empty), not implicit intersection.
    check(&s, &[("M20", text("")), ("N20", text("")), ("O22", text(""))]);
}

#[test]
fn sumproduct_counts_text_in_a_one_cell_range_as_zero() {
    let s = open(&package());
    check(&s, &[("J28", num(0.0))]);
}

#[test]
fn single_cell_t_and_numeric_sumproduct_are_unchanged() {
    let s = open(&package());
    check(&s, &[("P20", text("alphanum")), ("K28", num(4.0))]);
}
