//! A PivotTable read from an .xlsb file is a live PivotTable: refreshing it rebuilds the report
//! from the source list. Packages built by `gridcraft_xlsx::testing::xlsb_pivot` from the public
//! [MS-XLSB] record layouts.

use gridcraft_engine::core::{CellRef, Value};
use gridcraft_engine::model::Workbook;
use gridcraft_xlsx::testing::xlsb_pivot::{PivotCase, xlsb, xlsx};

/// The workbook with the Report sheet's report cleared and its PivotTable refreshed.
fn refreshed(bytes: &[u8], xlsb_file: bool) -> Workbook {
    let (mut wb, _) = if xlsb_file { gridcraft_xlsx::read_xlsb(bytes).unwrap() } else { gridcraft_xlsx::read_xlsx(bytes).unwrap() };
    let sheet = wb.sheets.iter().position(|s| s.name == "Report").unwrap();
    {
        let report = wb.sheet_mut(sheet).unwrap();
        assert_eq!(report.pivots.len(), 1);
        report.cells = Default::default();
    }
    gridcraft_engine::pivot::refresh(&mut wb, sheet, 0, true).unwrap();
    wb
}

/// The numbers to the right of the row label `label` on the Report sheet.
fn row_numbers(wb: &Workbook, label: &str) -> Vec<f64> {
    let sheet = wb.sheets.iter().find(|s| s.name == "Report").unwrap();
    for r in 0..20 {
        if sheet.cells.get(CellRef::new(r, 0)).is_some_and(|c| c.value == Value::text(label)) {
            return (1..8)
                .filter_map(|c| match sheet.cells.get(CellRef::new(r, c)).map(|c| &c.value) {
                    Some(Value::Number(n)) => Some(*n),
                    _ => None,
                })
                .collect();
        }
    }
    panic!("no row labelled {label}");
}

#[test]
fn xlsb_pivot_refreshes_like_the_xlsx_one() {
    for case in [PivotCase::Basic, PivotCase::TableSource] {
        for wb in [refreshed(&xlsb(case), true), refreshed(&xlsx(case), false)] {
            assert_eq!(row_numbers(&wb, "East"), [10.0, 30.0, 40.0], "{case:?}");
            assert_eq!(row_numbers(&wb, "West"), [20.0, 40.0, 60.0], "{case:?}");
            assert_eq!(row_numbers(&wb, "Grand Total"), [30.0, 70.0, 100.0], "{case:?}");
        }
    }
}

#[test]
fn xlsb_count_pivot_refreshes() {
    let wb = refreshed(&xlsb(PivotCase::Count), true);
    assert_eq!(row_numbers(&wb, "East"), [1.0, 1.0, 2.0]);
    assert_eq!(row_numbers(&wb, "West"), [1.0, 1.0, 2.0]);
    assert_eq!(row_numbers(&wb, "Grand Total"), [2.0, 2.0, 4.0]);
}

#[test]
fn xlsb_page_field_filters_the_refresh() {
    let wb = refreshed(&xlsb(PivotCase::PageFilter), true);
    assert_eq!(row_numbers(&wb, "A"), [10.0]);
    assert_eq!(row_numbers(&wb, "B"), [30.0]);
    assert_eq!(row_numbers(&wb, "Grand Total"), [40.0]);
}

/// Sum of Price (calculated Price = Sales/Units) is sum(Sales) / sum(Units) per Region.
#[test]
fn xlsb_calculated_field_refreshes_like_the_xlsx_one() {
    // East: Sales 10 + 30, Units 2 + 3; West: Sales 20 + 40, Units 4 + 10.
    for case in [PivotCase::Calculated, PivotCase::CalculatedChain] {
        for wb in [refreshed(&xlsb(case), true), refreshed(&xlsx(case), false)] {
            assert_eq!(row_numbers(&wb, "East"), [40.0, 40.0 / 5.0], "{case:?}");
            assert_eq!(row_numbers(&wb, "West"), [60.0, 60.0 / 14.0], "{case:?}");
            assert_eq!(row_numbers(&wb, "Grand Total"), [100.0, 100.0 / 19.0], "{case:?}");
        }
    }
}
