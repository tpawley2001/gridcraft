//! Analysis ToolPak "Chart Output": Histogram, Moving Average and Exponential Smoothing place an
//! embedded chart beside the report whose series reference the report cells, like Excel.

use gridcraft_core::CellRef;
use gridcraft_engine::Session;
use gridcraft_model::{Chart, ChartKind, LegendPos};
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([["Score", "Bins"], [1, 2], [2, 4], [2, 6], [3, null], [3, null], [3, null], [4, null], [4, null], [5, null], [9, null]]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

fn charts(s: &Session, sheet: usize) -> Vec<Chart> {
    s.doc().unwrap().wb.sheet(sheet).unwrap().charts.clone()
}

fn sheet_index(s: &Session, name: &str) -> usize {
    s.doc().unwrap().wb.sheets.iter().position(|sh| sh.name == name).unwrap()
}

#[test]
fn histogram_without_chart_output_adds_no_chart() {
    let mut s = session();
    s.execute("data.analysisToolPak", json!({"tool": "histogram", "range": "A1:A11", "bins": "B1:B4", "labels": true, "output": "D1"})).unwrap();
    assert!(charts(&s, 0).is_empty());
}

#[test]
fn histogram_chart_output_column_chart_beside_report() {
    let mut s = session();
    s.execute(
        "data.analysisToolPak",
        json!({"tool": "histogram", "range": "A1:A11", "bins": "B1:B4", "labels": true, "output": "D1", "chartOutput": true}),
    )
    .unwrap();
    let c = charts(&s, 0);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    assert_eq!(ch.kind, ChartKind::ColumnClustered);
    assert_eq!(ch.title.as_deref(), Some("Histogram"));
    assert_eq!(ch.x_title.as_deref(), Some("Bin"));
    assert_eq!(ch.y_title.as_deref(), Some("Frequency"));
    assert_eq!(ch.legend, LegendPos::Right);
    // Report D1:E5 (Bin, Frequency; 2, 4, 6, More): the chart starts at F1.
    assert_eq!(ch.anchor.cell, CellRef::parse("F1").unwrap());
    assert_eq!(ch.series.len(), 1);
    assert_eq!(ch.series[0].name.as_deref(), Some("Frequency"));
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$D$2:$D$5"));
    assert_eq!(ch.series[0].values, "Sheet1!$E$2:$E$5");
}

#[test]
fn histogram_cumulative_and_pareto_chart_the_sorted_table() {
    let mut s = session();
    s.execute(
        "data.analysisToolPak",
        json!({"tool": "histogram", "range": "A1:A11", "bins": "B1:B4", "labels": true, "output": "D1", "cumulative": true, "pareto": true, "chartOutput": true}),
    )
    .unwrap();
    let c = charts(&s, 0);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    // Plain table D:F, sorted (Pareto) table G:I; the chart plots G:I and starts at J1.
    assert_eq!(ch.anchor.cell, CellRef::parse("J1").unwrap());
    assert_eq!(ch.series.len(), 2);
    assert_eq!(ch.series[0].name.as_deref(), Some("Frequency"));
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$G$2:$G$5"));
    assert_eq!(ch.series[0].values, "Sheet1!$H$2:$H$5");
    assert_eq!(ch.series[1].name.as_deref(), Some("Cumulative %"));
    assert_eq!(ch.series[1].values, "Sheet1!$I$2:$I$5");
    assert_eq!(ch.series[1].kind, Some(ChartKind::Line));
    assert!(ch.series[1].secondary);
    assert!(!ch.series[0].secondary);
}

#[test]
fn moving_average_chart_output_line_chart_actual_and_forecast() {
    let mut s = session();
    s.execute(
        "data.analysisToolPak",
        json!({"tool": "movingAverage", "range": "A1:A11", "labels": true, "interval": 3, "output": "C1", "chartOutput": true}),
    )
    .unwrap();
    let c = charts(&s, 0);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    assert_eq!(ch.kind, ChartKind::Line);
    assert_eq!(ch.title.as_deref(), Some("Moving Average"));
    assert_eq!(ch.x_title.as_deref(), Some("Data Point"));
    assert_eq!(ch.y_title.as_deref(), Some("Value"));
    // Forecasts C1:C10 (one per data point): the chart starts at D1.
    assert_eq!(ch.anchor.cell, CellRef::parse("D1").unwrap());
    assert_eq!(ch.series.len(), 2);
    assert_eq!(ch.series[0].name.as_deref(), Some("Actual"));
    assert_eq!(ch.series[0].values, "Sheet1!$A$2:$A$11");
    assert_eq!(ch.series[1].name.as_deref(), Some("Forecast"));
    assert_eq!(ch.series[1].values, "Sheet1!$C$1:$C$10");
    assert!(ch.series.iter().all(|x| x.categories.is_none()));
}

#[test]
fn moving_average_standard_errors_move_the_chart() {
    let mut s = session();
    s.execute(
        "data.analysisToolPak",
        json!({"tool": "movingAverage", "range": "A2:A11", "interval": 3, "output": "C1", "standardErrors": true, "chartOutput": true}),
    )
    .unwrap();
    let ch = charts(&s, 0).remove(0);
    assert_eq!(ch.anchor.cell, CellRef::parse("E1").unwrap());
    assert_eq!(ch.series.len(), 2);
    assert_eq!(ch.series[1].values, "Sheet1!$C$1:$C$10");
}

#[test]
fn exponential_smoothing_on_new_sheet_and_one_undo_step() {
    let mut s = session();
    s.execute(
        "data.analysisToolPak",
        json!({"tool": "exponentialSmoothing", "range": "Sheet1!A2:A11", "damping": 0.3, "newSheet": "Smooth Out", "chartOutput": true}),
    )
    .unwrap();
    let si = sheet_index(&s, "Smooth Out");
    assert!(charts(&s, 0).is_empty());
    let c = charts(&s, si);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    assert_eq!(ch.kind, ChartKind::Line);
    assert_eq!(ch.title.as_deref(), Some("Exponential Smoothing"));
    assert_eq!(ch.series[0].name.as_deref(), Some("Actual"));
    assert_eq!(ch.series[0].values, "Sheet1!$A$2:$A$11");
    assert_eq!(ch.series[1].name.as_deref(), Some("Forecast"));
    assert_eq!(ch.series[1].values, "'Smooth Out'!$A$1:$A$10");
    assert_eq!(ch.anchor.cell, CellRef::parse("B1").unwrap());
    // Report, chart and new sheet go away in one undo step.
    let n = s.doc().unwrap().wb.sheets.len();
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.doc().unwrap().wb.sheets.len(), n - 1);
}

#[test]
fn undo_removes_chart_and_report_together() {
    let mut s = session();
    s.execute("data.analysisToolPak", json!({"tool": "exponentialSmoothing", "range": "A2:A11", "output": "C1", "chartOutput": true})).unwrap();
    assert_eq!(charts(&s, 0).len(), 1);
    s.execute("edit.undo", json!({})).unwrap();
    assert!(charts(&s, 0).is_empty());
    let c1 = s.doc().unwrap().wb.sheet(0).unwrap().value(CellRef::parse("C2").unwrap());
    assert!(c1.is_empty(), "{c1:?}");
}
