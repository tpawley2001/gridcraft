//! 3-D column, bar, line, area and pie charts: boxes, ribbons, slabs and a tilted pie.

use super::*;
use gridcraft_model::Chart3d;

fn view(rot_x: i16, rot_y: u16) -> Chart3d {
    Chart3d { rot_x: Some(rot_x), rot_y: Some(rot_y), right_angle_axes: Some(false), ..Chart3d::default() }
}

fn polygons(p: &[Prim]) -> usize {
    p.iter().filter(|p| matches!(p, Prim::Polygon { .. })).count()
}

fn polygons_in(p: &[Prim], color: Rgba) -> usize {
    p.iter().filter(|p| matches!(p, Prim::Polygon { fill, .. } if *fill == color)).count()
}

#[test]
fn three_d_column_draws_boxes_instead_of_rectangles() {
    let mut c = chart(ChartKind::ColumnClustered);
    let d = sample(c.kind);
    let flat = render(&c, &d, 640.0, 400.0, &ApproxMeasure);
    c.view_3d = Some(view(15, 20));
    let deep = render(&c, &d, 640.0, 400.0, &ApproxMeasure);
    assert_sane(&deep, 640.0, 400.0);
    assert!(polygons(&deep) > polygons(&flat) + 3 * 18, "{} polygons vs {}", polygons(&deep), polygons(&flat));
    // Each column has a front face in its series colour plus darker top and side faces.
    let north = [20, 96, 130, 255];
    assert_eq!(polygons_in(&deep, north), 6, "one front face per value");
    let flat_rects = flat.iter().filter(|p| matches!(p, Prim::Rect { fill: Some(f), .. } if *f == north)).count();
    let deep_rects = deep.iter().filter(|p| matches!(p, Prim::Rect { fill: Some(f), .. } if *f == north)).count();
    assert!(deep_rects < flat_rects, "columns are no longer flat rectangles");
    assert!(deep.iter().any(|p| matches!(p, Prim::Text { text, .. } if text == "Q3")), "category labels");
}

#[test]
fn series_in_depth_name_each_row_and_draw_back_to_front() {
    let mut c = chart(ChartKind::ColumnClustered);
    c.legend = LegendPos::None;
    c.data_labels = false;
    c.view_3d = Some(Chart3d { series_in_depth: true, depth_percent: Some(100), ..view(15, 20) });
    let d = sample(c.kind);
    let p = render(&c, &d, 640.0, 400.0, &ApproxMeasure);
    assert_sane(&p, 640.0, 400.0);
    for name in ["North", "East", "South"] {
        assert!(p.iter().any(|x| matches!(x, Prim::Text { text, .. } if text == name)), "series axis label {name}");
    }
    // The last series stands at the back, so its faces come first.
    let first = |c: Rgba| p.iter().position(|x| matches!(x, Prim::Polygon { fill, .. } if *fill == c)).unwrap();
    assert!(first([25, 107, 36, 255]) < first([20, 96, 130, 255]));
}

#[test]
fn three_d_bar_line_and_area_draw_inside_the_chart() {
    for kind in [
        ChartKind::BarClustered,
        ChartKind::BarStacked100,
        ChartKind::ColumnStacked,
        ChartKind::Line,
        ChartKind::LineStacked,
        ChartKind::Area,
        ChartKind::AreaStacked,
        ChartKind::AreaStacked100,
    ] {
        let mut c = chart(kind);
        let d = sample(kind);
        let flat = polygons(&render(&c, &d, 640.0, 400.0, &ApproxMeasure));
        for (rx, ry) in [(15, 20), (-30, 300), (0, 0), (90, 180)] {
            c.view_3d = Some(Chart3d { series_in_depth: true, gap_depth: Some(500), depth_percent: Some(2000), ..view(rx, ry) });
            let p = render(&c, &d, 640.0, 400.0, &ApproxMeasure);
            assert_sane(&p, 640.0, 400.0);
            assert!(polygons(&p) > flat, "{kind:?} at {rx},{ry}");
        }
    }
}

#[test]
fn three_d_pie_is_a_tilted_ellipse_inside_the_plot() {
    let mut c = chart(ChartKind::Pie);
    c.legend = LegendPos::None;
    let d = sample(c.kind);
    c.view_3d = Some(view(30, 0));
    let (w, h) = (500.0, 400.0);
    let p = render(&c, &d, w, h, &ApproxMeasure);
    assert_sane(&p, w, h);
    assert!(!p.iter().any(|x| matches!(x, Prim::Wedge { .. })), "a 3-D pie draws polygons, not flat wedges");
    let pts: Vec<[f32; 2]> = p
        .iter()
        .filter_map(|x| match x {
            Prim::Polygon { pts, .. } => Some(pts.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(pts.len() > 50);
    let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    assert!(x0 > 0.0 && x1 < w && y0 > 0.0 && y1 < h, "inside the chart: {x0} {x1} {y0} {y1}");
    assert!(y1 - y0 < 0.75 * (x1 - x0), "tilted: {} tall, {} wide", y1 - y0, x1 - x0);
    // The side band and cut faces are darker shades of the six slice colours.
    let mut fills: Vec<Rgba> = p
        .iter()
        .filter_map(|x| match x {
            Prim::Polygon { fill, .. } => Some(*fill),
            _ => None,
        })
        .collect();
    fills.sort_unstable();
    fills.dedup();
    assert!(fills.len() > 6, "side band shades: {fills:?}");
}

#[test]
fn exploded_3d_pie_pulls_slices_apart_and_stays_inside() {
    let mut c = chart(ChartKind::Pie);
    let d = sample(c.kind);
    c.view_3d = Some(view(75, 0));
    let whole = render(&c, &d, 500.0, 400.0, &ApproxMeasure);
    c.view_3d = Some(view(75, 90));
    if let Some(s) = c.series.first_mut() {
        s.explosion = 400;
    }
    let apart = render(&c, &d, 500.0, 400.0, &ApproxMeasure);
    assert_sane(&apart, 500.0, 400.0);
    assert_ne!(whole, apart);
}

#[test]
fn three_d_charts_survive_empty_and_hostile_data() {
    for kind in [ChartKind::ColumnClustered, ChartKind::BarStacked, ChartKind::Line, ChartKind::Area, ChartKind::Pie] {
        let mut c = chart(kind);
        c.view_3d = Some(Chart3d {
            rot_x: Some(i16::MIN),
            rot_y: Some(u16::MAX),
            depth_percent: Some(u16::MAX),
            gap_depth: Some(u16::MAX),
            ..Chart3d::default()
        });
        let mut d = sample(kind);
        for s in &mut d.series {
            s.values = vec![None, Some(f64::NAN), Some(1e308), Some(-1e308), Some(0.0)];
        }
        for (w, h) in [(640.0, 400.0), (12.0, 9.0), (3000.0, 20.0)] {
            assert_sane(&render(&c, &d, w, h, &ApproxMeasure), w, h);
        }
        d.series.clear();
        assert_sane(&render(&c, &d, 640.0, 400.0, &ApproxMeasure), 640.0, 400.0);
    }
}
