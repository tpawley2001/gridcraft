//! XLSB PivotTables: packages assembled in Rust from the public [MS-XLSB] record layouts
//! (`crate::testing::xlsb_pivot`), compared with what the .xlsx reader gives for the same
//! PivotTable written as XML. The refreshed report is checked in the engine's integration tests.

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{PivotFilter, PivotFunc, PivotLayout, PivotSort, PivotTable, Workbook};

use crate::testing::xlsb_pivot::{PivotCase, cache_part, view_part, xlsb, xlsb_with, xlsx};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

fn pivots(wb: &Workbook) -> Vec<PivotTable> {
    wb.sheets.iter().find(|s| s.name == "Report").unwrap().pivots.clone()
}
fn from_xlsb(case: PivotCase) -> (Workbook, ReadReport) {
    read_xlsb(&xlsb(case)).unwrap()
}
fn pivot_warnings(report: &ReadReport) -> Vec<&String> {
    report.warnings.iter().filter(|w| w.contains("Pivot") || w.contains("pivot")).collect()
}

/// The XLSB PivotTable equals the .xlsx reader's, field by field, without warnings.
fn same_as_xlsx(case: PivotCase) -> PivotTable {
    let (wb, report) = from_xlsb(case);
    assert!(pivot_warnings(&report).is_empty(), "{:?}", report.warnings);
    let (xwb, xreport) = read_xlsx(&xlsx(case)).unwrap();
    assert!(pivot_warnings(&xreport).is_empty(), "{:?}", xreport.warnings);
    let (got, want) = (pivots(&wb), pivots(&xwb));
    assert_eq!(got.len(), 1, "{case:?}");
    assert_eq!(want.len(), 1, "{case:?}");
    let (g, w) = (&got[0], &want[0]);
    assert_eq!(g.name, w.name);
    assert_eq!(g.source, w.source);
    assert_eq!(g.anchor, w.anchor);
    assert_eq!(g.last_range, w.last_range);
    assert_eq!(g.rows, w.rows);
    assert_eq!(g.columns, w.columns);
    assert_eq!(g.values, w.values);
    assert_eq!(g.filters, w.filters);
    assert_eq!(g.layout, w.layout);
    assert_eq!((g.grand_totals_rows, g.grand_totals_cols), (w.grand_totals_rows, w.grand_totals_cols));
    assert_eq!((g.subtotals, g.subtotals_top), (w.subtotals, w.subtotals_top));
    assert_eq!((g.style.as_str(), g.show_headers, g.enable_show_details), (w.style.as_str(), w.show_headers, w.enable_show_details));
    assert_eq!((g.values_on_rows, g.values_position), (w.values_on_rows, w.values_position));
    assert_eq!(g, w);
    g.clone()
}

#[test]
fn rows_columns_and_sum_match_xlsx() {
    let pt = same_as_xlsx(PivotCase::Basic);
    assert_eq!(pt.name, "PivotTable1");
    assert_eq!(pt.source, "Data!$A$1:$C$5");
    assert_eq!(pt.anchor, CellRef::parse("A3").unwrap());
    assert_eq!(pt.last_range, RangeRef::parse("A3:D7"));
    assert_eq!(pt.rows.iter().map(|f| f.source_col.as_str()).collect::<Vec<_>>(), ["Region"]);
    assert_eq!(pt.columns.iter().map(|f| f.source_col.as_str()).collect::<Vec<_>>(), ["Product"]);
    assert_eq!(pt.rows[0].sort, PivotSort::None);
    assert_eq!(pt.values.len(), 1);
    assert_eq!((pt.values[0].source_col.as_str(), pt.values[0].func, pt.values[0].name.as_str()), ("Sales", PivotFunc::Sum, "Sum of Sales"));
    assert_eq!(pt.layout, PivotLayout::Compact);
    assert_eq!(pt.style, "PivotStyleLight16");
    assert!(pt.grand_totals_rows && pt.grand_totals_cols && pt.subtotals && pt.show_headers && pt.enable_show_details);
    assert!(pt.filters.is_empty());
}

#[test]
fn page_field_selection_matches_xlsx() {
    let pt = same_as_xlsx(PivotCase::PageFilter);
    assert_eq!(pt.filters, vec![PivotFilter { source_col: "Region".into(), selected: Some(vec!["East".into()]) }]);
    assert_eq!(pt.rows.iter().map(|f| f.source_col.as_str()).collect::<Vec<_>>(), ["Product"]);
    assert!(pt.columns.is_empty());
    // The page field sits two rows above the body.
    assert_eq!(pt.last_range, RangeRef::parse("A1:B6"));
}

#[test]
fn count_data_field_matches_xlsx() {
    let pt = same_as_xlsx(PivotCase::Count);
    assert_eq!((pt.values[0].func, pt.values[0].name.as_str()), (PivotFunc::Count, "Count of Sales"));
}

#[test]
fn table_source_matches_xlsx() {
    let pt = same_as_xlsx(PivotCase::TableSource);
    assert_eq!(pt.source, "SalesTbl");
    let (wb, _) = from_xlsb(PivotCase::TableSource);
    assert_eq!(wb.sheets[0].tables.len(), 1);
}

#[test]
fn missing_cache_part_keeps_the_report_values() {
    let (wb, report) = from_xlsb(PivotCase::MissingCache);
    assert!(pivots(&wb).is_empty());
    assert_eq!(pivot_warnings(&report).len(), 1, "{:?}", report.warnings);
    let sheet = wb.sheets.iter().find(|s| s.name == "Report").unwrap();
    assert_eq!(sheet.cells.get(CellRef::parse("A3").unwrap()).map(|c| c.value.clone()), Some(Value::text("Sum of Sales")));
    assert_eq!(sheet.cells.get(CellRef::parse("D7").unwrap()).map(|c| c.value.clone()), Some(Value::number(100.0)));
}

#[test]
fn truncated_view_record_warns_once() {
    let (wb, report) = from_xlsb(PivotCase::TruncatedView);
    assert!(pivots(&wb).is_empty());
    let warnings = pivot_warnings(&report);
    assert_eq!(warnings.len(), 1, "{:?}", report.warnings);
    assert!(warnings[0].contains("damaged XLSB PivotTable"), "{warnings:?}");
    let sheet = wb.sheets.iter().find(|s| s.name == "Report").unwrap();
    assert_eq!(sheet.cells.get(CellRef::parse("A5").unwrap()).map(|c| c.value.clone()), Some(Value::text("East")));
}

#[test]
fn xlsb_pivot_survives_xlsx_save_and_reopen() {
    for case in [PivotCase::Basic, PivotCase::PageFilter, PivotCase::Count, PivotCase::TableSource] {
        let (wb, _) = from_xlsb(case);
        let before = pivots(&wb);
        let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
        assert!(pivot_warnings(&report).is_empty(), "{case:?}: {:?}", report.warnings);
        let after = pivots(&back);
        assert_eq!(after.len(), 1, "{case:?}");
        let strip = |p: &PivotTable| PivotTable { id: 0, ..p.clone() };
        assert_eq!(strip(&after[0]), strip(&before[0]), "{case:?}");
    }
}

/// Every truncation of either part imports (keeping the cells) without a panic; a pivot that
/// can't be read leaves a warning.
#[test]
fn truncated_parts_never_abort_the_import() {
    let (cache, view) = (cache_part(PivotCase::Basic), view_part(PivotCase::Basic));
    for n in 0..cache.len() {
        let (wb, report) = read_xlsb(&xlsb_with(PivotCase::Basic, &cache[..n], &view)).unwrap();
        assert!(pivots(&wb).is_empty() || !pivot_warnings(&report).is_empty() || n + 1 >= cache.len(), "cache cut at {n}");
    }
    for n in 0..view.len() {
        let (wb, report) = read_xlsb(&xlsb_with(PivotCase::Basic, &cache, &view[..n])).unwrap();
        if pivots(&wb).is_empty() {
            assert!(!pivot_warnings(&report).is_empty(), "view cut at {n}");
        }
    }
}

/// Hostile counts and indices in otherwise well-formed records: no panic, no huge allocation.
#[test]
fn hostile_counts_and_indices_are_bounded() {
    use crate::testing::xlsb_pivot::record;
    let cache = cache_part(PivotCase::Basic);
    let mut view = view_part(PivotCase::Basic);
    // An axis list claiming four billion fields, a data item on a missing field, a page field
    // with an out-of-range item and a pivot item pointing past the cache items.
    let end = record(315, &[]);
    view.truncate(view.len() - end.len());
    view.extend(record(311, &[0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0]));
    view.extend(record(293, &[[0x77u8, 0, 0, 0].as_slice(), &[0xee; 4], &[0; 16], &[0]].concat()));
    view.extend(record(289, &[[0u8; 4].as_slice(), &[0x55; 4], &[0xff; 4], &[0]].concat()));
    view.extend(record(282, &[0, 0, 0, 0x99, 0x99, 0, 0]));
    view.extend(end);
    let (_, report) = read_xlsb(&xlsb_with(PivotCase::Basic, &cache, &view)).unwrap();
    assert!(!pivot_warnings(&report).is_empty(), "{:?}", report.warnings);
    // A cache item run claiming four billion items.
    let mut run = 2u16.to_le_bytes().to_vec();
    run.extend([0xff; 4]);
    let bad_cache = [cache.clone(), record(191, &run)].concat();
    let (wb, _) = read_xlsb(&xlsb_with(PivotCase::Basic, &bad_cache, &view_part(PivotCase::Basic))).unwrap();
    assert!(wb.sheets.len() == 2);
}
