//! Legacy formulas (no dynamic-array `cm`): an IF consumed as a single value takes the first element
//! of an array condition, and an array constant passed as VLOOKUP's lookup value takes its first
//! element, while functions such as ABS, SIN and ROUND still lift over arrays. Expected values are
//! Microsoft Excel's own recalculated answers for the same formulas and inputs.

use std::io::Write;

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellError, CellRef, Value};
use gridcraft_engine::io;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

const METADATA: &str = r#"<?xml version="1.0"?><metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#;

/// Legacy formulas (no `cm`, no `t`), by cell. Expected values: Excel 365 after a full recalculation.
const LEGACY: &[(&str, &str)] = &[
    // IF consumed as a single value (inside ABS): its array condition counts with its first element.
    ("A51", "AVERAGE(ABS(IF((FREQUENCY($E$34:$E$40,$E$34:$E$40)-1)>0,1)))"),
    ("A52", "AVERAGE(ABS(IF((FREQUENCY($E$34:$E$40,$E$34:$E$40)-1)>0,1)+FREQUENCY($E$34:$E$40,$E$34:$E$40)))"),
    // An array constant as VLOOKUP's lookup value: only its first element is looked up.
    ("A73", "MODE(VLOOKUP({\"a\",\"b\",\"b\"},C72:D75,2))"),
    // Guards: Excel lifts these, and we already agree.
    ("A74", "AVERAGE(IF(FREQUENCY($E$34:$E$40,$E$34:$E$40)>0,1,0))"),
    ("A75", "AVERAGE(IF((FREQUENCY($E$34:$E$40,$E$34:$E$40)-1)>0,1))"),
    ("A39", "COUNT(ABS(FREQUENCY($E$34:$E$40,$E$34:$E$40)))"),
    ("A40", "MODE(ABS(FREQUENCY($E$34:$E$40,$E$34:$E$40)))"),
    ("A41", "ABS(FREQUENCY($E$34:$E$40,$E$34:$E$40)+1)"),
    ("A37", "ABS(FREQUENCY($E$34:$E$40,$E$34:$E$40))"),
    ("A36", "COLUMNS(SIN({2,3,4}+1)+1)"),
    ("A53", "COLUMNS(IF(C54,E50:F50,{5,6,7,8}))"),
    ("A54", "COLUMNS(IF(C54,{3,4,5},{6,7}))"),
    ("A62", "STDEV(2,ROUND({1,2,3},1))"),
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
    // E34:E40 = 2,2,2,2,4,4,4 (FREQUENCY of it over itself = {4;0;0;0;3;0;0;0}); C54 = 1;
    // C72:D75 = a 5 / b 4 / c 2 / d 1.
    for (addr, n) in [("E34", "2"), ("E35", "2"), ("E36", "2"), ("E37", "2"), ("E38", "4"), ("E39", "4"), ("E40", "4"), ("C54", "1")] {
        put(addr, format!(r#"<c r="{addr}"><v>{n}</v></c>"#));
    }
    for (r, (key, n)) in [("a", "5"), ("b", "4"), ("c", "2"), ("d", "1")].into_iter().enumerate() {
        put(&format!("C{}", 72 + r), format!(r#"<c r="C{}" t="inlineStr"><is><t>{key}</t></is></c>"#, 72 + r));
        put(&format!("D{}", 72 + r), format!(r#"<c r="D{}"><v>{n}</v></c>"#, 72 + r));
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
    s.execute("file.open", json!({"name": "legacy_arrays.xlsx", "base64": io::base64_encode(bytes)})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    s
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(cell))
}

fn num(n: f64) -> Value {
    Value::Number(n)
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
fn if_in_a_single_value_argument_takes_the_first_element_of_its_condition() {
    let s = open(&package());
    // IF((FREQUENCY-1)>0,1) inside ABS is 1 (first condition element TRUE), not {1,F,F,F,1,F,F,F}.
    check(&s, &[("A51", num(1.0)), ("A52", num(1.875))]);
}

#[test]
fn array_constant_as_vlookup_lookup_value_takes_its_first_element() {
    let s = open(&package());
    // VLOOKUP looks up "a" only -> 5; MODE of one value is #N/A.
    check(&s, &[("A73", Value::Error(CellError::NA))]);
}

#[test]
fn arrays_that_excel_lifts_in_legacy_formulas_stay_lifted() {
    let s = open(&package());
    check(
        &s,
        &[
            ("A74", num(0.25)),
            ("A75", num(1.0)),
            ("A39", num(8.0)),
            ("A40", num(0.0)),
            ("A41", num(5.0)),
            ("A37", num(4.0)),
            ("A36", num(3.0)),
            ("A53", num(2.0)),
            ("A54", num(3.0)),
            ("A62", num((2.0f64 / 3.0).sqrt())),
        ],
    );
}
