//! Pie of pie and bar of pie: the split, the combined slice, the second plot and the series lines.

use gridcraft_model::{OfPie, OfPieSplit};

use super::*;

const TAU: f32 = std::f32::consts::TAU;
const PALETTE: [Rgba; 7] =
    [[10, 20, 30, 255], [40, 50, 60, 255], [70, 80, 90, 255], [100, 110, 120, 255], [130, 140, 150, 255], [160, 170, 180, 255], [190, 200, 210, 255]];

fn of_pie_chart(kind: ChartKind, of_pie: Option<OfPie>) -> Chart {
    let mut c = chart(kind);
    c.title = None;
    c.legend = LegendPos::None;
    c.data_labels = false;
    c.of_pie = of_pie;
    c
}

fn data(values: &[f64]) -> ChartData {
    ChartData {
        category_levels: vec![],
        palette: PALETTE.to_vec(),
        categories: (1..=values.len()).map(|i| format!("P{i}")).collect(),
        series: vec![
            series("Share", values, PALETTE[0], ChartKind::Pie),
            // A second series is ignored, as in a pie.
            series("Other", &[5.0; 6], [1, 2, 3, 255], ChartKind::Pie),
        ],
    }
}

/// Total swept angle of the wedges filled with `fill`, and their centre.
fn sweep(prims: &[Prim], fill: Rgba) -> (f32, Option<(f32, f32)>) {
    let mut total = 0.0;
    let mut centre = None;
    for p in prims {
        if let Prim::Wedge { cx, cy, a0, a1, fill: f, .. } = p
            && *f == fill
        {
            total += a1 - a0;
            centre = Some((*cx, *cy));
        }
    }
    (total, centre)
}

fn wedges(prims: &[Prim]) -> Vec<(f32, f32, f32, f32, f32, Rgba)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Wedge { cx, cy, r_outer, a0, a1, fill, .. } => Some((*cx, *cy, *r_outer, *a0, *a1, *fill)),
            _ => None,
        })
        .collect()
}

fn rects(prims: &[Prim]) -> Vec<(f32, f32, f32, f32, Rgba)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Rect { x, y, w, h, fill: Some(f), .. } if *f != [0xFF, 0xFF, 0xFF, 0xFF] => Some((*x, *y, *w, *h, *f)),
            _ => None,
        })
        .collect()
}

const VALUES: [f64; 6] = [8.2, 3.2, 1.4, 1.2, 0.9, 0.4];

#[test]
fn bar_of_pie_six_points_moves_the_last_two_to_the_bar() {
    let prims = render(&of_pie_chart(ChartKind::BarOfPie, None), &data(&VALUES), 600.0, 300.0, &ApproxMeasure);
    let total = 15.3_f32;
    // The four points that stay keep their share of the circle and their own colours.
    for (j, v) in VALUES.iter().take(4).enumerate() {
        let (a, _) = sweep(&prims, PALETTE[j]);
        assert!((a - *v as f32 / total * TAU).abs() < 1e-3, "point {j}: {a}");
    }
    // The combined slice is 1.3 / 15.3 of the circle, the next palette colour, centred at 3 o'clock.
    let combined: Vec<_> = wedges(&prims).into_iter().filter(|w| w.5 == PALETTE[6]).collect();
    assert_eq!(combined.len(), 1, "{combined:?}");
    let (cx, cy, r, a0, a1, _) = combined[0];
    assert!((a1 - a0 - 1.3 / total * TAU).abs() < 1e-3, "{a0} {a1}");
    assert!(((a0 + a1) / 2.0 - TAU / 4.0).abs() < 1e-3, "centre at 3 o'clock: {a0} {a1}");
    assert!(cx < 300.0, "the pie sits on the left: {cx}");
    // The moved points are not in the pie; they are the bar's two segments, 0.9 above 0.4.
    assert_eq!(sweep(&prims, PALETTE[4]).0, 0.0);
    let bar = rects(&prims);
    assert_eq!(bar.len(), 2, "{bar:?}");
    let (top, bottom) = (bar[0], bar[1]);
    assert_eq!((top.4, bottom.4), (PALETTE[4], PALETTE[5]));
    assert!(top.1 < bottom.1, "the first moved point is on top");
    assert!((top.3 / bottom.3 - 0.9 / 0.4).abs() < 1e-3, "{top:?} {bottom:?}");
    // 75% of the pie's diameter, centred on the pie, right of a gap of one radius (gap width 100).
    let h = top.3 + bottom.3;
    assert!((h - 0.75 * 2.0 * r).abs() < 0.01, "bar height {h}, radius {r}");
    assert!(((top.1 + h / 2.0) - cy).abs() < 0.01);
    assert!((top.0 - (cx + 2.0 * r)).abs() < 0.01, "bar at {}, pie edge at {}", top.0, cx + r);
    // Two series lines join the combined slice's outer corners to the bar's left corners.
    let lines: Vec<&Vec<[f32; 2]>> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { pts, width, .. } if *width < 1.0 => Some(pts),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    let at = |a: f32| [cx + r * a.sin(), cy - r * a.cos()];
    let near = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 0.01 && (p[1] - q[1]).abs() < 0.01;
    assert!(near(lines[0][0], at(a0)) && near(lines[0][1], [top.0, top.1]), "{:?}", lines[0]);
    assert!(near(lines[1][0], at(a1)) && near(lines[1][1], [top.0, top.1 + h]), "{:?}", lines[1]);
}

#[test]
fn auto_split_takes_the_last_third_rounded_up() {
    let values = [8.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
    let mut d = data(&values);
    d.palette.push([220, 230, 240, 255]);
    let prims = render(&of_pie_chart(ChartKind::BarOfPie, None), &d, 600.0, 300.0, &ApproxMeasure);
    let fills: Vec<Rgba> = rects(&prims).into_iter().map(|r| r.4).collect();
    assert_eq!(fills, [PALETTE[4], PALETTE[5], PALETTE[6]]);
}

#[test]
fn pie_of_pie_draws_the_moved_points_as_a_second_pie() {
    let o = OfPie { second_size: 50, series_lines: false, ..OfPie::default() };
    let prims = render(&of_pie_chart(ChartKind::PieOfPie, Some(o)), &data(&VALUES), 600.0, 300.0, &ApproxMeasure);
    let (first, c1) = sweep(&prims, PALETTE[0]);
    let (moved, c2) = sweep(&prims, PALETTE[4]);
    let (last, _) = sweep(&prims, PALETTE[5]);
    assert!((first - 8.2 / 15.3 * TAU).abs() < 1e-3);
    assert!((moved - 0.9 / 1.3 * TAU).abs() < 1e-3 && (last - 0.4 / 1.3 * TAU).abs() < 1e-3, "{moved} {last}");
    let (Some(c1), Some(c2)) = (c1, c2) else { panic!("both pies are drawn") };
    assert!(c2.0 > c1.0 && (c1.1 - c2.1).abs() < 0.01, "{c1:?} {c2:?}");
    let radius = |c: (f32, f32)| wedges(&prims).into_iter().find(|w| (w.0, w.1) == c).map(|w| w.2).unwrap_or(0.0);
    assert!((radius(c2) - 0.5 * radius(c1)).abs() < 0.01);
    assert!(!prims.iter().any(|p| matches!(p, Prim::Line { width, .. } if *width < 1.0)), "series lines are off");
}

#[test]
fn split_by_position_value_percent_and_custom() {
    let cases = [
        (OfPieSplit::Position(3.0), vec![PALETTE[3], PALETTE[4], PALETTE[5]]),
        (OfPieSplit::Value(1.3), vec![PALETTE[3], PALETTE[4], PALETTE[5]]),
        (OfPieSplit::Percent(10.0), vec![PALETTE[2], PALETTE[3], PALETTE[4], PALETTE[5]]),
        (OfPieSplit::Custom(vec![1, 4, 99]), vec![PALETTE[1], PALETTE[4]]),
        (OfPieSplit::Position(f64::NAN), vec![]),
    ];
    for (split, want) in cases {
        let o = OfPie { split: split.clone(), ..OfPie::default() };
        let prims = render(&of_pie_chart(ChartKind::BarOfPie, Some(o)), &data(&VALUES), 600.0, 300.0, &ApproxMeasure);
        let fills: Vec<Rgba> = rects(&prims).into_iter().map(|r| r.4).collect();
        assert_eq!(fills, want, "{split:?}");
    }
}

#[test]
fn every_point_moved_or_degenerate_settings_do_not_crash() {
    for split in [OfPieSplit::Position(1e300), OfPieSplit::Value(f64::INFINITY), OfPieSplit::Percent(-5.0), OfPieSplit::Custom(vec![u32::MAX])] {
        for kind in [ChartKind::PieOfPie, ChartKind::BarOfPie] {
            let o = OfPie { split: split.clone(), gap_width: u16::MAX, second_size: u16::MAX, series_lines: true };
            for (w, h) in [(600.0, 300.0), (3.0, 3.0)] {
                let _ = render(&of_pie_chart(kind, Some(o.clone())), &data(&VALUES), w, h, &ApproxMeasure);
            }
            let _ = render(&of_pie_chart(kind, Some(o.clone())), &data(&[0.0, 0.0]), 600.0, 300.0, &ApproxMeasure);
        }
    }
}
