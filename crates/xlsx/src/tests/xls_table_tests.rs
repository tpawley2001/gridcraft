//! Tables in legacy `.xls` (BIFF8) files: Feature11/Feature12, ContinueFrt11 and List12 records
//! ([MS-XLS] 2.4.114, 2.4.157, 2.5.266, 2.5.113). Every file is assembled here in code.

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{Table, TotalsFn, Workbook};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s, v, xlstr};
use crate::{read_xls, read_xlsx, write_xlsx};

const FEATURE11: u16 = 0x0872;
const FEATURE12: u16 = 0x0878;
const CONTINUE_FRT11: u16 = 0x0875;
const LIST12: u16 = 0x0877;
const FEAT_HDR11: u16 = 0x0871;

/// A workbook with one worksheet per entry (name, sheet records).
fn book(sheets: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let globals = |positions: &[u32]| {
        let mut g = bof(0x0005);
        for ((name, _), pos) in sheets.iter().zip(positions) {
            g.extend(rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, name.len() as u8, 0], name.as_bytes()])));
        }
        g.extend(eof());
        g
    };
    let bodies: Vec<Vec<u8>> = sheets.iter().map(|(_, recs)| cat(&[&bof(0x0010), recs, &eof()])).collect();
    let mut positions = vec![0u32; sheets.len()];
    let mut at = globals(&positions).len() as u32;
    for (p, b) in positions.iter_mut().zip(&bodies) {
        *p = at;
        at += b.len() as u32;
    }
    let mut stream = globals(&positions);
    for b in &bodies {
        stream.extend_from_slice(b);
    }
    compound("Workbook", &stream)
}

fn label(r: u16, c: u16, s: &str) -> Vec<u8> {
    rec(0x0204, &cat(&[&cell(r, c, 0), &xlstr(s)]))
}

fn number(r: u16, c: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&cell(r, c, 0), &n.to_le_bytes()]))
}

/// A header row of labels and numeric data rows below it.
fn grid(r0: u16, c0: u16, header: &[&str], rows: u16) -> Vec<u8> {
    let mut out = vec![];
    for (i, h) in header.iter().enumerate() {
        out.extend(label(r0, c0 + i as u16, h));
    }
    for r in 1..=rows {
        for i in 0..header.len() as u16 {
            out.extend(number(r0 + r, c0 + i, f64::from(r * 10 + i)));
        }
    }
    out
}

struct Feat<'a> {
    id: u16,
    /// rwFirst, rwLast, colFirst, colLast.
    range: [u16; 4],
    id_list: u32,
    header: bool,
    totals: bool,
    auto_filter: bool,
    name: &'a str,
    fields: &'a [&'a str],
}

impl<'a> Feat<'a> {
    fn new(range: [u16; 4], name: &'a str, fields: &'a [&'a str]) -> Self {
        Feat { id: FEATURE12, range, id_list: 1, header: true, totals: false, auto_filter: true, name, fields }
    }

    /// Feat11FieldDataItem for one column.
    fn field(&self, i: usize, name: &str) -> Vec<u8> {
        let mut out = u16s(&[]);
        for x in [i as u32 + 1, 0, 0, 0, 0, u32::MAX, 0, 0, u32::MAX] {
            out.extend(x.to_le_bytes());
        }
        out.extend(xlstr(name));
        out.extend(xlstr(name));
        if self.auto_filter {
            // Feat11FdaAutoFilter with an empty recAutoFilter.
            out.extend(0u32.to_le_bytes());
            out.extend([0, 0]);
        }
        if !self.header {
            // CachedDiskHeader with no formatting.
            out.extend(0u32.to_le_bytes());
        }
        out
    }

    /// The whole record payload (FrtRefHeaderU, header, refs2, TableFeatureType).
    fn payload(&self) -> Vec<u8> {
        let mut feat = vec![];
        let flags: u32 = if self.auto_filter { 1 << 1 } else { 0 } | (0xC << 16);
        for x in [0, self.id_list, self.header as u32, self.totals as u32, self.fields.len() as u32 + 1, 64] {
            feat.extend(x.to_le_bytes());
        }
        feat.extend(u16s(&[0, 0]));
        feat.extend(flags.to_le_bytes());
        feat.extend([0u8; 32]);
        feat.extend(xlstr(self.name));
        feat.extend((self.fields.len() as u16).to_le_bytes());
        for (i, f) in self.fields.iter().enumerate() {
            feat.extend(self.field(i, f));
        }
        let mut out = u16s(&[self.id, 0]);
        out.extend(u16s(&self.range));
        out.extend(5u16.to_le_bytes());
        out.extend([0u8; 5]);
        out.extend(1u16.to_le_bytes());
        out.extend((feat.len() as u32).to_le_bytes());
        out.extend([0u8; 2]);
        out.extend(u16s(&self.range));
        out.extend(feat);
        out
    }

    fn record(&self) -> Vec<u8> {
        rec(self.id, &self.payload())
    }
}

fn list12(lsd: u16, id_list: u32, rgb: &[u8]) -> Vec<u8> {
    rec(LIST12, &cat(&[&u16s(&[LIST12, 0]), &[0u8; 8], &lsd.to_le_bytes(), &id_list.to_le_bytes(), rgb]))
}

/// List12TableStyleClientInfo.
fn style(name: &str, first: bool, last: bool, rows: bool, cols: bool) -> Vec<u8> {
    let flags = first as u16 | (last as u16) << 1 | (rows as u16) << 2 | (cols as u16) << 3;
    cat(&[&flags.to_le_bytes(), &xlstr(name)])
}

/// List12DisplayName: name, empty comment.
fn display_name(name: &str) -> Vec<u8> {
    cat(&[&xlstr(name), &xlstr("")])
}

fn feat_hdr11() -> Vec<u8> {
    rec(FEAT_HDR11, &cat(&[&u16s(&[FEAT_HDR11, 0]), &[0u8; 8], &5u16.to_le_bytes(), &[1], &u32::MAX.to_le_bytes(), &[0, 0, 0, 0]]))
}

fn range(a1: &str) -> RangeRef {
    RangeRef::parse(a1).unwrap()
}

fn names(t: &Table) -> Vec<&str> {
    t.columns.iter().map(|c| c.name.as_str()).collect()
}

fn sales_sheet(totals: bool) -> Vec<u8> {
    let rows = if totals { 4 } else { 3 };
    let mut f = Feat::new([0, rows, 0, 2], "Sales", &["Region", "Qty", "Price"]);
    f.totals = totals;
    cat(&[
        &grid(0, 0, &["Region", "Qty", "Price"], rows),
        &feat_hdr11(),
        &f.record(),
        &list12(1, 1, &style("TableStyleLight9", false, false, true, false)),
        &list12(0, 1, &[0u8; 4]),
    ])
}

fn read(file: &[u8]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xls(file).unwrap();
    (wb, report.warnings)
}

#[test]
fn feature12_table_with_header_style_and_columns() {
    let (wb, warnings) = read(&book(&[("Sheet1", sales_sheet(false))]));
    let tables = &wb.sheets[0].tables;
    assert_eq!(tables.len(), 1, "{warnings:?}");
    let t = &tables[0];
    assert_eq!((t.id, t.name.as_str(), t.range), (1, "Sales", range("A1:C4")));
    assert!(t.header_row && !t.totals_row);
    assert_eq!(names(t), ["Region", "Qty", "Price"]);
    assert_eq!(t.style, "TableStyleLight9");
    assert!(t.banded_rows && !t.banded_cols && !t.first_col && !t.last_col);
    assert!(t.filter_button);
    assert_eq!(t.data_range(), Some(range("A2:C4")));
    assert_eq!(v(&wb, 0, "B2"), Value::Number(11.0), "cells are kept");
    assert!(!warnings.iter().any(|w| w.contains("tables on sheet")), "{warnings:?}");
}

#[test]
fn totals_row_is_imported() {
    let (wb, _) = read(&book(&[("Sheet1", sales_sheet(true))]));
    let t = &wb.sheets[0].tables[0];
    assert_eq!(t.range, range("A1:C5"));
    assert!(t.totals_row);
    assert_eq!(t.data_range(), Some(range("A2:C4")));
}

#[test]
fn excel_2003_list_without_list12_takes_the_xlsx_defaults() {
    let mut f = Feat::new([2, 5, 1, 2], "List1", &["A", "B"]);
    f.id = FEATURE11;
    f.auto_filter = false;
    let (wb, _) = read(&book(&[("Sheet1", cat(&[&grid(2, 1, &["A", "B"], 3), &f.record()]))]));
    let t = &wb.sheets[0].tables[0];
    assert_eq!((t.name.as_str(), t.range), ("List1", range("B3:C6")));
    // As the .xlsx reader gives a table without <tableStyleInfo> or <autoFilter>.
    assert_eq!(t.style, "");
    assert!(!t.banded_rows && !t.banded_cols && !t.first_col && !t.last_col && !t.filter_button);
    assert!(t.header_row && !t.totals_row);
}

#[test]
fn display_name_overrides_the_feature_name() {
    let f = Feat::new([0, 3, 0, 1], "Table1", &["x", "y"]);
    let sheet = cat(&[&grid(0, 0, &["x", "y"], 3), &f.record(), &list12(2, 1, &display_name("Orders"))]);
    let (wb, _) = read(&book(&[("Sheet1", sheet)]));
    assert_eq!(wb.sheets[0].tables[0].name, "Orders");
}

// Structured references (`=SUM(Sales[Qty])`) are stored in BIFF8 formulas as PtgList, which the
// .xls formula decoder does not support yet (such a formula keeps its saved value), so that
// acceptance case is not tested here.

#[test]
fn imported_table_survives_an_xlsx_round_trip() {
    let (wb, _) = read(&book(&[("Sheet1", sales_sheet(true))]));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let (a, b) = (&wb.sheets[0].tables[0], &back.sheets[0].tables[0]);
    assert_eq!((a.id, &a.name, a.range), (b.id, &b.name, b.range));
    assert_eq!(names(a), names(b));
    assert_eq!(
        (a.header_row, a.totals_row, a.filter_button, &a.style, a.banded_rows, a.banded_cols, a.first_col, a.last_col),
        (b.header_row, b.totals_row, b.filter_button, &b.style, b.banded_rows, b.banded_cols, b.first_col, b.last_col)
    );
    assert_eq!(a, b);
}

#[test]
fn every_truncation_of_a_feature_skips_only_the_table() {
    let payload = Feat::new([0, 3, 0, 2], "Sales", &["Region", "Qty", "Price"]).payload();
    // Field data past the table's name only loses column names, which come from the header row.
    for n in 0..payload.len() {
        let sheet = cat(&[&grid(0, 0, &["Region", "Qty", "Price"], 3), &rec(FEATURE12, &payload[..n])]);
        let (wb, warnings) = read(&book(&[("Sheet1", sheet)]));
        assert_eq!(v(&wb, 0, "C4"), Value::Number(32.0), "cut at {n}");
        let skipped = warnings.iter().filter(|w| w.contains("tables on sheet")).count();
        assert!(skipped <= 1, "cut at {n}: {warnings:?}");
        match wb.sheets[0].tables.first() {
            None => assert_eq!(skipped, 1, "cut at {n}"),
            Some(t) => {
                assert_eq!(skipped, 0, "cut at {n}");
                assert_eq!(names(t), ["Region", "Qty", "Price"], "cut at {n}");
            }
        }
    }
    // Cut before the column count: no table.
    let sheet = cat(&[&grid(0, 0, &["Region", "Qty", "Price"], 3), &rec(FEATURE12, &payload[..40])]);
    assert!(read(&book(&[("Sheet1", sheet)])).0.sheets[0].tables.is_empty());
}

#[test]
fn bad_ranges_and_overlaps_are_skipped() {
    let ok = Feat::new([0, 3, 0, 1], "Good", &["a", "b"]);
    let mut overlap = Feat::new([2, 5, 1, 2], "Overlap", &["c", "d"]);
    overlap.id_list = 2;
    let mut reversed = Feat::new([9, 7, 0, 1], "Reversed", &["e", "f"]);
    reversed.id_list = 3;
    let mut merged = Feat::new([10, 12, 0, 1], "Merged", &["g", "h"]);
    merged.id_list = 4;
    let merge = rec(0x00E5, &u16s(&[1, 11, 11, 0, 1]));
    let sheet = cat(&[&grid(0, 0, &["a", "b"], 3), &ok.record(), &overlap.record(), &reversed.record(), &merged.record(), &merge]);
    let (wb, warnings) = read(&book(&[("Sheet1", sheet)]));
    let tables = &wb.sheets[0].tables;
    assert_eq!(tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Good"]);
    assert_eq!(warnings.iter().filter(|w| w.contains("tables on sheet")).count(), 1, "{warnings:?}");
}

#[test]
fn list12_for_a_missing_table_is_ignored_and_long_features_continue() {
    let f = Feat::new([0, 3, 0, 2], "Sales", &["Region", "Qty", "Price"]);
    let payload = f.payload();
    let (head, tail) = payload.split_at(60);
    let sheet = cat(&[
        &grid(0, 0, &["Region", "Qty", "Price"], 3),
        &rec(FEATURE12, head),
        &rec(CONTINUE_FRT11, &cat(&[&u16s(&[CONTINUE_FRT11, 0]), tail])),
        &list12(2, 99, &display_name("Nope")),
    ]);
    let (wb, warnings) = read(&book(&[("Sheet1", sheet)]));
    let t = &wb.sheets[0].tables[0];
    assert_eq!(t.name, "Sales");
    assert_eq!(names(t), ["Region", "Qty", "Price"]);
    assert!(!warnings.iter().any(|w| w.contains("tables on sheet")), "{warnings:?}");
}

#[test]
fn tables_on_several_sheets_get_unique_names_and_ids() {
    let a = Feat::new([0, 3, 0, 1], "Data", &["a", "b"]);
    let mut b = Feat::new([0, 3, 4, 5], "Data", &["c", "d"]);
    b.id_list = 2;
    let c = Feat::new([0, 2, 0, 0], "Other", &["e"]);
    let s1 = cat(&[&grid(0, 0, &["a", "b"], 3), &grid(0, 4, &["c", "d"], 3), &a.record(), &b.record()]);
    let s2 = cat(&[&grid(0, 0, &["e"], 2), &c.record()]);
    let (wb, _) = read(&book(&[("One", s1), ("Two", s2)]));
    let all: Vec<&Table> = wb.sheets.iter().flat_map(|s| &s.tables).collect();
    assert_eq!(all.len(), 3);
    let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Data", "Data2", "Other"]);
    let mut ids: Vec<u32> = all.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 3);
}

#[test]
fn missing_field_names_come_from_the_header_row_or_column_n() {
    // Two field items for a three-column range; the third column takes the header's text.
    let f = Feat::new([0, 3, 0, 2], "T", &["a", "b"]);
    let (wb, _) = read(&book(&[("Sheet1", cat(&[&grid(0, 0, &["a", "b", "c"], 3), &f.record()]))]));
    assert_eq!(names(&wb.sheets[0].tables[0]), ["a", "b", "c"]);
    // Without a header row: Column1, Column2.
    let mut f = Feat::new([0, 3, 0, 1], "T", &[]);
    f.header = false;
    f.auto_filter = false;
    let (wb, _) = read(&book(&[("Sheet1", cat(&[&grid(1, 0, &["x", "y"], 2), &f.record()]))]));
    let t = &wb.sheets[0].tables[0];
    assert!(!t.header_row && !t.filter_button);
    assert_eq!(names(t), ["Column1", "Column2"]);
    assert_eq!(t.columns.iter().map(|c| c.totals).collect::<Vec<_>>(), [TotalsFn::None; 2]);
}

#[test]
fn malformed_list12_is_skipped_with_a_warning() {
    let f = Feat::new([0, 3, 0, 1], "T", &["a", "b"]);
    let sheet = cat(&[&grid(0, 0, &["a", "b"], 3), &f.record(), &list12(1, 1, &[1]), &rec(LIST12, &[0; 5])]);
    let (wb, warnings) = read(&book(&[("Sheet1", sheet)]));
    assert_eq!(wb.sheets[0].tables.len(), 1);
    assert_eq!(wb.sheets[0].tables[0].style, "");
    assert_eq!(warnings.iter().filter(|w| w.contains("tables on sheet")).count(), 1, "{warnings:?}");
    assert_eq!(wb.sheets[0].cell(CellRef::new(0, 0)).map(|c| c.value.clone()), Some(Value::from("a".to_string())));
}
