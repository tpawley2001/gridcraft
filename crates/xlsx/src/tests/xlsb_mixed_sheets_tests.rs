//! An `.xlsb` with chart sheets, Excel 4.0 macro sheets and Custom Views together: kept sheets stay
//! at their file position, every written `sheetId` is unique and the view's active sheet follows
//! its sheet. The package is built in code from the builders of the single-feature tests.

use std::collections::HashMap;

use gridcraft_core::{CellRef, Value};

use super::make_zip;
use super::xls_custom_views_tests::GUID;
use super::xlsb_chart_sheet_tests::{CHART, DRAWING, Part, chart_sheet_part};
use super::xlsb_custom_views_tests::{sheet_view, view_on};
use super::xlsb_macro_sheet_tests::{macro_cells, names, unpack};
use super::xlsb_tests::{ROOT, cell, record, row, sst, wide, worksheet};
use crate::{XlsxKind, read_xlsb, read_xlsx, write_xlsx_as};

const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MS: &str = "http://schemas.microsoft.com/office/2006/relationships";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Chart,
    /// A worksheet with A1 = the number.
    Work(f64),
    Macro,
}

/// A package with `sheets` (name, kind, file sheet id) in BrtBundleSh order and one Custom View
/// whose active sheet is the one with file sheet id `active`.
fn package(sheets: &[(&str, Kind, u32)], active: u32) -> Vec<u8> {
    let mut book = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    book.extend(record(153, &props));
    book.extend(view_on(&GUID, active));
    book.extend(record(143, &[]));
    let mut rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="strings" Type="{R}/sharedStrings" Target="../text/strings.bin"/>"#
    );
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>"#,
    );
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    for (i, &(name, kind, id)) in sheets.iter().enumerate() {
        let mut data = 0u32.to_le_bytes().to_vec();
        data.extend(id.to_le_bytes());
        data.extend(wide(&format!("s{i}")));
        data.extend(wide(name));
        book.extend(record(156, &data));
        let (rel, ct, bytes) = match kind {
            Kind::Chart => (format!("{R}/chartsheet"), "chartsheet", chart_sheet_part(Part::Chart)),
            Kind::Macro => (format!("{MS}/xlMacrosheet"), "macrosheet", worksheet(&macro_cells(), &[])),
            Kind::Work(n) => {
                let mut d = row(0);
                d.extend(cell(5, 0, &n.to_le_bytes()));
                (format!("{R}/worksheet"), "worksheet", worksheet(&d, &sheet_view(&GUID)))
            }
        };
        rels.push_str(&format!(r#"<Relationship Id="s{i}" Type="{rel}" Target="../data/sheet{i}.bin"/>"#));
        types.push_str(&format!(r#"<Override PartName="/data/sheet{i}.bin" ContentType="application/vnd.ms-excel.{ct}"/>"#));
        if kind == Kind::Chart {
            let sheet_rels = format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId3" Type="{R}/drawing" Target="../xl/drawings/drawing1.xml"/></Relationships>"#
            );
            files.push((format!("data/_rels/sheet{i}.bin.rels"), sheet_rels.into_bytes()));
        }
        files.push((format!("data/sheet{i}.bin"), bytes));
    }
    book.extend(record(144, &[]));
    book.extend(record(132, &[]));
    rels.push_str("</Relationships>");
    types.push_str("</Types>");
    let drawing_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/chart" Target="../charts/chart1.xml"/></Relationships>"#
    );
    let strings = sst();
    // The writer keeps a chart sheet only when its series refer to sheets that exist.
    let chart = CHART.replace("Data!", "Sheet1!");
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", rels.as_bytes()),
        ("text/strings.bin", &strings),
        ("xl/drawings/drawing1.xml", DRAWING.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/charts/chart1.xml", chart.as_bytes()),
    ];
    parts.extend(files.iter().map(|(n, b)| (n.as_str(), b.as_slice())));
    make_zip(&parts)
}

/// The value of `attr` in every `<tag ` start tag of `xml`.
fn attrs(xml: &str, tag: &str, attr: &str) -> Vec<String> {
    xml.match_indices(&format!("<{tag} "))
        .map(|(i, _)| {
            let start = &xml[i..i + xml[i..].find('>').unwrap()];
            let key = format!(" {attr}=\"");
            let j = start.find(&key).unwrap_or_else(|| panic!("{attr} missing in {start}"));
            start[j + key.len()..].split('"').next().unwrap().to_string()
        })
        .collect()
}

/// Saves `bytes` (an .xlsb) as `kind`; returns the saved package and its parts.
fn save(bytes: &[u8], kind: XlsxKind) -> (Vec<u8>, HashMap<String, String>) {
    let (wb, _) = read_xlsb(bytes).unwrap();
    let out = write_xlsx_as(&wb, kind).unwrap();
    let parts = unpack(&out);
    (out, parts)
}

/// Checks the saved `<sheets>` order and ids; returns the active sheet id of the one Custom View
/// and the written `sheetId` of every sheet by name.
fn check(parts: &HashMap<String, String>, order: &[&str]) -> (String, HashMap<String, String>) {
    let book = &parts["xl/workbook.xml"];
    assert_eq!(names(parts), order, "{book}");
    let ids = attrs(book, "sheet", "sheetId");
    let mut seen = ids.clone();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), ids.len(), "duplicate sheetId: {book}");
    assert!(ids.iter().all(|id| id.parse::<u32>().unwrap() != 0), "{book}");
    let active = attrs(book, "customWorkbookView", "activeSheetId");
    assert_eq!(active.len(), 1, "{book}");
    let by_name = order.iter().map(|n| n.to_string()).zip(ids).collect();
    (active[0].clone(), by_name)
}

#[test]
fn xlsb_chart_macro_sheets_and_custom_view_keep_order_and_ids() {
    let file = package(&[("Chart1", Kind::Chart, 7), ("Sheet1", Kind::Work(1.0), 3), ("Macro1", Kind::Macro, 9), ("Sheet2", Kind::Work(2.0), 4)], 4);
    let (xlsm, parts) = save(&file, XlsxKind::MacroWorkbook);
    let (active, ids) = check(&parts, &["Chart1", "Sheet1", "Macro1", "Sheet2"]);
    assert_eq!(active, ids["Sheet2"]);
    for part in ["xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml"] {
        assert!(parts[part].contains("<customSheetView "), "{part}");
    }

    let (_, parts) = save(&file, XlsxKind::Workbook);
    let (active, ids) = check(&parts, &["Chart1", "Sheet1", "Sheet2"]);
    assert_eq!(active, ids["Sheet2"]);

    let (wb, _) = read_xlsx(&xlsm).unwrap();
    let a1 = |name: &str| wb.sheets.iter().find(|s| s.name == name).unwrap().cells.get(CellRef::new(0, 0)).map(|c| c.value.clone());
    assert_eq!(a1("Sheet1"), Some(Value::Number(1.0)));
    assert_eq!(a1("Sheet2"), Some(Value::Number(2.0)));
}

#[test]
fn xlsb_custom_view_on_a_last_chart_sheet_shows_the_first_worksheet() {
    let file = package(&[("Sheet1", Kind::Work(1.0), 3), ("Macro1", Kind::Macro, 9), ("Sheet2", Kind::Work(2.0), 4), ("Chart1", Kind::Chart, 7)], 7);
    for (kind, order) in
        [(XlsxKind::MacroWorkbook, &["Sheet1", "Macro1", "Sheet2", "Chart1"][..]), (XlsxKind::Workbook, &["Sheet1", "Sheet2", "Chart1"][..])]
    {
        let (_, parts) = save(&file, kind);
        let (active, ids) = check(&parts, order);
        // A chart sheet is not in the model's sheet list, so a view's active sheet maps only to
        // worksheets: one whose active sheet is a chart sheet falls back on purpose to the first
        // worksheet (the reader's `finish_book` mapping turns an unmapped sheet id into sheet 0).
        assert_eq!(active, ids["Sheet1"]);
    }
}
