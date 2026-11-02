//! A PivotTable read from an `.xls` file refreshes from its source cells: its report matches the
//! one a PivotTable built through the model API on the same data gives, and follows source edits.

use std::sync::Arc;

use gridcraft_engine::core::{CellRef, Value};
use gridcraft_engine::model::{Cell, PivotField, PivotFunc, PivotLayout, PivotTable, PivotValue, Workbook};
use gridcraft_engine::pivot;
use gridcraft_xlsx::testing::xls_pivot::{Cache, REGION, SALES, View, workbook};

fn report(wb: &mut Workbook, sheet: usize) -> Vec<Vec<Value>> {
    let area = pivot::refresh(wb, sheet, 0, true).unwrap();
    let s = wb.sheet(sheet).unwrap();
    (area.start.row..=area.end.row)
        .map(|r| (area.start.col..=area.end.col).map(|c| s.cells.get(CellRef::new(r, c)).map(|c| c.value.clone()).unwrap_or(Value::Empty)).collect())
        .collect()
}

#[test]
fn xls_pivot_renders_like_a_model_pivot_and_refreshes() {
    let view = View { name: "SalesPivot".into(), body: [0, 4, 0, 1], rows: vec![REGION], data: vec![(SALES, 0, None)], ..View::default() };
    let (mut wb, _) = gridcraft_xlsx::read_xls(&workbook(&[view], &[], Cache::Present)).unwrap();
    let sheet = wb.sheets.iter().position(|s| s.name == "Report").unwrap();
    let imported = report(&mut wb, sheet);

    let mut built = wb.clone();
    let model = PivotTable {
        name: "SalesPivot".into(),
        source: "Sheet1!$A$1:$C$7".into(),
        rows: vec![PivotField { source_col: "Region".into(), ..PivotField::default() }],
        values: vec![PivotValue { source_col: "Sales".into(), func: PivotFunc::Sum, name: "Sum of Sales".into(), ..PivotValue::default() }],
        layout: PivotLayout::Tabular,
        ..PivotTable::default()
    };
    Arc::make_mut(&mut built.sheets[sheet]).pivots = vec![model];
    assert_eq!(imported, report(&mut built, sheet));
    let text = |s: &str| Value::text(s);
    let grand = imported.iter().find(|r| r.first() == Some(&text("Grand Total"))).unwrap();
    assert_eq!(grand.get(1), Some(&Value::number(53.0)));
    let east = imported.iter().find(|r| r.first() == Some(&text("East"))).unwrap();
    assert_eq!(east.get(1), Some(&Value::number(23.0)));

    // Editing the source and refreshing updates the report.
    let src = wb.sheets.iter().position(|s| s.name == "Sheet1").unwrap();
    wb.sheet_mut(src).unwrap().cells.set(CellRef::new(1, 2), Cell::value(Value::number(110.0)));
    let after = report(&mut wb, sheet);
    let east = after.iter().find(|r| r.first() == Some(&text("East"))).unwrap();
    assert_eq!(east.get(1), Some(&Value::number(123.0)));
}
