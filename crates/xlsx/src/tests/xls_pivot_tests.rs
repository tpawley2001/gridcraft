//! `.xls` PivotTable import: pivot caches (SXStreamID, SXVS, DConRef and the `_SX_DB_CUR`
//! streams) and PivotTable views (SxView, Sxvd, SXVI, SxIvd, SXPI, SXDI) become model
//! PivotTables that refresh from their source and save to `.xlsx` as PivotTables.

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{PivotFunc, PivotLayout, PivotSort, PivotTable};

use crate::testing::xls_pivot::{Cache, PRODUCT, REGION, SALES, VALUES, View, cache_stream, workbook};
use crate::{read_xls, read_xlsx, write_xlsx};

fn region_by_sales() -> View {
    View { name: "SalesPivot".into(), body: [0, 4, 0, 1], rows: vec![REGION], data: vec![(SALES, 0, None)], ..View::default() }
}

fn two_values() -> View {
    View {
        name: "Mix".into(),
        body: [2, 5, 0, 4],
        rows: vec![VALUES],
        cols: vec![REGION],
        pages: vec![(PRODUCT, 0)],
        data: vec![(SALES, 0, Some("Total Sales".into())), (SALES, 1, Some("Orders".into()))],
    }
}

fn pivots(bytes: &[u8]) -> (Vec<PivotTable>, Vec<String>, gridcraft_model::Workbook) {
    let (wb, report) = read_xls(bytes).unwrap();
    let report_sheet = wb.sheets.iter().position(|s| s.name == "Report").unwrap();
    (wb.sheets[report_sheet].pivots.clone(), report.warnings, wb)
}

#[test]
fn row_field_and_sum_become_a_model_pivot() {
    let (pts, warnings, _) = pivots(&workbook(&[region_by_sales()], &[], Cache::Present));
    assert_eq!(pts.len(), 1, "{warnings:?}");
    let pt = &pts[0];
    assert_eq!(pt.name, "SalesPivot");
    assert_eq!(pt.source, "Sheet1!$A$1:$C$7");
    assert_eq!(pt.anchor, CellRef::new(0, 0));
    assert_eq!(pt.last_range, Some(RangeRef::new(CellRef::new(0, 0), CellRef::new(4, 1))));
    assert_eq!(pt.rows.len(), 1);
    assert_eq!(pt.rows[0].source_col, "Region");
    assert_eq!(pt.rows[0].sort, PivotSort::Asc);
    assert!(pt.columns.is_empty() && pt.filters.is_empty());
    assert_eq!(pt.values.len(), 1);
    assert_eq!((pt.values[0].source_col.as_str(), pt.values[0].func, pt.values[0].name.as_str()), ("Sales", PivotFunc::Sum, "Sum of Sales"));
    assert_eq!(pt.layout, PivotLayout::Tabular);
    assert!(pt.grand_totals_rows && pt.grand_totals_cols);
    assert!(!warnings.iter().any(|w| w.contains("PivotTable")), "{warnings:?}");
}

#[test]
fn page_and_column_fields_with_two_named_data_fields() {
    let (pts, warnings, _) = pivots(&workbook(&[two_values()], &[], Cache::Present));
    assert_eq!(pts.len(), 1, "{warnings:?}");
    let pt = &pts[0];
    assert_eq!(pt.columns.iter().map(|f| f.source_col.as_str()).collect::<Vec<_>>(), ["Region"]);
    assert!(pt.rows.is_empty());
    assert!(pt.values_on_rows, "Σ Values sits in the row fields");
    assert_eq!(pt.filters.len(), 1);
    assert_eq!(pt.filters[0].source_col, "Product");
    assert_eq!(pt.filters[0].selected, Some(vec!["A".to_string()]));
    let values: Vec<_> = pt.values.iter().map(|v| (v.source_col.as_str(), v.func, v.name.as_str())).collect();
    assert_eq!(values, [("Sales", PivotFunc::Sum, "Total Sales"), ("Sales", PivotFunc::Count, "Orders")]);
    // One page field: the report starts two rows above the body.
    assert_eq!(pt.last_range, Some(RangeRef::new(CellRef::new(0, 0), CellRef::new(5, 4))));
}

#[test]
fn a_missing_or_damaged_cache_keeps_the_cells_as_values() {
    let stale = [(1, 1, 30.0)];
    let full = cache_stream();
    let mut caches = vec![Cache::Missing, Cache::Truncated, Cache::Bytes(vec![]), Cache::Bytes(vec![0xC7, 0, 2, 0, 1, 0])];
    caches.extend((0..full.len()).step_by(3).map(|n| Cache::Bytes(full[..n].to_vec())));
    for cache in caches {
        let (pts, warnings, wb) = pivots(&workbook(&[region_by_sales()], &stale, cache.clone()));
        if pts.is_empty() {
            assert!(warnings.iter().any(|w| w.contains("SalesPivot") && w.contains("plain cells")), "{cache:?}: {warnings:?}");
        }
        let report = wb.sheets.iter().find(|s| s.name == "Report").unwrap();
        assert_eq!(report.cells.get(CellRef::new(1, 1)).map(|c| c.value.clone()), Some(Value::number(30.0)), "{cache:?}");
    }
    let (pts, _, _) = pivots(&workbook(&[region_by_sales()], &stale, Cache::Missing));
    assert!(pts.is_empty());
}

#[test]
fn damaged_view_records_never_panic() {
    // Corrupt bytes anywhere in the file (view records, cache stream, directory): no panic.
    let full = workbook(&[two_values(), region_by_sales()], &[], Cache::Present);
    for flip in (0..full.len()).step_by(7) {
        let mut b = full.clone();
        b[flip] ^= 0xA5;
        let _ = read_xls(&b);
    }
}

#[test]
fn an_xls_pivot_saves_to_xlsx_as_a_pivot_table() {
    let (wb, _) = read_xls(&workbook(&[region_by_sales(), View { body: [8, 12, 0, 4], ..two_values() }], &[], Cache::Present)).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let (back, report) = read_xlsx(&bytes).unwrap();
    let sheet = back.sheets.iter().find(|s| s.name == "Report").unwrap();
    assert_eq!(sheet.pivots.len(), 2, "{:?}", report.warnings);
    let first = &sheet.pivots[0];
    assert_eq!(first.name, "SalesPivot");
    assert_eq!(first.source, "Sheet1!$A$1:$C$7");
    assert_eq!(first.rows[0].source_col, "Region");
    assert_eq!(first.values[0].func, PivotFunc::Sum);
    let second = &sheet.pivots[1];
    assert_eq!(second.values.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Total Sales", "Orders"]);
    assert_eq!(second.filters[0].selected, Some(vec!["A".to_string()]));
}
