//! Sparkline group settings as Excel draws them: High / Low / First / Last / Negative points,
//! marker colour, the horizontal axis, empty-cell modes, fixed axis limits, right-to-left and
//! line weight.

use gridcraft_model::{SparklineEmpty, SparklineKind};

use crate::{Prim, Rgba, SparklineStyle, render_sparkline_styled};

const SERIES: Rgba = [10, 20, 30, 255];
const HIGH: Rgba = [0, 200, 0, 255];
const LOW: Rgba = [200, 0, 0, 255];
const FIRST: Rgba = [0, 0, 200, 255];
const LAST: Rgba = [200, 200, 0, 255];
const NEG: Rgba = [200, 0, 200, 255];
const MARK: Rgba = [0, 200, 200, 255];
const AXIS: Rgba = [90, 90, 90, 255];
const W: f32 = 120.0;
const H: f32 = 20.0;

/// first = index 0, negative = indexes 1 and 3, high = index 2, low = index 3, last = index 5.
const VALUES: [Option<f64>; 6] = [Some(3.0), Some(-2.0), Some(5.0), Some(-4.0), Some(1.0), Some(2.0)];

fn style() -> SparklineStyle {
    SparklineStyle::new(SERIES, false)
}

fn draw(kind: SparklineKind, values: &[Option<f64>], style: &SparklineStyle) -> Vec<Prim> {
    render_sparkline_styled(kind, values, style, W, H)
}

/// The series line's points (the only line in the series colour).
fn series_points(prims: &[Prim]) -> Vec<[f32; 2]> {
    let lines: Vec<&Vec<[f32; 2]>> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { pts, color, .. } if *color == SERIES => Some(pts),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 1, "one series line");
    lines.first().map(|p| (*p).clone()).unwrap_or_default()
}

/// Circles as (centre, fill), left to right.
fn circles(prims: &[Prim]) -> Vec<([f32; 2], Option<Rgba>)> {
    let mut out: Vec<([f32; 2], Option<Rgba>)> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Circle { cx, cy, fill, .. } => Some(([*cx, *cy], *fill)),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    out
}

/// Bar fills, left to right.
fn bar_fills(prims: &[Prim]) -> Vec<Option<Rgba>> {
    let mut out: Vec<(f32, Option<Rgba>)> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Rect { x, fill, .. } => Some((*x, *fill)),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, f)| f).collect()
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
}

/// A line sparkline with one point flag on: the circles drawn, checked against the series points.
fn assert_points(set: impl Fn(&mut SparklineStyle), expect: &[(usize, Rgba)]) {
    let mut s = style();
    set(&mut s);
    let prims = draw(SparklineKind::Line, &VALUES, &s);
    let pts = series_points(&prims);
    assert_eq!(pts.len(), 6);
    let got = circles(&prims);
    assert_eq!(got.len(), expect.len(), "circles: {got:?}");
    for ((centre, fill), (index, color)) in got.iter().zip(expect) {
        assert!(pts.get(*index).is_some_and(|p| near(*centre, *p)), "circle {centre:?} is on point {index}");
        assert_eq!(*fill, Some(*color), "point {index}");
    }
}

#[test]
fn a_plain_style_draws_no_point_markers() {
    assert_points(|_| {}, &[]);
}

#[test]
fn the_high_point_gets_its_colour() {
    assert_points(
        |s| {
            s.high = true;
            s.color_high = Some(HIGH);
        },
        &[(2, HIGH)],
    );
}

#[test]
fn the_low_point_gets_its_colour() {
    assert_points(
        |s| {
            s.low = true;
            s.color_low = Some(LOW);
        },
        &[(3, LOW)],
    );
}

#[test]
fn the_first_and_last_points_get_their_colours() {
    assert_points(
        |s| {
            s.first = true;
            s.last = true;
            s.color_first = Some(FIRST);
            s.color_last = Some(LAST);
        },
        &[(0, FIRST), (5, LAST)],
    );
}

#[test]
fn negative_points_get_their_colour() {
    assert_points(
        |s| {
            s.negative = true;
            s.color_negative = Some(NEG);
        },
        &[(1, NEG), (3, NEG)],
    );
}

#[test]
fn markers_use_the_markers_colour_and_the_high_point_wins() {
    assert_points(
        |s| {
            s.markers = true;
            s.color_markers = Some(MARK);
            s.high = true;
            s.color_high = Some(HIGH);
        },
        &[(0, MARK), (1, MARK), (2, HIGH), (3, MARK), (4, MARK), (5, MARK)],
    );
}

#[test]
fn markers_without_a_colour_use_the_series_colour() {
    assert_points(|s| s.markers = true, &[(0, SERIES), (1, SERIES), (2, SERIES), (3, SERIES), (4, SERIES), (5, SERIES)]);
}

#[test]
fn every_point_at_the_maximum_is_a_high_point() {
    let mut s = style();
    s.high = true;
    s.color_high = Some(HIGH);
    let prims = draw(SparklineKind::Line, &[Some(1.0), Some(7.0), Some(2.0), Some(7.0)], &s);
    let pts = series_points(&prims);
    let got = circles(&prims);
    assert_eq!(got.len(), 2);
    assert!(pts.get(1).is_some_and(|p| got.first().is_some_and(|c| near(c.0, *p))));
    assert!(pts.get(3).is_some_and(|p| got.get(1).is_some_and(|c| near(c.0, *p))));
}

#[test]
fn first_and_last_skip_empty_cells() {
    let mut s = style();
    s.first = true;
    s.last = true;
    s.color_first = Some(FIRST);
    s.color_last = Some(LAST);
    let prims = draw(SparklineKind::Line, &[None, Some(1.0), Some(2.0), Some(3.0), None], &s);
    let pts = series_points(&prims);
    assert_eq!(pts.len(), 3);
    let got = circles(&prims);
    assert_eq!(got.len(), 2);
    assert!(pts.first().is_some_and(|p| got.first().is_some_and(|c| near(c.0, *p) && c.1 == Some(FIRST))));
    assert!(pts.get(2).is_some_and(|p| got.get(1).is_some_and(|c| near(c.0, *p) && c.1 == Some(LAST))));
}

#[test]
fn negative_columns_use_the_series_colour_unless_negative_points_is_on() {
    let off = draw(SparklineKind::Column, &VALUES, &style());
    assert_eq!(bar_fills(&off), vec![Some(SERIES); 6]);
    let mut s = style();
    s.negative = true;
    s.color_negative = Some(NEG);
    let on = draw(SparklineKind::Column, &VALUES, &s);
    assert_eq!(bar_fills(&on), vec![Some(SERIES), Some(NEG), Some(SERIES), Some(NEG), Some(SERIES), Some(SERIES)]);
}

#[test]
fn column_high_low_first_and_last_bars_get_their_colours() {
    let mut s = style();
    s.high = true;
    s.color_high = Some(HIGH);
    s.low = true;
    s.color_low = Some(LOW);
    s.first = true;
    s.color_first = Some(FIRST);
    s.last = true;
    s.color_last = Some(LAST);
    let fills = bar_fills(&draw(SparklineKind::Column, &VALUES, &s));
    assert_eq!(fills, vec![Some(FIRST), Some(SERIES), Some(HIGH), Some(LOW), Some(SERIES), Some(LAST)]);
}

#[test]
fn win_loss_losses_get_the_negative_colour() {
    let mut s = style();
    s.negative = true;
    s.color_negative = Some(NEG);
    let fills = bar_fills(&draw(SparklineKind::WinLoss, &[Some(2.0), Some(-3.0), Some(1.0)], &s));
    assert_eq!(fills, vec![Some(SERIES), Some(NEG), Some(SERIES)]);
}

#[test]
fn empty_cells_as_gap_zero_or_span() {
    let values = [Some(1.0), Some(2.0), None, Some(3.0), Some(4.0)];
    let lines = |empty: SparklineEmpty| -> Vec<Vec<[f32; 2]>> {
        let mut s = style();
        s.empty = empty;
        draw(SparklineKind::Line, &values, &s)
            .into_iter()
            .filter_map(|p| match p {
                Prim::Line { pts, color, .. } if color == SERIES => Some(pts),
                _ => None,
            })
            .collect()
    };
    let gap = lines(SparklineEmpty::Gap);
    assert_eq!(gap.iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 2]);
    let span = lines(SparklineEmpty::Span);
    assert_eq!(span.iter().map(Vec::len).collect::<Vec<_>>(), vec![4]);
    let zero = lines(SparklineEmpty::Zero);
    assert_eq!(zero.iter().map(Vec::len).collect::<Vec<_>>(), vec![5]);
    // The empty cell is plotted as 0: the lowest point of the line (y grows downwards).
    let ys: Vec<f32> = zero.first().map(|l| l.iter().map(|p| p[1]).collect()).unwrap_or_default();
    let lowest = ys.get(2).copied().unwrap_or(f32::NAN);
    assert!(ys.iter().enumerate().all(|(i, y)| i == 2 || *y < lowest - 0.5), "{ys:?}");
}

#[test]
fn the_axis_is_drawn_at_zero_only_when_values_go_below_zero() {
    let axis_lines = |kind: SparklineKind, values: &[Option<f64>]| -> Vec<Vec<[f32; 2]>> {
        let mut s = style();
        s.axis = true;
        s.color_axis = Some(AXIS);
        draw(kind, values, &s)
            .into_iter()
            .filter_map(|p| match p {
                Prim::Line { pts, color, .. } if color == AXIS => Some(pts),
                _ => None,
            })
            .collect()
    };
    for kind in [SparklineKind::Line, SparklineKind::Column] {
        let crossing = axis_lines(kind, &VALUES);
        assert_eq!(crossing.len(), 1, "{kind:?}");
        let line = crossing.first().cloned().unwrap_or_default();
        assert_eq!(line.len(), 2, "{kind:?}");
        let (a, b) = (line.first().copied().unwrap_or_default(), line.get(1).copied().unwrap_or_default());
        assert!((a[1] - b[1]).abs() < 0.01, "horizontal: {line:?}");
        assert!((a[0] - b[0]).abs() > W * 0.8, "spans the plot: {line:?}");
        // Zero sits between the lowest (-4) and the highest (5) value: inside the box, below the middle.
        assert!(a[1] > H * 0.5 && a[1] < H, "{line:?}");
        assert!(axis_lines(kind, &[Some(1.0), Some(2.0), Some(3.0)]).is_empty(), "{kind:?}: no value below zero");
    }
    // Without the setting there is no axis line at all.
    let plain = draw(SparklineKind::Line, &VALUES, &style());
    assert_eq!(plain.iter().filter(|p| matches!(p, Prim::Line { .. })).count(), 1);
}

#[test]
fn fixed_axis_limits_set_the_vertical_scale() {
    let values = [Some(1.0), Some(2.0), Some(3.0)];
    let own = series_points(&draw(SparklineKind::Line, &values, &style()));
    let mut s = style();
    s.min = Some(0.0);
    s.max = Some(6.0);
    let fixed = series_points(&draw(SparklineKind::Line, &values, &s));
    let y = |pts: &[[f32; 2]], i: usize| pts.get(i).map(|p| p[1]).unwrap_or(f32::NAN);
    // On its own scale 3 is at the top; between 0 and 6 it is halfway up the box.
    assert!(y(&own, 2) < H * 0.25, "{own:?}");
    assert!((y(&fixed, 2) - H / 2.0).abs() < 0.75, "{fixed:?}");
    // Equal value steps stay equal height steps, each a sixth of the plot.
    let (d1, d2) = (y(&fixed, 0) - y(&fixed, 1), y(&fixed, 1) - y(&fixed, 2));
    assert!((d1 - d2).abs() < 0.01 && d1 > H / 10.0 && d1 < H / 6.0 + 0.01, "{fixed:?}");
}

#[test]
fn right_to_left_draws_the_first_point_at_the_right() {
    let mut s = style();
    s.first = true;
    s.color_first = Some(FIRST);
    let ltr = circles(&draw(SparklineKind::Line, &VALUES, &s));
    s.right_to_left = true;
    let rtl = circles(&draw(SparklineKind::Line, &VALUES, &s));
    assert!(ltr.first().is_some_and(|c| c.0[0] < W * 0.2), "{ltr:?}");
    assert!(rtl.first().is_some_and(|c| c.0[0] > W * 0.8), "{rtl:?}");
    let mut c = style();
    c.negative = true;
    c.color_negative = Some(NEG);
    c.right_to_left = true;
    let fills = bar_fills(&draw(SparklineKind::Column, &VALUES, &c));
    assert_eq!(fills, vec![Some(SERIES), Some(SERIES), Some(NEG), Some(SERIES), Some(NEG), Some(SERIES)]);
}

#[test]
fn the_line_weight_scales_the_line_width() {
    let width = |weight: f64| -> f32 {
        let mut s = style();
        s.line_weight = weight;
        draw(SparklineKind::Line, &VALUES, &s)
            .into_iter()
            .find_map(|p| match p {
                Prim::Line { width, color, .. } if color == SERIES => Some(width),
                _ => None,
            })
            .unwrap_or(f32::NAN)
    };
    assert!((width(0.75) - 1.25).abs() < 0.01);
    assert!((width(1.5) - 2.5).abs() < 0.01);
    assert!((width(2.25) - 3.75).abs() < 0.01);
}
