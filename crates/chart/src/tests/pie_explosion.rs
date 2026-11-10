//! Exploded pie and doughnut slices are drawn pulled out along their bisector, and the pie
//! shrinks so the farthest slice still fits where the closed pie did.

use super::*;

/// `(cx, cy, r_outer, a0, a1)` of every wedge, in drawing order.
fn wedges(c: &Chart, d: &ChartData) -> Vec<(f32, f32, f32, f32, f32)> {
    render(c, d, 400.0, 400.0, &ApproxMeasure)
        .iter()
        .filter_map(|p| match p {
            Prim::Wedge { cx, cy, r_outer, a0, a1, .. } => Some((*cx, *cy, *r_outer, *a0, *a1)),
            _ => None,
        })
        .collect()
}

fn pie_chart(kind: ChartKind, explosion: u32, points: Vec<(u32, u32)>) -> (Chart, ChartData) {
    let mut c = chart(kind);
    c.title = None;
    c.legend = LegendPos::None;
    c.data_labels = false;
    c.first_slice_angle = 0;
    c.series = vec![Series { explosion, point_explosions: points, ..Series::default() }];
    let d = ChartData {
        category_levels: vec![],
        palette: vec![],
        categories: ["A", "B", "C", "D"].iter().map(|s| s.to_string()).collect(),
        series: vec![series("Only", &[1.0, 2.0, 3.0, 4.0], [20, 96, 130, 255], kind)],
    };
    (c, d)
}

/// The box around every slice: its centre and points along its outer arc.
fn extent(w: &[(f32, f32, f32, f32, f32)]) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (cx, cy, r, a0, a1) in w {
        for k in 0..=64 {
            let a = a0 + (a1 - a0) * k as f32 / 64.0;
            for (x, y) in [(*cx, *cy), (cx + r * a.sin(), cy - r * a.cos())] {
                b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
    }
    b
}

#[test]
fn one_exploded_slice_is_pulled_out_and_the_pie_still_fits() {
    let (c, d) = pie_chart(ChartKind::Pie, 0, vec![]);
    let closed = wedges(&c, &d);
    assert_eq!(closed.len(), 4);
    let (cx, cy) = (closed[0].0, closed[0].1);
    assert!(closed.iter().all(|w| w.0 == cx && w.1 == cy), "a closed pie shares one centre: {closed:?}");
    let room = extent(&closed);

    let (c, d) = pie_chart(ChartKind::Pie, 0, vec![(1, 40)]);
    let open = wedges(&c, &d);
    assert_eq!(open.len(), 4);
    let dist = |w: &(f32, f32, f32, f32, f32)| ((w.0 - cx).powi(2) + (w.1 - cy).powi(2)).sqrt();
    assert!(dist(&open[1]) > 1.0, "slice 1 is pulled out: {open:?}");
    for k in [0, 2, 3] {
        assert!(dist(&open[k]) < 1e-3, "slice {k} stays put: {open:?}");
    }
    // Along its bisector, by 40% of the (shrunken) radius.
    let mid = (open[1].3 + open[1].4) / 2.0;
    assert!((open[1].0 - (cx + 0.4 * open[1].2 * mid.sin())).abs() < 1e-2, "{open:?}");
    assert!((open[1].1 - (cy - 0.4 * open[1].2 * mid.cos())).abs() < 1e-2, "{open:?}");
    assert!(open[1].2 < closed[1].2, "the pie shrinks to make room");
    let (x0, y0, x1, y1) = extent(&open);
    assert!(x0 >= room.0 - 0.5 && y0 >= room.1 - 0.5 && x1 <= room.2 + 0.5 && y1 <= room.3 + 0.5, "{open:?} in {room:?}");
}

#[test]
fn exploded_pie_keeps_every_slice_inside_the_plot() {
    let (c, d) = pie_chart(ChartKind::Pie, 0, vec![]);
    let room = extent(&wedges(&c, &d));
    // A series explosion, a point override, and a hostile value past the maximum.
    for (e, points) in [(25, vec![]), (10, vec![(2, 25)]), (u32::MAX, vec![(0, u32::MAX)])] {
        let (c, d) = pie_chart(ChartKind::Pie, e, points);
        let w = wedges(&c, &d);
        assert_eq!(w.len(), 4);
        assert!(w.iter().all(|w| w.0.is_finite() && w.1.is_finite() && w.2 >= 2.0), "{w:?}");
        let (x0, y0, x1, y1) = extent(&w);
        assert!(x0 >= room.0 - 0.5 && y0 >= room.1 - 0.5 && x1 <= room.2 + 0.5 && y1 <= room.3 + 0.5, "{w:?} in {room:?}");
    }
}

#[test]
fn doughnut_explodes_its_outer_ring() {
    let (mut c, mut d) = pie_chart(ChartKind::Doughnut, 0, vec![]);
    c.hole_size = 50;
    d.series.insert(0, series("Inner", &[1.0, 1.0, 1.0, 1.0], [221, 132, 82, 255], ChartKind::Doughnut));
    c.series = vec![Series::default(), Series::default()];
    let room = extent(&wedges(&c, &d));
    c.series[1].explosion = 25;
    let w = wedges(&c, &d);
    assert_eq!(w.len(), 8, "{w:?}");
    let (cx, cy) = (w[0].0, w[0].1);
    assert!(w[..4].iter().all(|s| s.0 == cx && s.1 == cy), "the inner ring stays closed: {w:?}");
    assert!(w[4..].iter().all(|s| ((s.0 - cx).powi(2) + (s.1 - cy).powi(2)).sqrt() > 1.0), "the outer ring is pulled out: {w:?}");
    let (x0, y0, x1, y1) = extent(&w);
    assert!(x0 >= room.0 - 0.5 && y0 >= room.1 - 0.5 && x1 <= room.2 + 0.5 && y1 <= room.3 + 0.5, "{w:?} in {room:?}");
}
