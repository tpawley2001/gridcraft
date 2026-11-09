//! Analysis ToolPak Regression plots: Residual Plots, Line Fit Plots and Normal Probability Plots
//! add XY scatter charts beside the report whose series reference the input and report cells.

use std::collections::HashSet;

use gridcraft_engine::Session;
use gridcraft_model::{Chart, ChartKind};
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([["y", "x"], [2, 1], [4, 2], [5, 3], [4, 4], [5, 5]]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

fn regression(s: &mut Session, extra: Json) {
    let mut p = json!({"tool": "regression", "inputY": "A1:A6", "inputX": "B1:B6", "labels": true, "output": "D1"});
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    s.execute("data.analysisToolPak", p).unwrap();
}

fn charts(s: &Session) -> Vec<Chart> {
    s.doc().unwrap().wb.sheet(0).unwrap().charts.clone()
}

fn cell_text(s: &Session, a1: &str) -> String {
    let c = gridcraft_core::CellRef::parse(a1).unwrap();
    s.doc().unwrap().wb.sheet(0).unwrap().cell(c).and_then(|c| c.value.as_text().map(str::to_owned)).unwrap_or_default()
}

// Report at D1 (k = 1): RESIDUAL OUTPUT at D22, headers D24:F24, data D25:F29 (Observation,
// Predicted y, Residuals); PROBABILITY OUTPUT at H22, data H25:I29 (Percentile, y).

#[test]
fn no_plot_options_add_no_chart() {
    let mut s = session();
    regression(&mut s, json!({"residuals": true}));
    assert!(charts(&s).is_empty());
}

#[test]
fn residual_plot_scatter_of_x_against_residuals() {
    let mut s = session();
    regression(&mut s, json!({"residualPlots": true}));
    // The residual table is written even without `residuals`.
    assert_eq!(cell_text(&s, "D22"), "RESIDUAL OUTPUT");
    assert_eq!(cell_text(&s, "F24"), "Residuals");
    let c = charts(&s);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    assert_eq!(ch.kind, ChartKind::Scatter);
    assert_eq!(ch.title.as_deref(), Some("x Residual Plot"));
    assert_eq!(ch.x_title.as_deref(), Some("x"));
    assert_eq!(ch.y_title.as_deref(), Some("Residuals"));
    assert_eq!(ch.series.len(), 1);
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$B$2:$B$6"));
    assert_eq!(ch.series[0].values, "Sheet1!$F$25:$F$29");
    // Right of the report (SUMMARY OUTPUT spans D:L).
    assert!(ch.anchor.cell.col >= 12, "{:?}", ch.anchor.cell);
}

#[test]
fn line_fit_plot_shows_y_and_predicted_y() {
    let mut s = session();
    regression(&mut s, json!({"lineFitPlots": true}));
    let c = charts(&s);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    assert_eq!(ch.kind, ChartKind::Scatter);
    assert_eq!(ch.title.as_deref(), Some("x Line Fit Plot"));
    assert_eq!(ch.x_title.as_deref(), Some("x"));
    assert_eq!(ch.y_title.as_deref(), Some("y"));
    let names: Vec<_> = ch.series.iter().map(|s| s.name.clone().unwrap_or_default()).collect();
    assert_eq!(names, ["y", "Predicted y"]);
    assert_eq!(ch.series[0].values, "Sheet1!$A$2:$A$6");
    assert_eq!(ch.series[1].values, "Sheet1!$E$25:$E$29");
    for se in &ch.series {
        assert_eq!(se.categories.as_deref(), Some("Sheet1!$B$2:$B$6"));
    }
}

#[test]
fn normal_probability_plot_charts_the_probability_table() {
    let mut s = session();
    regression(&mut s, json!({"residuals": true, "normalProbability": true}));
    assert_eq!(cell_text(&s, "H22"), "PROBABILITY OUTPUT");
    let c = charts(&s);
    assert_eq!(c.len(), 1);
    let ch = &c[0];
    assert_eq!(ch.kind, ChartKind::Scatter);
    assert_eq!(ch.title.as_deref(), Some("Normal Probability Plot"));
    assert_eq!(ch.x_title.as_deref(), Some("Sample Percentile"));
    assert_eq!(ch.y_title.as_deref(), Some("y"));
    assert_eq!(ch.series.len(), 1);
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$H$25:$H$29"));
    assert_eq!(ch.series[0].values, "Sheet1!$I$25:$I$29");
}

#[test]
fn all_plots_per_x_variable_in_order_without_overlap_and_one_undo() {
    let mut s = session();
    s.execute("range.setValues", json!({"range": "C1", "values": [["z"], [3], [1], [4], [1], [5]]})).unwrap();
    s.execute(
        "data.analysisToolPak",
        json!({"tool": "regression", "inputY": "A1:A6", "inputX": "B1:C6", "labels": true, "output": "E1",
               "residualPlots": true, "lineFitPlots": true, "normalProbability": true}),
    )
    .unwrap();
    let c = charts(&s);
    let titles: Vec<_> = c.iter().map(|ch| ch.title.clone().unwrap_or_default()).collect();
    assert_eq!(titles, ["x Residual Plot", "z Residual Plot", "x Line Fit Plot", "z Line Fit Plot", "Normal Probability Plot"]);
    assert_eq!(c[1].series[0].categories.as_deref(), Some("Sheet1!$C$2:$C$6"));
    let ids: HashSet<_> = c.iter().map(|ch| ch.id).collect();
    assert_eq!(ids.len(), c.len(), "chart ids are unique");
    let mut rows: Vec<_> = c.iter().map(|ch| ch.anchor.cell.row).collect();
    rows.sort_unstable();
    rows.dedup();
    assert_eq!(rows.len(), c.len(), "charts are stacked, not on top of each other");
    s.execute("edit.undo", json!({})).unwrap();
    assert!(charts(&s).is_empty(), "report and charts are one undo step");
}
