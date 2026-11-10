//! A formula read from a file without dynamic-array metadata (`cm`) is a legacy formula: Excel
//! calculates it with implicit intersection and never spills it. Expected values follow Excel's
//! own answers for the same formula shapes.

use std::io::{Read, Write};

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellError, CellRef, Value};
use gridcraft_engine::io;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

const METADATA: &str = r#"<?xml version="1.0"?><metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#;

/// Legacy formulas (no `cm`, no `t`), by cell.
const LEGACY: &[(&str, &str)] = &[
    ("C2", "A1:A5"),
    ("C9", "A1:A5"),
    ("D3", "A1:A5*2"),
    ("D9", "A1:A5+1"),
    ("E4", "A:A&\"/\"&B:B"),
    ("E12", "A:A&\"/\"&B:B"),
    ("H9", "C7+G7:J7"),
    ("K9", "C7+G7:J7"),
    ("F5", "X"),
    ("F3", "X+Y"),
    ("G3", "INDEX(A1:B5,0,2)"),
    ("G11", "INDEX(A1:B5,0,2)"),
    ("H2", "{2,3}*{4;5}"),
    ("H3", "MMULT(B1:B2,G7:H7)"),
    ("H4", "SIN({-1,0,1,2})"),
    ("H12", "INDIRECT(\"7:7\")"),
    ("J12", "Data!A:B Data!3:4"),
    ("J14", "COUNTIF(A1:A5,A1:A5>25)"),
    ("C14", "SUM(A1:A5)"),
    ("C15", "SUMPRODUCT(A1:A5*B1:B5)"),
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// Sheet "Data": A1:A5 = 10..50, B1:B5 = 1..5, C7 = 100, G7:J7 = 1..4, J15 = 1; the legacy
/// formulas above; a legacy CSE array formula in L1:L5 and a dynamic-array formula (`cm`) in M1.
/// Names X = A1:A5, Y = B1:B5.
fn package() -> Vec<u8> {
    let mut cells: std::collections::BTreeMap<u32, Vec<(u32, String)>> = Default::default();
    let mut put = |addr: &str, xml: String| {
        let c = at(addr);
        cells.entry(c.row + 1).or_default().push((c.col, xml));
    };
    for r in 1..=5 {
        put(&format!("A{r}"), format!(r#"<c r="A{r}"><v>{}</v></c>"#, r * 10));
        put(&format!("B{r}"), format!(r#"<c r="B{r}"><v>{r}</v></c>"#));
    }
    put("C7", r#"<c r="C7"><v>100</v></c>"#.into());
    for (i, col) in ["G", "H", "I", "J"].iter().enumerate() {
        put(&format!("{col}7"), format!(r#"<c r="{col}7"><v>{}</v></c>"#, i + 1));
    }
    put("J15", r#"<c r="J15"><v>1</v></c>"#.into());
    for (addr, f) in LEGACY {
        put(addr, format!(r#"<c r="{addr}"><f>{}</f></c>"#, esc(f)));
    }
    put("L1", r#"<c r="L1"><f t="array" ref="L1:L5">A1:A5*2</f></c>"#.into());
    put("M1", r#"<c r="M1" cm="1"><f t="array" ref="M1:M5">A1:A5*2</f></c>"#.into());
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
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="X">Data!$A$1:$A$5</definedName><definedName name="Y">Data!$B$1:$B$5</definedName></definedNames></workbook>"#
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
    s.execute("file.open", json!({"name": "legacy.xlsx", "base64": io::base64_encode(bytes)})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    s
}

fn value(s: &Session, cell: &str) -> Value {
    s.doc().unwrap().wb.sheet(0).unwrap().value(at(cell))
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

const VALUE: Value = Value::Error(CellError::Value);

fn check(s: &Session, expected: &[(&str, Value)]) {
    let wrong: Vec<String> = expected
        .iter()
        .filter(|(cell, v)| value(s, cell) != *v)
        .map(|(cell, v)| format!("{cell}: expected {v:?}, got {:?}", value(s, cell)))
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The saved sheet XML of `s`.
fn saved_sheet(s: &Session) -> String {
    let bytes = io::save_bytes(&s.doc().unwrap().wb, "legacy.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

/// The `<c …>` start tag and `<f>` text of `cell` in sheet XML.
fn saved_cell(xml: &str, cell: &str) -> (String, String) {
    let start = xml.find(&format!(r#"<c r="{cell}""#)).unwrap_or_else(|| panic!("{cell} not saved"));
    let rest = &xml[start..];
    let tag = rest[..rest.find('>').unwrap() + 1].to_string();
    let f = rest.find("<f").map(|i| {
        let body = &rest[i..];
        let open_end = body.find('>').unwrap() + 1;
        body[open_end..body.find("</f>").unwrap()].to_string()
    });
    (tag, f.unwrap_or_default())
}

#[test]
fn a_range_as_the_result_intersects_the_formula_row_or_column() {
    let s = open(&package());
    check(&s, &[("C2", num(20.0)), ("C9", VALUE), ("F5", num(50.0)), ("H12", num(2.0)), ("J12", VALUE)]);
}

#[test]
fn operator_operands_intersect() {
    let s = open(&package());
    check(
        &s,
        &[
            ("D3", num(60.0)),
            ("D9", VALUE),
            ("E4", Value::text("40/4")),
            ("E12", Value::text("/")),
            ("H9", num(102.0)),
            ("K9", VALUE),
            ("F3", num(33.0)),
        ],
    );
}

#[test]
fn a_reference_returned_by_index_intersects() {
    let s = open(&package());
    check(&s, &[("G3", num(3.0)), ("G11", VALUE)]);
}

#[test]
fn array_results_show_their_first_value_without_spilling() {
    let s = open(&package());
    check(&s, &[("H2", num(8.0)), ("H3", num(1.0)), ("H4", num((-1.0f64).sin()))]);
    let sheet = s.doc().unwrap().wb.sheet(0).unwrap();
    for cell in ["C3", "D4", "E5", "G4", "H5", "I2", "I3", "I4"] {
        assert_eq!(sheet.value(at(cell)), Value::Empty, "{cell} got a spilled value");
    }
}

#[test]
fn a_value_parameter_intersects_but_array_parameters_keep_the_range() {
    let s = open(&package());
    // COUNTIF's criteria is a value: A1:A5>25 in row 14 is #VALUE!, which no cell of A1:A5 equals.
    check(&s, &[("J14", num(0.0)), ("C14", num(150.0)), ("C15", num(550.0))]);
}

#[test]
fn array_and_dynamic_array_formulas_are_unchanged() {
    let s = open(&package());
    let expected: Vec<(String, Value)> =
        (1..=5).flat_map(|r| [(format!("L{r}"), num(f64::from(r) * 20.0)), (format!("M{r}"), num(f64::from(r) * 20.0))]).collect();
    let expected: Vec<(&str, Value)> = expected.iter().map(|(c, v)| (c.as_str(), v.clone())).collect();
    check(&s, &expected);
}

#[test]
fn formulas_typed_after_opening_are_dynamic_and_stay_so_after_saving() {
    let mut s = open(&package());
    s.execute("cell.set", json!({"cell": "N1", "input": "=A1:A5*2"})).unwrap();
    s.execute("cell.set", json!({"cell": "P1", "input": "=INDEX(A1:B5,0,2)"})).unwrap();
    // Retyping a legacy formula makes it a dynamic-array formula, as in Excel.
    s.execute("cell.set", json!({"cell": "D3", "input": "=A1:A5*2"})).unwrap();
    s.execute("formulas.calculateNow", json!({})).unwrap();
    let typed = [("N1", num(20.0)), ("N2", num(40.0)), ("P1", num(1.0)), ("P5", num(5.0)), ("D3", num(20.0)), ("D4", num(40.0))];
    check(&s, &typed);
    let bytes = io::save_bytes(&s.doc().unwrap().wb, "legacy.xlsx").unwrap();
    let back = open(&bytes);
    check(&back, &typed);
}

#[test]
fn legacy_formulas_are_saved_as_read_and_reopen_the_same() {
    let s = open(&package());
    let xml = saved_sheet(&s);
    for (cell, f) in LEGACY {
        let (tag, text) = saved_cell(&xml, cell);
        assert!(!tag.contains(" cm="), "{cell} was saved as a dynamic-array formula: {tag}");
        assert!(!text.contains("SINGLE"), "{cell} gained an explicit @: {text}");
        assert_eq!(text, esc(f), "{cell} formula text changed");
    }
    let back = open(&io::save_bytes(&s.doc().unwrap().wb, "legacy.xlsx").unwrap());
    check(&back, &[("C2", num(20.0)), ("D3", num(60.0)), ("E4", Value::text("40/4")), ("H2", num(8.0)), ("J14", num(0.0))]);
}
