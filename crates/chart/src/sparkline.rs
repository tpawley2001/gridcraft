//! In-cell sparklines.

use std::collections::HashMap;

use gridcraft_model::{Color, Sparkline, SparklineAxis, SparklineEmpty, SparklineKind, Theme};

use crate::render::{DOWN, finalize};
use crate::resolve::MAX_POINTS;
use crate::{Prim, Rgba};

/// Draws a sparkline into a `w`×`h` box at the origin. Gaps (`None`) break lines; column and
/// win/loss sparklines draw negatives in our "down" red.
pub fn render_sparkline(kind: SparklineKind, values: &[Option<f64>], color: Rgba, markers: bool, w: f32, h: f32) -> Vec<Prim> {
    let mut style = SparklineStyle::new(color, markers);
    style.negative = kind != SparklineKind::Line;
    render_sparkline_styled(kind, values, &style, w, h)
}

/// How a sparkline is drawn beyond its type: Excel's sparkline group settings with every colour
/// already resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct SparklineStyle {
    /// The series colour.
    pub color: Rgba,
    /// A marker on every point (line sparklines).
    pub markers: bool,
    pub high: bool,
    pub low: bool,
    pub first: bool,
    pub last: bool,
    pub negative: bool,
    /// Point colours; `None` falls back to the series colour.
    pub color_markers: Option<Rgba>,
    pub color_high: Option<Rgba>,
    pub color_low: Option<Rgba>,
    pub color_first: Option<Rgba>,
    pub color_last: Option<Rgba>,
    pub color_negative: Option<Rgba>,
    /// Show the horizontal axis (drawn only when the data goes below zero).
    pub axis: bool,
    pub color_axis: Option<Rgba>,
    /// How empty cells are plotted.
    pub empty: SparklineEmpty,
    /// Fixed vertical axis limits; `None` = from this sparkline's own values.
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub right_to_left: bool,
    /// Line weight in points (Excel's default is 0.75).
    pub line_weight: f64,
}

impl SparklineStyle {
    /// The plain look: the series colour, optional markers, gaps for empty cells.
    pub fn new(color: Rgba, markers: bool) -> Self {
        SparklineStyle {
            color,
            markers,
            high: false,
            low: false,
            first: false,
            last: false,
            negative: false,
            color_markers: None,
            color_high: None,
            color_low: None,
            color_first: None,
            color_last: None,
            color_negative: None,
            axis: false,
            color_axis: None,
            empty: SparklineEmpty::Gap,
            min: None,
            max: None,
            right_to_left: false,
            line_weight: 0.75,
        }
    }
}

/// A reason for a point to be highlighted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointMark {
    High,
    Low,
    First,
    Last,
    Negative,
    Markers,
}

/// The order in which a point that qualifies for several highlights picks one: the first that
/// applies wins. Kept in this one place so it can follow a measurement of Excel.
fn point_order() -> [PointMark; 6] {
    [PointMark::High, PointMark::Low, PointMark::First, PointMark::Last, PointMark::Negative, PointMark::Markers]
}

/// What the highlights compare against: the data's extremes and the first / last valued points.
struct PointFacts {
    lo: f64,
    hi: f64,
    first: Option<usize>,
    last: Option<usize>,
    line: bool,
}

/// The highlight of point `j` (value `v`) and its colour, if any.
fn point_mark(style: &SparklineStyle, facts: &PointFacts, j: usize, v: f64) -> Option<(PointMark, Rgba)> {
    let series = style.color;
    point_order().into_iter().find_map(|m| {
        let (on, color) = match m {
            PointMark::High => (style.high && v == facts.hi, style.color_high.unwrap_or(series)),
            PointMark::Low => (style.low && v == facts.lo, style.color_low.unwrap_or(series)),
            PointMark::First => (style.first && facts.first == Some(j), style.color_first.unwrap_or(series)),
            PointMark::Last => (style.last && facts.last == Some(j), style.color_last.unwrap_or(series)),
            PointMark::Negative => (style.negative && v < 0.0, style.color_negative.unwrap_or(DOWN)),
            PointMark::Markers => (facts.line && style.markers, style.color_markers.unwrap_or(series)),
        };
        on.then_some((m, color))
    })
}

/// The vertical range to plot: the fixed limits where set (and sound), else the data's own.
fn plot_limits(style: &SparklineStyle, lo: f64, hi: f64) -> (f64, f64) {
    let fixed = |v: Option<f64>| v.filter(|x| x.is_finite()).map(|x| x.clamp(-1e300, 1e300));
    let a = fixed(style.min).unwrap_or(lo);
    let b = fixed(style.max).unwrap_or(hi);
    if a <= b { (a, b) } else { (lo, hi) }
}

/// The series line width for a line weight in points: 1.25 at Excel's default 0.75 pt.
fn line_width(weight: f64) -> f32 {
    let wt = if weight.is_finite() && weight > 0.0 { weight.min(30.0) } else { 0.75 };
    (1.25 * wt / 0.75) as f32
}

/// Draws a sparkline with its group's settings into a `w`×`h` box at the origin.
pub fn render_sparkline_styled(kind: SparklineKind, values: &[Option<f64>], style: &SparklineStyle, w: f32, h: f32) -> Vec<Prim> {
    if !(w.is_finite() && h.is_finite()) || w < 1.0 || h < 1.0 || w > 1e6 || h > 1e6 {
        return vec![];
    }
    let vals: Vec<Option<f64>> = values.iter().take(MAX_POINTS).map(|v| v.filter(|x| x.is_finite()).map(|x| x.clamp(-1e300, 1e300))).collect();
    let n = vals.len();
    if n == 0 {
        return vec![];
    }
    let pad = (h * 0.1).clamp(0.5, 2.0);
    let (x0, y0, iw, ih) = (pad, pad, (w - 2.0 * pad).max(0.5), (h - 2.0 * pad).max(0.5));
    let mut out = Vec::new();
    let (lo, hi) = vals.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    if !lo.is_finite() {
        return out;
    }
    let facts = PointFacts {
        lo,
        hi,
        first: vals.iter().position(Option::is_some),
        last: vals.iter().rposition(Option::is_some),
        line: kind == SparklineKind::Line,
    };
    // Slot of point `j` from the left: right-to-left puts the first point at the right.
    let slot_of = |j: usize| if style.right_to_left { n.saturating_sub(1).saturating_sub(j) } else { j };
    let axis_color = style.color_axis.unwrap_or([0, 0, 0, 255]);
    let axis = |out: &mut Vec<Prim>, plo: f64, phi: f64, y: f32| {
        if style.axis && lo < 0.0 && plo <= 0.0 && 0.0 <= phi {
            out.push(Prim::Line { pts: vec![[x0, y], [x0 + iw, y]], color: axis_color, width: 0.75, dash: false });
        }
    };
    match kind {
        SparklineKind::Line => {
            let zero = style.empty == SparklineEmpty::Zero;
            let has_empty = vals.iter().any(Option::is_none);
            let (dlo, dhi) = if zero && has_empty { (lo.min(0.0), hi.max(0.0)) } else { (lo, hi) };
            let (plo, phi) = plot_limits(style, dlo, dhi);
            let span = phi - plo;
            let fy = |v: f64| {
                let f = if span > 0.0 { ((v - plo) / span).clamp(0.0, 1.0) as f32 } else { 0.5 };
                y0 + ih - f * ih
            };
            let fx = |j: usize| if n > 1 { x0 + slot_of(j) as f32 / (n - 1) as f32 * iw } else { x0 + iw / 2.0 };
            axis(&mut out, plo, phi, fy(0.0));
            let color = style.color;
            let width = line_width(style.line_weight);
            let mut run: Vec<[f32; 2]> = Vec::new();
            let flush = |run: &mut Vec<[f32; 2]>, out: &mut Vec<Prim>| {
                if run.len() >= 2 {
                    let pts = if run.len() > 4000 { run.iter().step_by(run.len() / 2000 + 1).copied().collect() } else { std::mem::take(run) };
                    out.push(Prim::Line { pts, color, width, dash: false });
                }
                run.clear();
            };
            for (j, v) in vals.iter().enumerate() {
                match v.or(zero.then_some(0.0)) {
                    Some(v) => run.push([fx(j), fy(v)]),
                    None if style.empty == SparklineEmpty::Span => {}
                    None => flush(&mut run, &mut out),
                }
            }
            flush(&mut run, &mut out);
            let r = (h * 0.08).clamp(1.0, 3.0);
            let step = (n / 500).max(1);
            let mut drawn = 0usize;
            for (j, v) in vals.iter().enumerate() {
                let Some(v) = v else { continue };
                let mark = point_mark(style, &facts, j, *v);
                let fill = match mark {
                    // Every-point highlights thin out on very long ranges.
                    Some((PointMark::Markers | PointMark::Negative, c)) if j % step == 0 => Some(c),
                    Some((PointMark::Markers | PointMark::Negative, _)) => None,
                    Some((_, c)) => Some(c),
                    None => (n == 1).then_some(color),
                };
                if let Some(fill) = fill {
                    drawn += 1;
                    if drawn > 2000 {
                        break;
                    }
                    out.push(Prim::Circle { cx: fx(j), cy: fy(*v), r, fill: Some(fill), stroke: None });
                }
            }
        }
        SparklineKind::Column | SparklineKind::WinLoss => {
            let slot = iw / n as f32;
            let bw = (slot * 0.8).max(0.25);
            let win = kind == SparklineKind::WinLoss;
            let (plo, phi) = if win {
                (-1.0, 1.0)
            } else {
                let (a, b) = plot_limits(style, lo, hi);
                (a.min(0.0), b.max(0.0))
            };
            let span = phi - plo;
            let fy = |v: f64| if span > 0.0 { y0 + ih - ((v - plo) / span).clamp(0.0, 1.0) as f32 * ih } else { y0 + ih };
            let base = fy(0.0);
            axis(&mut out, plo, phi, base);
            for (j, v) in vals.iter().enumerate() {
                // Empty cells: no bar in any mode (as zero they have no height).
                let Some(v) = v else { continue };
                let bar = if win { v.signum() * (*v != 0.0) as i32 as f64 } else { *v };
                if bar == 0.0 && win {
                    continue;
                }
                let top = fy(bar);
                let (y, hh) = (top.min(base), (top - base).abs().max(0.5));
                let fill = point_mark(style, &facts, j, *v).map_or(style.color, |(_, c)| c);
                let x = x0 + slot_of(j) as f32 * slot + (slot - bw) / 2.0;
                out.push(Prim::Rect { x, y, w: bw, h: hh, fill: Some(fill), stroke: None, radius: 0.0 });
            }
        }
    }
    finalize(out, w, h)
}

/// Shared vertical-axis limits of the sparkline groups of one sheet whose minimum or maximum is
/// "same for all sparklines" (`SparklineAxis::Group`), keyed by group id.
#[derive(Clone, Debug, Default)]
pub struct SparklineGroupLimits {
    limits: HashMap<u32, (f64, f64)>,
}

impl SparklineGroupLimits {
    /// Gathers the minimum and maximum over the values of every sparkline in a group that shares
    /// an axis limit. `values_of` evaluates a sparkline's source; it is called once per such
    /// sparkline. A sparkline with group id 0 is a group of its own and needs no entry.
    pub fn new(sparklines: &[Sparkline], mut values_of: impl FnMut(&Sparkline) -> Vec<Option<f64>>) -> Self {
        let mut limits: HashMap<u32, (f64, f64)> = HashMap::new();
        for sp in sparklines {
            let g = &sp.group;
            if g.id == 0 || !(g.min_axis == SparklineAxis::Group || g.max_axis == SparklineAxis::Group) {
                continue;
            }
            let entry = limits.entry(g.id).or_insert((f64::INFINITY, f64::NEG_INFINITY));
            for v in values_of(sp).into_iter().take(MAX_POINTS).flatten().filter(|v| v.is_finite()) {
                *entry = (entry.0.min(v), entry.1.max(v));
            }
        }
        limits.retain(|_, (lo, hi)| lo.is_finite() && hi.is_finite());
        SparklineGroupLimits { limits }
    }

    /// The shared (minimum, maximum) of group `id`, if it has values.
    pub fn get(&self, id: u32) -> Option<(f64, f64)> {
        self.limits.get(&id).copied()
    }
}

/// The drawing style of sparkline `sp`, its colours resolved against `theme`. The series colour
/// falls back to `fallback`; `own` is the sparkline's own values (used for a group-wide limit
/// when the sparkline is a group of its own) and `groups` the sheet's shared limits.
pub fn sparkline_style(sp: &Sparkline, theme: &Theme, fallback: [u8; 3], own: &[Option<f64>], groups: &SparklineGroupLimits) -> SparklineStyle {
    let rgba = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    let opt = |c: Option<Color>| c.and_then(|c| c.resolve(theme)).map(rgba);
    let g = &sp.group;
    let shared = if g.id == 0 {
        own.iter().flatten().filter(|v| v.is_finite()).fold(None, |acc: Option<(f64, f64)>, v| match acc {
            Some((a, b)) => Some((a.min(*v), b.max(*v))),
            None => Some((*v, *v)),
        })
    } else {
        groups.get(g.id)
    };
    let limit = |axis: SparklineAxis, manual: Option<f64>, pick: fn((f64, f64)) -> f64| match axis {
        SparklineAxis::Individual => None,
        SparklineAxis::Custom => manual.filter(|v| v.is_finite()),
        SparklineAxis::Group => shared.map(pick),
    };
    SparklineStyle {
        color: rgba(sp.color.resolve(theme).unwrap_or(fallback)),
        markers: sp.markers,
        high: g.high,
        low: g.low,
        first: g.first,
        last: g.last,
        negative: g.negative,
        color_markers: opt(g.color_markers),
        color_high: opt(g.color_high),
        color_low: opt(g.color_low),
        color_first: opt(g.color_first),
        color_last: opt(g.color_last),
        color_negative: opt(g.color_negative),
        axis: g.display_x_axis,
        color_axis: opt(g.color_axis),
        empty: g.display_empty,
        min: limit(g.min_axis, g.manual_min, |l| l.0),
        max: limit(g.max_axis, g.manual_max, |l| l.1),
        right_to_left: g.right_to_left,
        line_weight: g.line_weight,
    }
}
