//! Formulas Excel saves without the dynamic-array `cm` flag are read as legacy formulas, with Excel's
//! implicit intersection `@` on parameters that take a single value. GROUPBY's filter_array takes an
//! array: Excel saves `TEXTJOIN(",",FALSE,GROUPBY(...,E1:E10>3))` without `cm` and calculates it over
//! the whole array. Expected values are Microsoft 365 Excel's own answers for these formulas and inputs.

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

/// Formulas stored without `cm`, by cell, spelled as Excel writes them.
const LEGACY: &[(&str, &str)] = &[
    ("B60", r#"_xlfn.TEXTJOIN(",",FALSE,_xlfn.GROUPBY(H1:H10,E1:E10,_xleta.SUM,3))"#),
    ("B61", r#"_xlfn.TEXTJOIN(",",FALSE,_xlfn.GROUPBY(H1:H10,E1:E10,_xleta.SUM,0,1,-2))"#),
    ("B62", r#"_xlfn.TEXTJOIN(",",FALSE,_xlfn.GROUPBY(H1:H10,E1:E10,_xleta.SUM,0,0,,E1:E10>3))"#),
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// Sheet "Data" with the inputs of the Excel measurement and the formulas above.
fn package() -> Vec<u8> {
    let mut cells: std::collections::BTreeMap<u32, Vec<(u32, String)>> = Default::default();
    let mut put = |addr: &str, xml: String| {
        let c = at(addr);
        cells.entry(c.row + 1).or_default().push((c.col, xml));
    };
    // E1:E10 = 3,5,5,8,1,9,5,2,8,4; H1:H10 = a,b,a,c,b,a,c,a,b,A.
    for (r, (n, key)) in [(3, "a"), (5, "b"), (5, "a"), (8, "c"), (1, "b"), (9, "a"), (5, "c"), (2, "a"), (8, "b"), (4, "A")].into_iter().enumerate()
    {
        put(&format!("E{}", r + 1), format!(r#"<c r="E{}"><v>{n}</v></c>"#, r + 1));
        put(&format!("H{}", r + 1), format!(r#"<c r="H{}" t="inlineStr"><is><t>{key}</t></is></c>"#, r + 1));
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
    s.execute("file.open", json!({"name": "legacy_groupby.xlsx", "base64": io::base64_encode(bytes)})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    s
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(cell))
}

fn text(t: &str) -> Value {
    Value::Text(t.into())
}

fn check(s: &Session, expected: &[(&str, Value)]) {
    let close = |a: &Value, b: &Value| match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x - y).abs() <= 1e-12 * y.abs().max(1.0),
        _ => a == b,
    };
    let wrong: Vec<String> = expected
        .iter()
        .filter(|(cell, v)| !close(&value(s, cell), v))
        .map(|(cell, v)| format!("{cell}: expected {v:?}, got {:?}", value(s, cell)))
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn legacy_groupby_filter_array_is_read_as_an_array() {
    let s = open(&package());
    check(&s, &[("B62", text("a,18,b,13,c,13"))]);
}

#[test]
fn legacy_groupby_without_a_filter_keeps_its_result() {
    let s = open(&package());
    check(&s, &[("B60", text("a,3,a,20,b,14,c,13,Total,47")), ("B61", text("a,23,b,14,c,13,Total,50"))]);
}
