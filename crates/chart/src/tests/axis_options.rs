//! Axis display units and axis crossing points: divided labels, the units label, the axes moved to
//! where they cross, columns growing from the category axis, and hostile values.

use gridcraft_model::{AxisCrosses, DisplayUnit, DisplayUnits};

use super::*;

const BLUE: Rgba = [20, 96, 130, 255];

fn plain(kind: ChartKind) -> Chart {
    let mut c = chart(kind);
    c.title = None;
    c.x_title = None;
    c.y_title = None;
    c.legend = LegendPos::None;
    c.data_labels = false;
    c
}

fn one_series(kind: ChartKind, values: &[f64]) -> ChartData {
    let mut s = series("S", values, BLUE, kind);
    s.number_format = "General".into();
    ChartData { category_levels: vec![], palette: vec![], categories: (1..=values.len()).map(|i| format!("C{i}")).collect(), series: vec![s] }
}

fn text_at<'a>(prims: &'a [Prim], t: &str) -> (f32, f32, &'a HAlign) {
    prims
        .iter()
        .find_map(|p| match p {
            Prim::Text { text, x, y, align, .. } if text == t => Some((*x, *y, align)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {t:?} in {:?}", texts(prims)))
}

/// `(top, bottom)` of the series' bars.
fn bars(prims: &[Prim]) -> Vec<(f32, f32)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Rect { y, h, fill: Some(f), .. } if *f == BLUE => Some((*y, y + h)),
            _ => None,
        })
        .collect()
}

#[test]
fn display_units_divide_labels_and_show_their_label() {
    for (unit, label) in [
        (DisplayUnit::Thousands, "Thousands"),
        (DisplayUnit::Millions, "Millions"),
        (DisplayUnit::HundredThousands, "x 100000"),
        (DisplayUnit::TenMillions, "x 10000000"),
        (DisplayUnit::Custom(250.0), "x 250"),
    ] {
        for kind in [ChartKind::ColumnClustered, ChartKind::BarClustered, ChartKind::Line, ChartKind::Scatter] {
            let mut c = plain(kind);
            c.scale.y.display_units = Some(DisplayUnits { unit, show_label: true });
            let d = one_series(kind, &[1e7, 2e7, 3e7]);
            let p = render(&c, &d, 480.0, 288.0, &ApproxMeasure);
            assert_sane(&p, 480.0, 288.0);
            assert!(texts(&p).contains(&label), "{kind:?} {unit:?}: {:?}", texts(&p));
            assert!(!texts(&p).iter().any(|t| *t == "30000000" || *t == "35000000"), "{kind:?}: labels not divided: {:?}", texts(&p));
        }
    }
    // Without the label only the numbers change.
    let mut c = plain(ChartKind::ColumnClustered);
    c.scale.y.display_units = Some(DisplayUnits { unit: DisplayUnit::Millions, show_label: false });
    let p = render(&c, &one_series(ChartKind::ColumnClustered, &[1e7, 2e7, 3e7]), 480.0, 288.0, &ApproxMeasure);
    assert!(texts(&p).contains(&"30") && !texts(&p).contains(&"Millions"), "{:?}", texts(&p));
}

#[test]
fn columns_grow_from_the_category_axis_crossing() {
    let mut c = plain(ChartKind::ColumnClustered);
    c.scale.cat_crosses = AxisCrosses::At(2.0);
    let p = render(&c, &one_series(ChartKind::ColumnClustered, &[1.0, 3.0, 4.0]), 480.0, 288.0, &ApproxMeasure);
    let (_, two, _) = text_at(&p, "2");
    let b = bars(&p);
    assert_eq!(b.len(), 3, "{b:?}");
    // Value 1 hangs down from the line at 2; 3 and 4 stand on it.
    assert!((b[0].0 - two).abs() < 0.5 && b[0].1 > two, "{b:?} vs {two}");
    assert!((b[1].1 - two).abs() < 0.5 && (b[2].1 - two).abs() < 0.5, "{b:?} vs {two}");
    // The category labels sit just under that line.
    let (_, c1, _) = text_at(&p, "C1");
    assert!(c1 > two && c1 - two < 6.0, "{c1} vs {two}");
}

#[test]
fn axes_at_the_far_end_put_their_labels_outside() {
    let mut c = plain(ChartKind::ColumnClustered);
    c.scale.cat_crosses = AxisCrosses::Max;
    c.scale.y.crosses = AxisCrosses::Max;
    let p = render(&c, &one_series(ChartKind::ColumnClustered, &[1.0, 3.0, 4.0]), 480.0, 288.0, &ApproxMeasure);
    assert_sane(&p, 480.0, 288.0);
    let (_, c1, _) = text_at(&p, "C1");
    let (x4, y4, align) = text_at(&p, "4");
    let (x0, y0, _) = text_at(&p, "0");
    assert!(c1 < y4 && y4 < y0, "category labels above the plot: {c1} vs {y4}..{y0}");
    let (c3x, ..) = text_at(&p, "C3");
    assert!(x4 > c3x && x0 > c3x && *align == HAlign::Left, "value labels right of the plot");

    // Bars: the category axis on the right, labels right of it; value labels on top.
    let mut c = plain(ChartKind::BarClustered);
    c.scale.cat_crosses = AxisCrosses::Max;
    c.scale.y.crosses = AxisCrosses::Max;
    let p = render(&c, &one_series(ChartKind::BarClustered, &[1.0, 3.0, 4.0]), 480.0, 288.0, &ApproxMeasure);
    assert_sane(&p, 480.0, 288.0);
    let (cx, cy, align) = text_at(&p, "C1");
    let (fx, fy, _) = text_at(&p, "4");
    assert!(*align == HAlign::Left && cx >= fx, "category labels right of the plot: {cx} vs {fx}");
    assert!(fy < cy, "value labels above: {fy} vs {cy}");
}

#[test]
fn a_bar_chart_value_axis_sits_at_its_category() {
    let mut c = plain(ChartKind::BarClustered);
    c.scale.y.crosses = AxisCrosses::At(3.0);
    let p = render(&c, &one_series(ChartKind::BarClustered, &[1.0, 3.0, 4.0, 2.0]), 480.0, 288.0, &ApproxMeasure);
    let (_, zero_y, _) = text_at(&p, "0");
    let (_, c2, _) = text_at(&p, "C2");
    let (_, c3, _) = text_at(&p, "C3");
    // Categories run bottom-up: the axis is between C2 (below) and C3 (above).
    assert!(c3 < zero_y && zero_y < c2, "{c3} < {zero_y} < {c2}");
}

#[test]
fn scatter_axes_cross_at_values() {
    let mut c = plain(ChartKind::Scatter);
    let d = one_series(ChartKind::Scatter, &[1.0, 3.0, 4.0, 2.0]);
    let auto = render(&c, &d, 480.0, 288.0, &ApproxMeasure);
    c.scale.x.crosses = AxisCrosses::At(2.0);
    c.scale.y.crosses = AxisCrosses::At(5.0);
    let p = render(&c, &d, 480.0, 288.0, &ApproxMeasure);
    // The X axis (labels "2", "4", ...) moved up to y = 2; the Y labels moved right to x = 5.
    let (x_auto, ..) = text_at(&auto, "3");
    let (x_moved, ..) = text_at(&p, "3");
    assert!(x_moved > x_auto + 20.0, "Y labels moved right: {x_auto} -> {x_moved}");
    let (_, y_auto, _) = text_at(&auto, "6");
    let (_, y_moved, _) = text_at(&p, "6");
    assert!(y_moved < y_auto - 20.0, "X labels moved up: {y_auto} -> {y_moved}");
}

#[test]
fn hostile_crossings_and_units_never_panic() {
    let crossings = [
        AxisCrosses::At(f64::NAN),
        AxisCrosses::At(1e308),
        AxisCrosses::At(-1e308),
        AxisCrosses::At(f64::INFINITY),
        AxisCrosses::Min,
        AxisCrosses::Max,
    ];
    let units =
        [DisplayUnit::Custom(-5.0), DisplayUnit::Custom(f64::NAN), DisplayUnit::Custom(1e-308), DisplayUnit::Custom(1e308), DisplayUnit::Trillions];
    for kind in ALL_KINDS {
        for (cr, unit) in crossings.iter().zip(units.iter().cycle()) {
            let mut c = plain(kind);
            c.scale.cat_crosses = *cr;
            c.scale.x.crosses = *cr;
            c.scale.y.crosses = *cr;
            c.scale.y.display_units = Some(DisplayUnits { unit: *unit, show_label: true });
            let p = render(&c, &sample(kind), 480.0, 288.0, &ApproxMeasure);
            assert_sane(&p, 480.0, 288.0);
        }
    }
}
