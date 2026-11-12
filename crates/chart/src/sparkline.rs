//! In-cell sparklines.

use gridcraft_model::{SparklineEmpty, SparklineKind};

use crate::render::{DOWN, finalize};
use crate::resolve::MAX_POINTS;
use crate::{Prim, Rgba};

/// Draws a sparkline into a `w`×`h` box at the origin. Gaps (`None`) break lines; column and
/// win/loss sparklines draw negatives in our "down" red.
pub fn render_sparkline(kind: SparklineKind, values: &[Option<f64>], color: Rgba, markers: bool, w: f32, h: f32) -> Vec<Prim> {
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
    match kind {
        SparklineKind::Line => {
            let span = hi - lo;
            let fy = |v: f64| {
                let f = if span > 0.0 { ((v - lo) / span) as f32 } else { 0.5 };
                y0 + ih - f * ih
            };
            let fx = |j: usize| if n > 1 { x0 + j as f32 / (n - 1) as f32 * iw } else { x0 + iw / 2.0 };
            let mut run: Vec<[f32; 2]> = Vec::new();
            let flush = |run: &mut Vec<[f32; 2]>, out: &mut Vec<Prim>| {
                if run.len() >= 2 {
                    let pts = if run.len() > 4000 { run.iter().step_by(run.len() / 2000 + 1).copied().collect() } else { std::mem::take(run) };
                    out.push(Prim::Line { pts, color, width: 1.25, dash: false });
                }
                run.clear();
            };
            for (j, v) in vals.iter().enumerate() {
                match v {
                    Some(v) => run.push([fx(j), fy(*v)]),
                    None => flush(&mut run, &mut out),
                }
            }
            flush(&mut run, &mut out);
            if markers || n == 1 {
                let r = (h * 0.08).clamp(1.0, 3.0);
                let step = (n / 500).max(1);
                for (j, v) in vals.iter().enumerate().step_by(step) {
                    if let Some(v) = v {
                        out.push(Prim::Circle { cx: fx(j), cy: fy(*v), r, fill: Some(color), stroke: None });
                    }
                }
            }
        }
        SparklineKind::Column | SparklineKind::WinLoss => {
            let slot = iw / n as f32;
            let bw = (slot * 0.8).max(0.25);
            let win = kind == SparklineKind::WinLoss;
            let (lo0, hi0) = if win { (-1.0, 1.0) } else { (lo.min(0.0), hi.max(0.0)) };
            let span = hi0 - lo0;
            let fy = |v: f64| if span > 0.0 { y0 + ih - ((v - lo0) / span) as f32 * ih } else { y0 + ih };
            let base = fy(0.0);
            for (j, v) in vals.iter().enumerate() {
                let Some(v) = v else { continue };
                let v = if win { v.signum() * (*v != 0.0) as i32 as f64 } else { *v };
                if v == 0.0 && win {
                    continue;
                }
                let top = fy(v);
                let (y, hh) = (top.min(base), (top - base).abs().max(0.5));
                let fill = if v < 0.0 { DOWN } else { color };
                out.push(Prim::Rect { x: x0 + j as f32 * slot + (slot - bw) / 2.0, y, w: bw, h: hh, fill: Some(fill), stroke: None, radius: 0.0 });
            }
        }
    }
    finalize(out, w, h)
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

/// Draws a sparkline with its group's settings into a `w`×`h` box at the origin.
pub fn render_sparkline_styled(kind: SparklineKind, values: &[Option<f64>], style: &SparklineStyle, w: f32, h: f32) -> Vec<Prim> {
    // Stub: the point highlights, axis, empty-cell mode and limits are not drawn yet.
    render_sparkline(kind, values, style.color, style.markers, w, h)
}
