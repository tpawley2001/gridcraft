//! A radar chart's value axis scales like Excel's: no headroom past the data maximum, a 1-2-5 step
//! chosen from the room the rings have, and the chart's own Format Axis bounds and major unit
//! (`c:valAx/c:scaling/c:max`, `c:majorUnit`, ECMA-376 Part 1 §21.2.2) when the file sets them.

use gridcraft_chart::{ApproxMeasure, Prim, radar_scale, render, resolve};
use gridcraft_engine::Session;
use serde_json::json;

/// Excel's printed radar axes: (data min, data max, radius, label height) -> (min, max, step).
/// Radius and label height were measured in points on Excel's own PDF of each chart.
const MEASURED: &[(f64, f64, f32, f32, f64, f64, f64)] = &[
    (1.0, 15.0, 84.0, 10.0, 0.0, 15.0, 5.0),
    (1.0, 5.0, 84.0, 10.0, 0.0, 5.0, 1.0),
    (1.8, 5.0, 89.0, 11.0, 0.0, 5.0, 1.0),
    (1.0, 8.0, 87.0, 9.2, 0.0, 8.0, 1.0),
    (75.0, 145.0, 150.0, 10.0, 0.0, 160.0, 20.0),
];

#[test]
fn radar_scale_matches_excels_measured_axes() {
    for &(lo, hi, radius, label, min, max, step) in MEASURED {
        let s = radar_scale(lo, hi, radius, label);
        assert_eq!((s.min, s.max, s.step), (min, max, step), "data {lo}..{hi}, radius {radius}, label {label}");
    }
}

#[test]
fn radar_scale_has_no_headroom_and_lands_on_the_step_grid() {
    // Data maxima on the grid are the axis maximum; values come out exact.
    let s = radar_scale(0.1, 0.3, 200.0, 9.0);
    assert_eq!(s.min, 0.0, "{s:?}");
    assert!((s.max - 0.3).abs() < 1e-12, "the data maximum 0.3 is the axis maximum: {s:?}");
    assert!(((s.max / s.step) - (s.max / s.step).round()).abs() < 1e-12, "{s:?}");
    // Reversed bounds are accepted.
    assert_eq!(radar_scale(15.0, 1.0, 84.0, 10.0), radar_scale(1.0, 15.0, 84.0, 10.0));
    // All zero: unchanged fallback.
    let z = radar_scale(0.0, 0.0, 100.0, 9.0);
    assert_eq!((z.min, z.max, z.step), (0.0, 1.0, 0.2));
}

#[test]
fn radar_scale_on_a_tiny_axis_uses_one_ring() {
    let s = radar_scale(1.0, 5.0, 0.0, 9.0);
    assert_eq!(s.min, 0.0);
    assert!(s.max >= 5.0 && (s.max - s.min - s.step).abs() < 1e-9, "one step covers the data: {s:?}");
}

/// A radar chart over two series and five categories with values 1..5.
fn radar_chart() -> gridcraft_model::Workbook {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([
        ["Axis", "North", "South"],
        ["Speed", "4", "2"],
        ["Range", "3", "5"],
        ["Cost", "5", "3"],
        ["Comfort", "2", "4"],
        ["Safety", "1", "4"]
    ]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:C6", "type": "radar"})).unwrap();
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    gridcraft_xlsx::read_xlsx(&bytes).unwrap().0
}

/// The numeric texts drawn for the chart (the ring labels; categories are words).
fn ring_labels(wb: &gridcraft_model::Workbook) -> Vec<f64> {
    let chart = wb.sheet(0).unwrap().charts[0].clone();
    let data = resolve(wb, 0, &chart);
    render(&chart, &data, 480.0, 360.0, &ApproxMeasure)
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, .. } => text.parse::<f64>().ok(),
            _ => None,
        })
        .collect()
}

#[test]
fn radar_rings_stop_at_the_data_maximum() {
    let labels = ring_labels(&radar_chart());
    assert!(labels.contains(&0.0), "{labels:?}");
    assert!(labels.iter().all(|v| *v <= 5.0), "no ring past the data maximum 5: {labels:?}");
}

#[test]
fn radar_uses_the_charts_fixed_bounds_and_major_unit() {
    let mut wb = radar_chart();
    {
        let chart = &mut wb.sheet_mut(0).unwrap().charts[0];
        chart.scale.y.max = Some(10.0);
        chart.scale.y.major_unit = Some(2.5);
    }
    let labels = ring_labels(&wb);
    for v in [0.0, 2.5, 5.0, 7.5, 10.0] {
        assert!(labels.contains(&v), "ring {v} labelled: {labels:?}");
    }
    assert!(labels.iter().all(|v| *v <= 10.0 && (v / 2.5).fract() == 0.0), "{labels:?}");
}
