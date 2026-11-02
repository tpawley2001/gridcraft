//! XLSB tables (ListObjects) and AutoFilters: BIFF12 records and packages assembled in Rust from
//! the public [MS-XLSB] record layouts, compared with what the .xlsx reader gives for the same
//! table written as XML.
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{FilterCriterion, Table, TotalsFn};

use super::{make_zip, minimal};
use crate::{ReadReport, read_xlsb, read_xlsx};
use gridcraft_model::Workbook;

pub(super) const NULL: [u8; 4] = [0xff; 4];

pub(super) fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
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
pub(super) fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}
fn nullable(text: Option<&str>) -> Vec<u8> {
    text.map(wide).unwrap_or(NULL.to_vec())
}
pub(super) fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
pub(super) fn row(r: u32) -> Vec<u8> {
    let mut out = r.to_le_bytes().to_vec();
    out.extend([0; 13]);
    record(0, &out)
}
pub(super) fn text_cell(col: u32, s: &str) -> Vec<u8> {
    let mut data = u32s(&[col, 0]);
    data.extend(wide(s));
    record(6, &data)
}
pub(super) fn number_cell(col: u32, n: f64) -> Vec<u8> {
    let mut data = u32s(&[col, 0]);
    data.extend(n.to_le_bytes());
    record(5, &data)
}
pub(super) fn worksheet(data: &[u8], tail: &[u8]) -> Vec<u8> {
    [record(129, &[]), record(145, &[]), data.to_vec(), record(146, &[]), tail.to_vec(), record(130, &[])].concat()
}
pub(super) fn book() -> Vec<u8> {
    let mut sheet = u32s(&[0, 1]);
    sheet.extend(wide("rId1"));
    sheet.extend(wide("Data"));
    [record(131, &[]), record(143, &[]), record(156, &sheet), record(144, &[]), record(132, &[])].concat()
}
/// `BrtBeginListParts`, one `BrtListPart` per relationship id, `BrtEndListParts`.
pub(super) fn list_parts(ids: &[&str]) -> Vec<u8> {
    let mut out = record(660, &u32s(&[ids.len() as u32]));
    for id in ids {
        out.extend(record(661, &wide(id)));
    }
    out.extend(record(662, &[]));
    out
}

/// A one-sheet XLSB package: the worksheet, its relationships (when given) and the table parts
/// `xl/tables/table{n}.bin` (n from 1), each with a typed override.
pub(super) fn package(sheet: &[u8], sheet_rels: Option<&str>, tables: &[Vec<u8>]) -> Vec<u8> {
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/>"#,
    );
    let names: Vec<String> = (1..=tables.len()).map(|n| format!("xl/tables/table{n}.bin")).collect();
    for name in &names {
        types.push_str(&format!(r#"<Override PartName="/{name}" ContentType="application/vnd.ms-excel.table"/>"#));
    }
    types.push_str("</Types>");
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/></Relationships>"#;
    let book = book();
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.bin", sheet),
    ];
    if let Some(r) = sheet_rels {
        parts.push(("xl/worksheets/_rels/sheet1.bin.rels", r.as_bytes()));
    }
    for (name, bytes) in names.iter().zip(tables) {
        parts.push((name, bytes));
    }
    make_zip(&parts)
}
/// Sheet relationships naming `rIdT{n}` → `../tables/table{n}.bin` for n in 1..=count.
pub(super) fn table_rels(count: usize) -> String {
    let mut s = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for n in 1..=count {
        s.push_str(&format!(r#"<Relationship Id="rIdT{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table{n}.bin"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// `BrtBeginList` for `range` = (rwFirst, rwLast, colFirst, colLast).
fn begin_list(range: [u32; 4], name: &str, header: bool, totals: bool) -> Vec<u8> {
    let mut data = u32s(&range);
    data.extend(u32s(&[0, 1, u32::from(header), u32::from(totals), u32::from(totals)]));
    data.extend(NULL.repeat(6)); // No differential formats.
    data.extend(u32s(&[0])); // dwConnID
    data.extend(wide(name));
    data.extend(wide(name));
    data.extend(NULL.repeat(4)); // Comment and three cell style names.
    record(343, &data)
}
/// `BrtBeginListCol` … `BrtEndListCol`, with `extra` records inside.
pub(super) fn list_col(id: u32, name: &str, ilta: u32, total: Option<&str>, extra: &[u8]) -> Vec<u8> {
    let mut data = u32s(&[id, ilta]);
    data.extend(NULL.repeat(3));
    data.extend(u32s(&[0]));
    data.extend(NULL); // stName
    data.extend(wide(name));
    data.extend(nullable(total));
    data.extend(NULL.repeat(3));
    [record(347, &data), extra.to_vec(), record(348, &[])].concat()
}
pub(super) fn style_client(name: &str, flags: u16) -> Vec<u8> {
    let mut data = flags.to_le_bytes().to_vec();
    data.extend(wide(name));
    record(513, &data)
}
/// An AutoFilter block over `range` with filter-column blocks `columns`.
pub(super) fn autofilter(range: [u32; 4], columns: &[Vec<u8>]) -> Vec<u8> {
    [record(161, &u32s(&range)), columns.concat(), record(162, &[])].concat()
}
pub(super) fn filter_column(col: u32, body: &[u8]) -> Vec<u8> {
    let mut data = u32s(&[col]);
    data.extend([0, 0]);
    [record(163, &data), body.to_vec(), record(164, &[])].concat()
}
fn values_filter(values: &[&str]) -> Vec<u8> {
    let mut out = record(165, &u32s(&[0, 0]));
    for v in values {
        out.extend(record(167, &wide(v)));
    }
    out.extend(record(166, &[]));
    out
}
fn custom_number(and: bool, sign: u8, n: f64) -> Vec<u8> {
    let mut c = vec![4, sign];
    c.extend(n.to_le_bytes());
    [record(172, &u32s(&[u32::from(!and)])), record(174, &c), record(173, &[])].concat()
}
/// The "Sales" table part over `range`, columns Region/Units/Price, with `af` and `cols_extra`
/// spliced in (`cols_extra` replaces the plain columns when given).
pub(super) fn sales_table(name: &str, range: [u32; 4], totals: bool, af: &[u8], cols: Option<Vec<u8>>) -> Vec<u8> {
    let cols = cols
        .unwrap_or_else(|| [list_col(1, "Region", 0, None, &[]), list_col(2, "Units", 0, None, &[]), list_col(3, "Price", 0, None, &[])].concat());
    [
        begin_list(range, name, true, totals),
        af.to_vec(),
        record(345, &u32s(&[3])),
        cols,
        record(346, &[]),
        style_client("TableStyleMedium2", 4),
        record(344, &[]),
    ]
    .concat()
}
pub(super) fn sales_cells() -> Vec<u8> {
    let mut data = [row(0), text_cell(0, "Region"), text_cell(1, "Units"), text_cell(2, "Price")].concat();
    for (r, (region, units, price)) in [("East", 5.0, 1.5), ("West", 7.0, 2.0), ("East", 2.0, 3.25)].into_iter().enumerate() {
        data.extend([row(r as u32 + 1), text_cell(0, region), number_cell(1, units), number_cell(2, price)].concat());
    }
    data
}
fn read(bytes: &[u8]) -> (Workbook, ReadReport) {
    read_xlsb(bytes).unwrap()
}
fn one_table(table: Vec<u8>) -> (Workbook, ReadReport) {
    read(&package(&worksheet(&sales_cells(), &list_parts(&["rIdT1"])), Some(&table_rels(1)), &[table]))
}
fn warned(report: &ReadReport, text: &str) -> bool {
    report.warnings.iter().any(|w| w.contains(text))
}

#[test]
fn xlsb_table_matches_the_xlsx_reader() {
    let (wb, _) = one_table(sales_table("Sales", [0, 3, 0, 2], false, &autofilter([0, 3, 0, 2], &[]), None));
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.tables.len(), 1);
    let xlsb = &sheet.tables[0];

    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdT1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#;
    let table = r#"<?xml version="1.0"?><table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="Sales" displayName="Sales" ref="A1:C4" totalsRowShown="0"><autoFilter ref="A1:C4"/><tableColumns count="3"><tableColumn id="1" name="Region"/><tableColumn id="2" name="Units"/><tableColumn id="3" name="Price"/></tableColumns><tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>"#;
    let body = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Region</t></is></c><c r="B1" t="inlineStr"><is><t>Units</t></is></c><c r="C1" t="inlineStr"><is><t>Price</t></is></c></row></sheetData><tableParts count="1"><tablePart r:id="rIdT1"/></tableParts>"#;
    let xlsx = minimal(body, &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/tables/table1.xml", table)], "", "");
    let (xwb, _) = read_xlsx(&xlsx).unwrap();
    let expected: &Table = &xwb.sheets[0].tables[0];
    assert_eq!(&Table { id: expected.id, ..xlsb.clone() }, expected);
    assert_eq!(xlsb.range, RangeRef::parse("A1:C4").unwrap());
    assert!(xlsb.header_row && xlsb.banded_rows && !xlsb.banded_cols && xlsb.filter_button);
    assert_eq!(xlsb.style, "TableStyleMedium2");
    assert_eq!(wb.table("sales"), Some((0, 0)));
    assert_eq!(sheet.value(CellRef::new(1, 0)), Value::Text("East".into()));
}

#[test]
fn xlsb_table_totals_row_keeps_functions_and_labels() {
    let cols = [
        list_col(1, "Region", 0, Some("Total"), &[]),
        list_col(2, "Units", 6, None, &record(352, &[0, 0, 0, 0, 0])),
        list_col(3, "Price", 9, None, &record(352, &[0, 0, 0, 0, 0])),
    ]
    .concat();
    let (wb, report) = one_table(sales_table("Sales", [0, 4, 0, 2], true, &[], Some(cols)));
    let t = &wb.sheets[0].tables[0];
    assert!(t.totals_row && !t.filter_button);
    assert_eq!(t.columns[0].totals, TotalsFn::None);
    assert_eq!(t.columns[0].totals_label.as_deref(), Some("Total"));
    assert_eq!(t.columns[1].totals, TotalsFn::Sum);
    assert_eq!(t.columns[1].totals_label, None);
    assert_eq!(t.columns[2].totals, TotalsFn::Custom);
    assert!(warned(&report, "custom totals row formulas"));
    assert_eq!(report.warnings.iter().filter(|w| w.contains("custom totals")).count(), 1);
}

#[test]
fn xlsb_table_filter_column_and_calculated_columns() {
    let af = autofilter([0, 3, 0, 2], &[filter_column(0, &values_filter(&["East"]))]);
    let cols =
        [list_col(1, "Region", 0, None, &[]), list_col(2, "Units", 0, None, &record(351, &[0, 0, 0, 0, 0])), list_col(3, "Price", 0, None, &[])]
            .concat();
    let (wb, report) = one_table(sales_table("Sales", [0, 3, 0, 2], false, &af, Some(cols)));
    let t = &wb.sheets[0].tables[0];
    assert!(t.filter_button);
    assert_eq!(t.filter, vec![(0, FilterCriterion::Values { values: vec!["East".into()], blanks: false })]);
    assert_eq!(t.columns[1].formula, None);
    assert!(warned(&report, "calculated column formulas"));
}

#[test]
fn xlsb_sheet_autofilter_with_a_custom_filter() {
    let af = autofilter([0, 9, 4, 5], &[filter_column(0, &custom_number(true, 4, 5.0))]);
    let (wb, _) = read(&package(&worksheet(&sales_cells(), &af), None, &[]));
    let sheet = &wb.sheets[0];
    assert!(sheet.tables.is_empty());
    let af = sheet.autofilter.as_ref().unwrap();
    assert_eq!(af.range, RangeRef::parse("E1:F10").unwrap());
    assert_eq!(af.criteria, vec![(0, FilterCriterion::Custom { a: (">".into(), "5".into()), b: None, and: true })]);

    // Two OR-ed comparisons, a top-10 and an above-average column.
    let mut two = vec![4, 1];
    two.extend(1.0f64.to_le_bytes());
    let mut text = vec![6, 2];
    text.extend([0; 8]);
    text.extend(wide("a*"));
    let custom = [record(172, &u32s(&[1])), record(174, &two), record(174, &text), record(173, &[])].concat();
    let mut top = vec![3];
    top.extend(10.0f64.to_le_bytes());
    top.extend(0.0f64.to_le_bytes());
    let mut dynamic = u32s(&[1]);
    dynamic.extend([0; 17]);
    let af = autofilter([0, 9, 0, 3], &[filter_column(0, &custom), filter_column(1, &record(170, &top)), filter_column(2, &record(171, &dynamic))]);
    let (wb, _) = read(&package(&worksheet(&[], &af), None, &[]));
    let criteria = &wb.sheets[0].autofilter.as_ref().unwrap().criteria;
    assert_eq!(
        criteria,
        &vec![
            (0, FilterCriterion::Custom { a: ("<".into(), "1".into()), b: Some(("=".into(), "a*".into())), and: false }),
            (1, FilterCriterion::Top10 { bottom: false, percent: true, count: 10 }),
            (2, FilterCriterion::AboveAverage(true)),
        ]
    );
}

#[test]
fn xlsb_missing_or_malformed_table_parts_are_skipped_with_a_warning() {
    let table = sales_table("Sales", [0, 3, 0, 2], false, &[], None);
    // No relationship for the id, no sheet relationships at all, and a null id.
    for (ids, rels) in [(vec!["rIdT9"], Some(table_rels(1))), (vec!["rIdT1"], None)] {
        let (wb, report) = read(&package(&worksheet(&sales_cells(), &list_parts(&ids)), rels.as_deref(), std::slice::from_ref(&table)));
        assert!(wb.sheets[0].tables.is_empty());
        assert!(warned(&report, "part is missing or malformed"));
        assert_eq!(wb.sheets[0].value(CellRef::new(0, 1)), Value::Text("Units".into()));
        assert_eq!(wb.sheets[0].value(CellRef::new(3, 2)), Value::Number(3.25));
    }
    let null_id = [record(660, &u32s(&[1])), record(661, &NULL), record(662, &[])].concat();
    let (wb, report) = read(&package(&worksheet(&sales_cells(), &null_id), None, &[]));
    assert!(wb.sheets[0].tables.is_empty() && warned(&report, "part is missing or malformed"));
    // Every truncation of the table part, and a range past the sheet, open the sheet without it.
    for n in 0..table.len() {
        let (wb, report) = one_table(table[..n].to_vec());
        assert!(wb.sheets[0].tables.is_empty(), "{n}");
        assert!(warned(&report, "part is missing or malformed"), "{n}");
        assert_eq!(wb.sheets[0].value(CellRef::new(3, 2)), Value::Number(3.25));
    }
    let (wb, report) = one_table(sales_table("Sales", [0, 3, 0, 16384], false, &[], None));
    assert!(wb.sheets[0].tables.is_empty() && warned(&report, "part is missing or malformed"));
    // Columns that don't match the range width.
    let (wb, report) = one_table(sales_table("Sales", [0, 3, 0, 3], false, &[], None));
    assert!(wb.sheets[0].tables.is_empty() && warned(&report, "doesn't fit"));
}

#[test]
fn xlsb_duplicate_table_names_skip_the_second_table() {
    let first = sales_table("Sales", [0, 3, 0, 2], false, &[], None);
    let second = sales_table("SALES", [0, 3, 4, 6], false, &[], None);
    let third = sales_table("Other", [0, 3, 1, 3], false, &[], None); // Overlaps the first.
    let fourth = sales_table("More", [5, 8, 0, 2], false, &[], None);
    let sheet = worksheet(&sales_cells(), &list_parts(&["rIdT1", "rIdT2", "rIdT3", "rIdT4"]));
    let (wb, report) = read(&package(&sheet, Some(&table_rels(4)), &[first, second, third, fourth]));
    let tables = &wb.sheets[0].tables;
    assert_eq!(tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Sales", "More"]);
    assert_eq!(tables.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(tables[0].range, RangeRef::parse("A1:C4").unwrap());
    assert!(warned(&report, "duplicate name"));
    assert!(warned(&report, "overlapping"));
}
