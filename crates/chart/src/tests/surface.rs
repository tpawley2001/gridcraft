//! Surface charts: contour bands and their legend, the projected 3-D surface, wireframes.

use super::*;

/// A 3x3 grid from 0 to 80: series rows (0, 20, 40), (20, 40, 60), (40, 60, 80).
fn grid() -> ChartData {
    let mk = |name: &str, v: &[f64]| SeriesData { number_format: "General".into(), ..series(name, v, [0, 0, 0, 255], ChartKind::Contour) };
    ChartData {
        category_levels: vec![],
        palette: vec![[10, 20, 30, 255], [40, 50, 60, 255], [70, 80, 90, 255], [100, 110, 120, 255], [130, 140, 150, 255]],
        categories: vec!["a".into(), "b".into(), "c".into()],
        series: vec![mk("S1", &[0.0, 20.0, 40.0]), mk("S2", &[20.0, 40.0, 60.0]), mk("S3", &[40.0, 60.0, 80.0])],
    }
}

fn surface_chart(kind: ChartKind) -> Chart {
    let mut c = chart(kind);
    c.scale.y.major_unit = Some(20.0);
    c
}

fn polygons(prims: &[Prim]) -> Vec<(&Vec<[f32; 2]>, Rgba)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Polygon { pts, fill, .. } => Some((pts, *fill)),
            _ => None,
        })
        .collect()
}

#[test]
fn contour_has_one_band_per_major_unit_and_a_band_legend() {
    let c = surface_chart(ChartKind::Contour);
    let d = grid();
    let entries = legend_entries(&c, &d);
    let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(names, ["0-20", "20-40", "40-60", "60-80"]);
    let colors: Vec<Rgba> = entries.iter().map(|e| e.1).collect();
    assert_eq!(colors, d.palette[..4], "band k takes palette colour k");
    let prims = render(&c, &d, 600.0, 400.0, &ApproxMeasure);
    assert_sane(&prims, 600.0, 400.0);
    let fills: std::collections::BTreeSet<Rgba> = polygons(&prims).iter().map(|p| p.1).collect();
    assert_eq!(fills.into_iter().collect::<Vec<_>>(), d.palette[..4], "every band is filled, nothing else");
    let t = texts(&prims);
    for label in ["0-20", "60-80", "a", "c", "S1", "S3"] {
        assert!(t.contains(&label), "{label} missing: {t:?}");
    }
}

#[test]
fn contour_auto_unit_is_the_value_axis_unit() {
    // Without a fixed major unit the bands follow the value axis' automatic unit (20 for 0..80).
    let c = chart(ChartKind::Contour);
    assert_eq!(legend_entries(&c, &grid()).len(), 4);
}

#[test]
fn contour_bands_tile_the_plot() {
    // The band pieces of every grid triangle cover the plot exactly once.
    let c = surface_chart(ChartKind::Contour);
    let prims = render(&c, &grid(), 600.0, 400.0, &ApproxMeasure);
    let area = |pts: &Vec<[f32; 2]>| {
        let n = pts.len();
        (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f32>().abs() / 2.0
    };
    let total: f32 = polygons(&prims).iter().map(|p| area(p.0)).sum();
    let frame = prims
        .iter()
        .find_map(|p| match p {
            Prim::Rect { w, h, fill: None, .. } => Some(w * h),
            _ => None,
        })
        .unwrap();
    assert!((total - frame).abs() < frame * 0.01, "bands {total} vs plot {frame}");
}

#[test]
fn wireframe_contour_draws_iso_lines_and_no_fill() {
    let c = surface_chart(ChartKind::ContourWireframe);
    let d = grid();
    let prims = render(&c, &d, 600.0, 400.0, &ApproxMeasure);
    assert_sane(&prims, 600.0, 400.0);
    assert!(polygons(&prims).is_empty(), "a wireframe fills nothing");
    let iso = prims.iter().filter(|p| matches!(p, Prim::Line { color, .. } if d.palette[..4].contains(color))).count();
    assert!(iso >= 3, "iso-lines at 20, 40 and 60: {iso}");
    assert_eq!(legend_entries(&c, &d).len(), 4);
}

#[test]
fn surface_3d_renders_inside_the_plot() {
    let c = surface_chart(ChartKind::Surface);
    let d = grid();
    let (w, h) = (640.0, 420.0);
    let prims = render(&c, &d, w, h, &ApproxMeasure);
    assert_sane(&prims, w, h);
    let quads: Vec<_> = polygons(&prims).into_iter().filter(|p| d.palette.contains(&p.1)).collect();
    assert_eq!(quads.len(), 4, "a 3x3 grid has 2x2 cells");
    // The first category sits left of the last along the front edge; the value labels are on the left.
    let t = texts(&prims);
    for label in ["a", "c", "S1", "S3", "0", "80"] {
        assert!(t.contains(&label), "{label} missing: {t:?}");
    }
    let x_of = |s: &str| {
        prims.iter().find_map(|p| match p {
            Prim::Text { text, x, .. } if text == s => Some(*x),
            _ => None,
        })
    };
    assert!(x_of("a").unwrap() < x_of("c").unwrap());
    assert!(x_of("80").unwrap() < x_of("a").unwrap(), "the value axis is on the left");
    assert!(x_of("S3").unwrap() > x_of("c").unwrap(), "the series axis recedes to the right");
    assert_eq!(legend_entries(&c, &d).len(), 4);
}

#[test]
fn wireframe_surface_draws_no_filled_quads() {
    let c = surface_chart(ChartKind::SurfaceWireframe);
    let d = grid();
    let prims = render(&c, &d, 640.0, 420.0, &ApproxMeasure);
    assert_sane(&prims, 640.0, 420.0);
    assert!(polygons(&prims).iter().all(|p| !d.palette.contains(&p.1)), "only the walls are filled");
    let outlines = prims.iter().filter(|p| matches!(p, Prim::Line { pts, color, .. } if pts.len() == 5 && d.palette.contains(color))).count();
    assert_eq!(outlines, 4);
}

#[test]
fn surface_with_gaps_and_huge_grids_stays_bounded() {
    let mut d = grid();
    d.series[1].values[1] = None;
    for kind in [ChartKind::Surface, ChartKind::Contour, ChartKind::ContourWireframe] {
        let prims = render(&chart(kind), &d, 500.0, 300.0, &ApproxMeasure);
        assert_sane(&prims, 500.0, 300.0);
    }
    let big = ChartData {
        series: (0..255)
            .map(|i| series(&format!("s{i}"), &(0..5000).map(|j| ((i * j) % 97) as f64).collect::<Vec<_>>(), [0; 4], ChartKind::Surface))
            .collect(),
        ..ChartData::default()
    };
    for kind in [ChartKind::Surface, ChartKind::Contour, ChartKind::ContourWireframe] {
        let prims = render(&chart(kind), &big, 800.0, 600.0, &ApproxMeasure);
        assert_sane(&prims, 800.0, 600.0);
        assert!(prims.len() < 400_000, "{kind:?}: {}", prims.len());
    }
}
