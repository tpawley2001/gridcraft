//! Surface charts: a grid of values (categories along X, series along Y or depth) coloured by
//! value bands. Contour charts are the grid seen from above; 3-D surface charts project it.

use super::frame::{Bx, value_range};
use super::legend::{Swatch, cat_label};
use super::{AXIS, Chart, ChartData, ChartKind, Ctx, Fmt, GRID, HAlign, LABEL_SIZE, Prim, Rgba, TEXT, Theme, VAlign, WHITE, nice_scale, with_fixed};
use crate::resolve::series_color;

/// Most bands (the value axis allows at most 50 major units).
const MAX_BANDS: usize = 50;
/// Most grid points drawn (categories x series); wider grids keep their first categories.
const MAX_GRID: usize = 10_000;
/// Most categories along a surface's X axis.
const MAX_SURFACE_CATS: usize = 1_000;

/// The value bands: `n` bands of `step`, the first starting at `lo`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bands {
    pub(super) lo: f64,
    pub(super) step: f64,
    pub(super) n: usize,
}

impl Bands {
    /// The band a value falls in; values below the first band or above the last join them.
    fn of(&self, v: f64) -> usize {
        let k = ((v - self.lo) / self.step).floor();
        if k.is_finite() && k > 0.0 { (k as usize).min(self.n.saturating_sub(1)) } else { 0 }
    }
    fn edge(&self, k: usize) -> f64 {
        self.lo + k as f64 * self.step
    }
    fn hi(&self) -> f64 {
        self.edge(self.n)
    }
}

fn is_wireframe(k: ChartKind) -> bool {
    matches!(k, ChartKind::SurfaceWireframe | ChartKind::ContourWireframe)
}

/// The grid: one row per series, one column per category (`None` = no number).
struct Grid {
    z: Vec<Vec<Option<f64>>>,
    ncat: usize,
}

impl Grid {
    fn new(data: &ChartData) -> Grid {
        let nser = data.series.len().max(1);
        let longest = data.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        let ncat = longest.min(MAX_SURFACE_CATS).min((MAX_GRID / nser).max(2));
        let z = data.series.iter().map(|s| (0..ncat).map(|j| s.values.get(j).copied().flatten().filter(|v| v.is_finite())).collect()).collect();
        Grid { z, ncat }
    }
    fn nser(&self) -> usize {
        self.z.len()
    }
    fn at(&self, i: usize, j: usize) -> Option<f64> {
        self.z.get(i).and_then(|r| r.get(j)).copied().flatten()
    }
}

/// The bands a surface chart is coloured with: the value axis' major unit (fixed, else the
/// automatic one) from the axis minimum (fixed, else the data minimum rounded down to a unit) to
/// the maximum (fixed, else the data maximum rounded up).
pub(super) fn bands(chart: &Chart, data: &ChartData) -> Option<Bands> {
    let grid = Grid::new(data);
    let (lo, hi) = value_range(grid.z.iter().flatten().flatten().copied())?;
    let fixed = &chart.scale.y;
    let step = with_fixed(nice_scale(lo, hi, false), fixed).step;
    if !(step.is_finite() && step > 0.0) {
        return None;
    }
    let given = |v: Option<f64>| v.filter(|v| v.is_finite());
    let bottom = given(fixed.min).unwrap_or((lo / step + 1e-9).floor() * step);
    let top = given(fixed.max).unwrap_or((hi / step - 1e-9).ceil() * step);
    let n = ((top - bottom) / step - 1e-9).ceil();
    let n = if n.is_finite() && n >= 1.0 { (n as usize).min(MAX_BANDS) } else { 1 };
    Some(Bands { lo: bottom, step, n })
}

fn band_color(data: &ChartData, k: usize) -> Rgba {
    k.checked_rem(data.palette.len()).and_then(|i| data.palette.get(i)).copied().unwrap_or_else(|| series_color(&Theme::default(), k))
}

fn fmt(data: &ChartData) -> Fmt {
    Fmt::new(data.series.first().map(|s| s.number_format.as_str()).unwrap_or("General"))
}

/// Legend entries of a surface chart: one per band, named by its range (`0-20`).
pub(super) fn band_entries(chart: &Chart, data: &ChartData) -> Vec<(String, Rgba, Swatch)> {
    let Some(b) = bands(chart, data) else { return vec![] };
    let f = fmt(data);
    let sw = if is_wireframe(chart.kind) { Swatch::Line } else { Swatch::Box };
    (0..b.n).map(|k| (format!("{}-{}", f.format(b.edge(k), Some(b.step)), f.format(b.edge(k + 1), Some(b.step))), band_color(data, k), sw)).collect()
}

/// A grid point: its position and value.
type Pt = ([f32; 2], f64);

/// The two triangles of every grid cell whose four corners hold numbers.
fn triangles(grid: &Grid, pos: &dyn Fn(usize, usize) -> [f32; 2]) -> Vec<[Pt; 3]> {
    let mut out = Vec::new();
    for i in 0..grid.nser().saturating_sub(1) {
        for j in 0..grid.ncat.saturating_sub(1) {
            let corner = |a: usize, b: usize| grid.at(a, b).map(|v| (pos(a, b), v));
            let (Some(p00), Some(p01), Some(p10), Some(p11)) = (corner(i, j), corner(i, j + 1), corner(i + 1, j), corner(i + 1, j + 1)) else {
                continue;
            };
            out.push([p00, p01, p11]);
            out.push([p00, p11, p10]);
        }
    }
    out
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    let s = if (b.1 - a.1).abs() > 0.0 { ((t - a.1) / (b.1 - a.1)).clamp(0.0, 1.0) as f32 } else { 0.0 };
    ([a.0[0] + (b.0[0] - a.0[0]) * s, a.0[1] + (b.0[1] - a.0[1]) * s], t)
}

/// The part of polygon `poly` where the value is `>= t` (`keep_above`) or `<= t`.
fn clip(poly: &[Pt], t: f64, keep_above: bool) -> Vec<Pt> {
    let inside = |p: &Pt| if keep_above { p.1 >= t } else { p.1 <= t };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for (k, cur) in poly.iter().enumerate() {
        let Some(prev) = poly.get(if k == 0 { poly.len() - 1 } else { k - 1 }) else { continue };
        match (inside(prev), inside(cur)) {
            (true, true) => out.push(*cur),
            (true, false) => out.push(lerp(*prev, *cur, t)),
            (false, true) => {
                out.push(lerp(*prev, *cur, t));
                out.push(*cur);
            }
            (false, false) => {}
        }
    }
    out
}

/// The iso-line where a triangle's value crosses `t`, if it does.
fn iso_segment(tri: &[Pt; 3], t: f64) -> Option<[[f32; 2]; 2]> {
    let mut pts = Vec::with_capacity(2);
    for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
        if (a.1 < t) != (b.1 < t) {
            pts.push(lerp(a, b, t).0);
        }
    }
    match pts.as_slice() {
        [p, q] => Some([*p, *q]),
        _ => None,
    }
}

/// The widest of `labels` (the first 2000), for margins.
fn widest(ctx: &Ctx, labels: &[String]) -> f32 {
    labels.iter().take(2000).map(|t| ctx.tw(t, LABEL_SIZE)).fold(0.0f32, f32::max)
}

fn labels(data: &ChartData, grid: &Grid) -> (Vec<String>, Vec<String>) {
    let cats = (0..grid.ncat).map(|j| cat_label(data, j)).collect();
    let sers = data.series.iter().enumerate().map(|(i, s)| if s.name.is_empty() { format!("Series{}", i + 1) } else { s.name.clone() }).collect();
    (cats, sers)
}

/// Every `k`-th label so that labels `slot` apart and `size` long don't overlap.
fn label_step(size: f32, slot: f32, n: usize) -> usize {
    ((size / slot.max(1e-3)).ceil() as usize).clamp(1, n.max(1))
}

// ---------------------------------------------------------------- contour (top view)

pub(super) fn contour(ctx: &mut Ctx, chart: &Chart, data: &ChartData, area: Bx) {
    let grid = Grid::new(data);
    let (cats, sers) = labels(data, &grid);
    let lh = LABEL_SIZE * 1.4;
    let left = (widest(ctx, &sers) + 6.0).min(area.w * 0.3);
    let plot = Bx { x: area.x + left, y: area.y + lh * 0.5, w: area.w - left - 6.0, h: area.h - lh * 1.5 - 4.0 };
    if !plot.ok() {
        return;
    }
    let span = |n: usize| n.saturating_sub(1).max(1) as f32;
    let (nc, ns) = (grid.ncat, grid.nser());
    let pos = |i: usize, j: usize| [plot.x + j as f32 / span(nc) * plot.w, plot.bottom() - i as f32 / span(ns) * plot.h];
    if let Some(b) = bands(chart, data) {
        let tris = triangles(&grid, &pos);
        if is_wireframe(chart.kind) {
            for k in 0..=b.n {
                let color = band_color(data, k.min(b.n.saturating_sub(1)));
                for tri in &tris {
                    if let Some([p, q]) = iso_segment(tri, b.edge(k)) {
                        ctx.line(vec![p, q], color, 1.0);
                    }
                }
            }
        } else {
            for tri in &tris {
                let (lo, hi) = tri.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, z), p| (a.min(p.1), z.max(p.1)));
                for k in b.of(lo)..=b.of(hi) {
                    // The first and last bands also take values outside the axis bounds.
                    let mut poly = tri.to_vec();
                    if k > 0 {
                        poly = clip(&poly, b.edge(k), true);
                    }
                    if k + 1 < b.n {
                        poly = clip(&poly, b.edge(k + 1), false);
                    }
                    if poly.len() >= 3 {
                        let color = band_color(data, k);
                        ctx.out.push(Prim::Polygon { pts: poly.into_iter().map(|p| p.0).collect(), fill: color, stroke: Some((color, 0.5)) });
                    }
                }
            }
        }
    }
    ctx.out.push(Prim::Rect { x: plot.x, y: plot.y, w: plot.w, h: plot.h, fill: None, stroke: Some((AXIS, 0.75)), radius: 0.0 });
    let k = label_step(widest(ctx, &cats) + 6.0, plot.w / span(nc), nc);
    for (j, t) in cats.iter().enumerate().step_by(k) {
        let p = pos(0, j);
        let t = ctx.fit(t, LABEL_SIZE, plot.w / span(nc) * k as f32 + 4.0);
        ctx.text(p[0], plot.bottom() + 3.0, &t, LABEL_SIZE, TEXT, (HAlign::Center, VAlign::Top));
    }
    let k = label_step(lh, plot.h / span(ns), ns);
    for (i, t) in sers.iter().enumerate().step_by(k) {
        let p = pos(i, 0);
        let t = ctx.fit(t, LABEL_SIZE, left - 6.0);
        ctx.text(plot.x - 4.0, p[1], &t, LABEL_SIZE, TEXT, (HAlign::Right, VAlign::Middle));
    }
}

// ---------------------------------------------------------------- 3-D surface

/// A point in the chart's unit box: `x` along the categories (0 = first, at the front left),
/// `y` up the value axis, `z` back along the series.
type P3 = [f32; 3];

/// Projects the unit box with Excel's default 3-D surface view (rotX 15, rotY 20,
/// perspective 30): `(screen x, screen y up, depth)`, depth growing away from the viewer.
fn project(p: P3) -> [f32; 3] {
    let (rx, ry) = (15f32.to_radians(), 20f32.to_radians());
    // The depth axis is drawn as deep as the category axis is wide.
    let (x, y, z) = (p[0] - 0.5, p[1] - 0.5, p[2] - 0.5);
    let x1 = x * ry.cos() + z * ry.sin();
    let z1 = z * ry.cos() - x * ry.sin();
    let y2 = y * rx.cos() + z1 * rx.sin();
    let d = z1 * rx.cos() - y * rx.sin();
    // Perspective 30: a mild shrink with distance.
    let f = 1.0 / (1.0 + 0.125 * d);
    [x1 * f, y2 * f, d]
}

pub(super) fn surface_3d(ctx: &mut Ctx, chart: &Chart, data: &ChartData, area: Bx) {
    let grid = Grid::new(data);
    let (cats, sers) = labels(data, &grid);
    let Some(b) = bands(chart, data) else { return };
    let f = fmt(data);
    let vlabels: Vec<String> = (0..=b.n).map(|k| f.format(b.edge(k), Some(b.step))).collect();
    let lh = LABEL_SIZE * 1.4;
    let left = (widest(ctx, &vlabels) + 8.0).min(area.w * 0.25);
    let right = (widest(ctx, &sers) + 8.0).min(area.w * 0.25);
    let plot = Bx { x: area.x + left, y: area.y + lh * 0.5, w: area.w - left - right, h: area.h - lh * 2.0 - 4.0 };
    if !plot.ok() {
        return;
    }
    // Fit the projected box into the plot.
    let proj: Vec<[f32; 3]> = (0..8u8).map(|c| project([f32::from(c & 1), f32::from((c >> 1) & 1), f32::from((c >> 2) & 1)])).collect();
    let (mut x0, mut x1, mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
    for p in &proj {
        (x0, x1, y0, y1) = (x0.min(p[0]), x1.max(p[0]), y0.min(p[1]), y1.max(p[1]));
    }
    let (sx, sy) = (plot.w / (x1 - x0).max(1e-3), plot.h / (y1 - y0).max(1e-3));
    let screen = |p: P3| -> ([f32; 2], f32) {
        let q = project(p);
        ([plot.x + (q[0] - x0) * sx, plot.bottom() - (q[1] - y0) * sy], q[2])
    };
    let s2 = |p: P3| screen(p).0;
    let span = (b.hi() - b.lo).max(f64::MIN_POSITIVE);
    let yv = |v: f64| ((v - b.lo) / span).clamp(0.0, 1.0) as f32;

    // Floor and the two back walls (left side and back), gridlines at each major unit.
    let wall = |pts: [P3; 4]| pts.iter().map(|p| s2(*p)).collect::<Vec<_>>();
    for pts in [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
        [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0]],
        [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
    ] {
        ctx.out.push(Prim::Polygon { pts: wall(pts), fill: WHITE, stroke: Some((AXIS, 0.75)) });
    }
    let every = label_step(lh, plot.h / b.n.max(1) as f32 * 0.8, b.n + 1);
    for (k, t) in vlabels.iter().enumerate() {
        let y = yv(b.edge(k));
        if chart.gridlines {
            ctx.line(vec![s2([0.0, y, 0.0]), s2([0.0, y, 1.0]), s2([1.0, y, 1.0])], GRID, 0.75);
        }
        if k % every == 0 {
            let p = s2([0.0, y, 0.0]);
            ctx.text(p[0] - 4.0, p[1], t, LABEL_SIZE, TEXT, (HAlign::Right, VAlign::Middle));
        }
    }

    // The surface: one quad per grid cell, farthest first.
    let span_n = |n: usize| n.saturating_sub(1).max(1) as f32;
    let (nc, ns) = (grid.ncat, grid.nser());
    let mut quads: Vec<(f32, Vec<[f32; 2]>, f64)> = Vec::new();
    for i in 0..ns.saturating_sub(1) {
        for j in 0..nc.saturating_sub(1) {
            let mut pts = Vec::with_capacity(4);
            let mut depth = 0.0f32;
            let mut sum = 0.0f64;
            for (a, c) in [(i, j), (i, j + 1), (i + 1, j + 1), (i + 1, j)] {
                let Some(v) = grid.at(a, c) else { break };
                let (p, d) = screen([c as f32 / span_n(nc), yv(v), a as f32 / span_n(ns)]);
                pts.push(p);
                depth += d;
                sum += v;
            }
            if pts.len() == 4 {
                quads.push((depth, pts, sum / 4.0));
            }
        }
    }
    quads.sort_by(|a, b| b.0.total_cmp(&a.0));
    let wire = is_wireframe(chart.kind);
    for (_, pts, v) in quads {
        let color = band_color(data, b.of(v));
        if wire {
            let mut ring = pts;
            if let Some(p) = ring.first().copied() {
                ring.push(p);
            }
            ctx.line(ring, color, 1.0);
        } else {
            ctx.out.push(Prim::Polygon { pts, fill: color, stroke: Some((darker(color), 0.5)) });
        }
    }

    // Category labels along the front edge, series labels along the right edge of the floor.
    let k = label_step(widest(ctx, &cats) + 6.0, plot.w / span_n(nc), nc);
    for (j, t) in cats.iter().enumerate().step_by(k) {
        let p = s2([j as f32 / span_n(nc), 0.0, 0.0]);
        let t = ctx.fit(t, LABEL_SIZE, plot.w / span_n(nc) * k as f32 + 4.0);
        ctx.text(p[0], p[1] + 3.0, &t, LABEL_SIZE, TEXT, (HAlign::Center, VAlign::Top));
    }
    let k = label_step(lh, plot.h * 0.3 / span_n(ns), ns);
    for (i, t) in sers.iter().enumerate().step_by(k) {
        let p = s2([1.0, 0.0, i as f32 / span_n(ns)]);
        let t = ctx.fit(t, LABEL_SIZE, right - 6.0);
        ctx.text(p[0] + 4.0, p[1] + 2.0, &t, LABEL_SIZE, TEXT, (HAlign::Left, VAlign::Top));
    }
}

fn darker(c: Rgba) -> Rgba {
    let d = |v: u8| (f32::from(v) * 0.8) as u8;
    [d(c[0]), d(c[1]), d(c[2]), c[3]]
}

pub(super) fn surface(ctx: &mut Ctx, chart: &Chart, data: &ChartData, area: Bx) {
    match chart.kind {
        ChartKind::Contour | ChartKind::ContourWireframe => contour(ctx, chart, data, area),
        _ => surface_3d(ctx, chart, data, area),
    }
}
