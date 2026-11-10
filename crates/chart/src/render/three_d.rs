//! 3-D column, bar, line, area and pie charts: an oblique projection of the chart's `view3D`.
//!
//! Columns and bars are boxes (a lit front face, darker top and side faces) standing on a floor in
//! front of a back wall and a side wall with gridlines; series in depth stand one behind the other
//! and draw back to front. Lines are ribbons, areas extruded slabs, and a pie is a tilted ellipse
//! with a side band. The X rotation tilts the view, the Y rotation turns the depth sideways; the
//! depth comes from `depthPercent` and `gapDepth`. Perspective, `hPercent` and bar shapes other
//! than boxes are not modelled (cylinders, cones and pyramids draw as boxes).

use std::f32::consts::{PI, TAU};

use gridcraft_model::{Chart3d, Series};

use super::cartesian::{Fam, axis_scales, family, series_spans};
use super::frame::{Bx, on};
use super::legend::{cat_label, point_color};
use super::{
    Chart, ChartData, ChartKind, Ctx, Fmt, GRID, HAlign, LABEL_SIZE, MAX_CATS, MAX_LABELS, MAX_WEDGES, Prim, Rgba, Scale, TEXT, VAlign, WHITE,
};

/// Most boxes, ribbon segments or area segments a 3-D chart draws (each is up to four polygons).
const MAX_PIECES: usize = 20_000;
/// The default view of a 3-D column, bar, line or area chart (degrees).
const DEFAULT_ROT_X: f32 = 15.0;
const DEFAULT_ROT_Y: f32 = 20.0;
/// The default elevation of a 3-D pie (degrees).
const DEFAULT_PIE_ROT_X: f32 = 30.0;
const TOP_SHADE: f32 = 0.85;
const SIDE_SHADE: f32 = 0.7;
const WALL: Rgba = [0xF7, 0xF7, 0xF7, 0xFF];

/// Whether `chart` draws in 3-D here.
pub(super) fn is_3d(chart: &Chart) -> bool {
    use ChartKind::*;
    chart.view_3d.is_some()
        && matches!(
            chart.kind,
            ColumnClustered
                | ColumnStacked
                | ColumnStacked100
                | BarClustered
                | BarStacked
                | BarStacked100
                | Line
                | LineMarkers
                | LineStacked
                | Area
                | AreaStacked
                | AreaStacked100
                | Pie
        )
}

/// `c` darkened (`f` < 1) keeping its alpha.
fn shade(c: Rgba, f: f32) -> Rgba {
    let k = |v: u8| (f32::from(v) * f).round().clamp(0.0, 255.0) as u8;
    [k(c[0]), k(c[1]), k(c[2]), c[3]]
}

fn poly(ctx: &mut Ctx, pts: Vec<[f32; 2]>, fill: Rgba, stroke: Option<(Rgba, f32)>) {
    if pts.len() >= 3 {
        ctx.out.push(Prim::Polygon { pts, fill, stroke });
    }
}

fn deg(v: f32) -> f32 {
    v * PI / 180.0
}

/// The oblique projection: `u` runs along the categories, `v` along the values and `w` into the
/// depth, all in pixels from the front-bottom-left corner of the plot box.
struct Proj {
    x0: f32,
    y0: f32,
    /// Screen offset per pixel of depth.
    sx: f32,
    sy: f32,
    horizontal: bool,
}

impl Proj {
    fn at(&self, u: f32, v: f32, w: f32) -> [f32; 2] {
        if self.horizontal { [self.x0 + v + w * self.sx, self.y0 - u - w * self.sy] } else { [self.x0 + u + w * self.sx, self.y0 - v - w * self.sy] }
    }
    /// The depth offset along the category direction / value direction (sign says which side shows).
    fn su(&self) -> f32 {
        if self.horizontal { self.sy } else { self.sx }
    }
    fn sv(&self) -> f32 {
        if self.horizontal { self.sx } else { self.sy }
    }
    /// Shades of the face at the value end and the face at the category side.
    fn shades(&self) -> (f32, f32) {
        if self.horizontal { (SIDE_SHADE, TOP_SHADE) } else { (TOP_SHADE, SIDE_SHADE) }
    }
}

#[derive(Clone, Copy)]
struct Cuboid {
    u0: f32,
    u1: f32,
    v0: f32,
    v1: f32,
    w0: f32,
    w1: f32,
    color: Rgba,
}

/// A box: the visible value-end face, the visible category-side face, then the front face.
fn cuboid(ctx: &mut Ctx, p: &Proj, b: &Cuboid) {
    let (vs, us) = p.shades();
    if p.sv().abs() > 1e-3 {
        let v = if p.sv() > 0.0 { b.v1 } else { b.v0 };
        poly(ctx, vec![p.at(b.u0, v, b.w0), p.at(b.u1, v, b.w0), p.at(b.u1, v, b.w1), p.at(b.u0, v, b.w1)], shade(b.color, vs), None);
    }
    if p.su().abs() > 1e-3 {
        let u = if p.su() > 0.0 { b.u1 } else { b.u0 };
        poly(ctx, vec![p.at(u, b.v0, b.w0), p.at(u, b.v1, b.w0), p.at(u, b.v1, b.w1), p.at(u, b.v0, b.w1)], shade(b.color, us), None);
    }
    poly(ctx, vec![p.at(b.u0, b.v0, b.w0), p.at(b.u1, b.v0, b.w0), p.at(b.u1, b.v1, b.w0), p.at(b.u0, b.v1, b.w0)], b.color, None);
}

/// Where a 3-D column, bar, line or area chart's pieces go.
struct Layout {
    p: Proj,
    /// Lengths along the categories, values and depth.
    cat_len: f32,
    val_len: f32,
    depth: f32,
    /// One category's width.
    cw: f32,
    /// Depth of one row and the box's share of it (the rest is the gap around it).
    pitch: f32,
    fill_frac: f32,
    in_depth: bool,
    scale: Scale,
}

impl Layout {
    /// The value `x` along the value axis, clipped to the plot.
    fn vpos(&self, x: f64) -> f32 {
        self.scale.frac(x).clamp(0.0, 1.0) * self.val_len
    }
    /// Depth span of row `r`.
    fn row_w(&self, r: usize) -> (f32, f32) {
        let w0 = r as f32 * self.pitch + self.pitch * (1.0 - self.fill_frac) / 2.0;
        (w0, w0 + self.pitch * self.fill_frac)
    }
    fn row_of(&self, i: usize) -> usize {
        if self.in_depth { i } else { 0 }
    }
}

/// Labels the axes take room for.
struct AxisText {
    ticks: Vec<f64>,
    labels: Vec<String>,
    cat_w: f32,
    ser_w: f32,
}

/// Fits the plot box, the projection and the depth into `area`.
#[allow(clippy::too_many_arguments)]
fn layout(
    area: Bx,
    v: &Chart3d,
    scale: Scale,
    text: &AxisText,
    val_w: f32,
    horizontal: bool,
    ncat: usize,
    nslots: usize,
    nrows: usize,
    in_depth: bool,
) -> Option<Layout> {
    let depth_frac = f32::from(v.depth_percent.unwrap_or(100).clamp(20, 2000)) / 100.0;
    let gap_frac = f32::from(v.gap_depth.unwrap_or(150).min(500)) / 100.0;
    let rot_x = deg(v.rot_x.map_or(DEFAULT_ROT_X, f32::from).clamp(-90.0, 90.0));
    let rot_y = deg(v.rot_y.map_or(DEFAULT_ROT_Y, f32::from));
    let (dir_x, dir_y) = (rot_y.sin(), rot_x.sin());
    let text_h = LABEL_SIZE * 1.6;
    let left = if horizontal { text.cat_w } else { val_w };
    let b = Bx { x: area.x + left + 6.0, y: area.y + 2.0, w: area.w - left - 8.0 - text.ser_w, h: area.h - text_h - 4.0 };
    if !b.ok() {
        return None;
    }
    // Lengths along categories (L), values and depth (D): D = c·L, and the category direction
    // loses |D·s| to the depth offset.
    let (cat_room, val_room) = if horizontal { (b.h, b.w) } else { (b.w, b.h) };
    let (cat_dir, val_dir) = if horizontal { (dir_y, dir_x) } else { (dir_x, dir_y) };
    let bar_frac = 1.0 / (nslots as f32 + 1.5);
    let c = nrows as f32 * (depth_frac + gap_frac) * bar_frac / ncat as f32;
    let mut cat_len = cat_room / (1.0 + c * cat_dir.abs());
    let mut depth = c * cat_len;
    let cap = 0.6 * b.w.min(b.h);
    if depth > cap {
        depth = cap;
        cat_len = cat_room - depth * cat_dir.abs();
    }
    let val_len = val_room - depth * val_dir.abs();
    if !(cat_len > 4.0 && val_len > 4.0 && depth.is_finite()) {
        return None;
    }
    let (dx, dy) = (depth * dir_x, depth * dir_y);
    Some(Layout {
        p: Proj { x0: b.x + (-dx).max(0.0), y0: b.bottom() - (-dy).max(0.0), sx: dir_x, sy: dir_y, horizontal },
        cat_len,
        val_len,
        depth,
        cw: cat_len / ncat as f32,
        pitch: depth / nrows as f32,
        fill_frac: depth_frac / (depth_frac + gap_frac),
        in_depth,
        scale,
    })
}

/// The back wall, side wall and floor with their gridlines, and the value, category and series labels.
fn walls(ctx: &mut Ctx, chart: &Chart, data: &ChartData, lay: &Layout, text: &AxisText, ncat: usize) {
    let p = &lay.p;
    let (cat_len, val_len, depth) = (lay.cat_len, lay.val_len, lay.depth);
    let side_u = if p.su() >= 0.0 { 0.0 } else { cat_len };
    let floor_v = if p.sv() >= 0.0 { 0.0 } else { val_len };
    let edge = Some((GRID, 0.75));
    poly(ctx, vec![p.at(0.0, 0.0, depth), p.at(cat_len, 0.0, depth), p.at(cat_len, val_len, depth), p.at(0.0, val_len, depth)], WALL, edge);
    poly(ctx, vec![p.at(side_u, 0.0, 0.0), p.at(side_u, val_len, 0.0), p.at(side_u, val_len, depth), p.at(side_u, 0.0, depth)], WALL, edge);
    poly(ctx, vec![p.at(0.0, floor_v, 0.0), p.at(cat_len, floor_v, 0.0), p.at(cat_len, floor_v, depth), p.at(0.0, floor_v, depth)], WHITE, edge);
    for (t, label) in text.ticks.iter().zip(&text.labels) {
        let y = lay.vpos(*t);
        if chart.gridlines {
            ctx.line(vec![p.at(side_u, y, 0.0), p.at(side_u, y, depth), p.at(cat_len - side_u, y, depth)], GRID, 0.75);
        }
        let [x, sy] = p.at(0.0, y, 0.0);
        if p.horizontal {
            ctx.text(x, sy + 4.0, label, LABEL_SIZE, TEXT, (HAlign::Center, VAlign::Top));
        } else {
            ctx.text(x - 4.0, sy, label, LABEL_SIZE, TEXT, (HAlign::Right, VAlign::Middle));
        }
    }
    let step = (ncat / 100).max(1);
    for j in (0..ncat).step_by(step) {
        let room = if p.horizontal { text.cat_w } else { (lay.cw * step as f32).max(LABEL_SIZE) };
        let label = ctx.fit(&cat_label(data, j), LABEL_SIZE, room);
        let [x, y] = p.at((j as f32 + 0.5) * lay.cw, 0.0, 0.0);
        if p.horizontal {
            ctx.text(x - 4.0, y, &label, LABEL_SIZE, TEXT, (HAlign::Right, VAlign::Middle));
        } else {
            ctx.text(x, y + 4.0, &label, LABEL_SIZE, TEXT, (HAlign::Center, VAlign::Top));
        }
    }
    if lay.in_depth {
        for (r, s) in data.series.iter().enumerate() {
            let (w0, w1) = lay.row_w(r);
            let name = ctx.fit(&s.name, LABEL_SIZE, text.ser_w.max(LABEL_SIZE));
            let [x, y] = if p.horizontal { p.at(0.0, val_len, (w0 + w1) / 2.0) } else { p.at(cat_len, 0.0, (w0 + w1) / 2.0) };
            ctx.text(x + 4.0, y, &name, LABEL_SIZE, TEXT, (HAlign::Left, VAlign::Middle));
        }
    }
}

/// Draws a 3-D column, bar, line or area chart into `area` (axis titles already reserved).
pub(super) fn cartesian_3d(ctx: &mut Ctx, chart: &Chart, data: &ChartData, area: Bx, v: &Chart3d) {
    let nser = data.series.len();
    if nser == 0 {
        return;
    }
    let ncat = data.categories.len().max(data.series.iter().map(|s| s.values.len()).max().unwrap_or(0)).min(MAX_CATS).min((MAX_PIECES / nser).max(1));
    if ncat == 0 {
        return;
    }
    let horizontal = matches!(chart.kind, ChartKind::BarClustered | ChartKind::BarStacked | ChartKind::BarStacked100);
    let fam = family(chart.kind);
    let fams = vec![fam; nser];
    let sec = vec![false; nser];
    let spans = series_spans(data, &fams, &sec, ncat);
    let axes = axis_scales(chart, data, &fams, &sec, &spans);
    let Some((scale, fmt)) = axes.first() else { return };
    let ticks = scale.ticks();
    let labels: Vec<String> = ticks.iter().map(|t| fmt.format(*t, Some(scale.step))).collect();
    // Series in depth: a 3-D line always; columns and areas when the file says so.
    let in_depth = match fam {
        Fam::Line | Fam::LineStack => true,
        Fam::Col | Fam::Area => v.series_in_depth,
        _ => false,
    };
    let nrows = if in_depth { nser } else { 1 };
    let nslots = if fam == Fam::Col && !in_depth { nser } else { 1 };
    let val_w = labels.iter().map(|t| ctx.tw(t, LABEL_SIZE)).fold(0.0, f32::max);
    let cat_w = (0..ncat.min(200)).map(|j| ctx.tw(&cat_label(data, j), LABEL_SIZE)).fold(0.0, f32::max).min(area.w * 0.3);
    let ser_w = if in_depth { data.series.iter().map(|s| ctx.tw(&s.name, LABEL_SIZE)).fold(0.0, f32::max).min(area.w * 0.25) } else { 0.0 };
    let text = AxisText { ticks, labels, cat_w, ser_w };
    let Some(lay) = layout(area, v, *scale, &text, val_w, horizontal, ncat, nslots, nrows, in_depth) else { return };
    walls(ctx, chart, data, &lay, &text, ncat);
    let labels_on = chart.data_labels && ncat <= MAX_LABELS && nser.saturating_mul(ncat) <= MAX_LABELS * 4;
    match fam {
        Fam::Line | Fam::LineStack | Fam::Area | Fam::AreaStack | Fam::Area100 => surfaces(ctx, data, &lay, &spans, fam, labels_on),
        _ => boxes(ctx, data, &lay, &spans, fam, nslots, labels_on),
    }
}

/// 3-D lines (ribbons) and areas (extruded slabs), farther rows first.
fn surfaces(ctx: &mut Ctx, data: &ChartData, lay: &Layout, spans: &[Vec<Option<(f64, f64)>>], fam: Fam, labels_on: bool) {
    let p = &lay.p;
    let line = matches!(fam, Fam::Line | Fam::LineStack);
    let u_of = |j: usize| (j as f32 + 0.5) * lay.cw;
    let thick = (lay.val_len * 0.03).clamp(1.5, 4.0);
    let base = lay.vpos(0.0);
    let mut order: Vec<usize> = (0..data.series.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(lay.row_of(*i)));
    for i in order {
        let (Some(s), Some(row)) = (data.series.get(i), spans.get(i)) else { continue };
        let (w0, w1) = lay.row_w(lay.row_of(i));
        let pts: Vec<Option<(f32, f32)>> = row.iter().map(|sp| sp.map(|(lo, hi)| (lay.vpos(lo), lay.vpos(hi)))).collect();
        let mut segs: Vec<usize> = (0..pts.len().saturating_sub(1)).collect();
        if p.su() < 0.0 {
            segs.reverse();
        }
        let top_face = |ctx: &mut Ctx, j: usize, a: f32, z: f32| {
            let (u0, u1) = (u_of(j), u_of(j + 1));
            poly(ctx, vec![p.at(u0, a, w0), p.at(u1, z, w0), p.at(u1, z, w1), p.at(u0, a, w1)], shade(s.color, TOP_SHADE), None);
        };
        if line {
            for j in segs {
                let (Some(Some((_, a))), Some(Some((_, z)))) = (pts.get(j), pts.get(j + 1)) else { continue };
                top_face(ctx, j, *a, *z);
                let (u0, u1) = (u_of(j), u_of(j + 1));
                poly(ctx, vec![p.at(u0, *a, w0), p.at(u1, *z, w0), p.at(u1, *z - thick, w0), p.at(u0, *a - thick, w0)], s.color, None);
            }
        } else {
            let pts: Vec<(f32, f32)> = pts.iter().map(|x| x.unwrap_or((base, base))).collect();
            for j in segs {
                if let (Some((_, a)), Some((_, z))) = (pts.get(j), pts.get(j + 1)) {
                    top_face(ctx, j, *a, *z);
                }
            }
            // The end cap on the visible side.
            let end = if p.su() >= 0.0 { pts.len().checked_sub(1) } else { Some(0) };
            if let Some(j) = end
                && let Some((lo, hi)) = pts.get(j).copied()
            {
                let u = u_of(j);
                poly(ctx, vec![p.at(u, lo, w0), p.at(u, hi, w0), p.at(u, hi, w1), p.at(u, lo, w1)], shade(s.color, SIDE_SHADE), None);
            }
            let mut front: Vec<[f32; 2]> = pts.iter().enumerate().map(|(j, (_, hi))| p.at(u_of(j), *hi, w0)).collect();
            front.extend(pts.iter().enumerate().rev().map(|(j, (lo, _))| p.at(u_of(j), *lo, w0)));
            if let [(lo, hi)] = pts.as_slice() {
                // A single point: give it some width.
                let (u, d) = (u_of(0), lay.cw * 0.3);
                front = vec![p.at(u - d, *hi, w0), p.at(u + d, *hi, w0), p.at(u + d, *lo, w0), p.at(u - d, *lo, w0)];
            }
            poly(ctx, front, s.color, None);
        }
        if labels_on {
            let f = Fmt::new(&s.number_format);
            for (j, x) in row.iter().enumerate() {
                if let (Some((_, hi)), Some(val)) = (x, s.values.get(j).copied().flatten()) {
                    let [x, y] = p.at(u_of(j), lay.vpos(*hi), w0);
                    ctx.text(x, y - 4.0, &f.format(val, None), LABEL_SIZE, TEXT, (HAlign::Center, VAlign::Bottom));
                }
            }
        }
    }
}

/// 3-D columns and bars: one box per value, drawn back to front.
fn boxes(ctx: &mut Ctx, data: &ChartData, lay: &Layout, spans: &[Vec<Option<(f64, f64)>>], fam: Fam, nslots: usize, labels_on: bool) {
    let p = &lay.p;
    let bw = lay.cw / (nslots as f32 + 1.5);
    let mut all: Vec<(Cuboid, usize, f64)> = Vec::new();
    for (i, s) in data.series.iter().enumerate() {
        let Some(row) = spans.get(i) else { continue };
        let (w0, w1) = lay.row_w(lay.row_of(i));
        let k = if nslots > 1 { i as f32 } else { 0.0 };
        for (j, sp) in row.iter().enumerate() {
            let Some((lo, hi)) = sp else { continue };
            let u0 = j as f32 * lay.cw + (lay.cw - nslots as f32 * bw) / 2.0 + k * bw;
            let (a, z) = (lay.vpos(*lo), lay.vpos(*hi));
            let cub = Cuboid { u0, u1: u0 + bw, v0: a.min(z), v1: a.max(z), w0, w1, color: s.color };
            all.push((cub, i, s.values.get(j).copied().flatten().unwrap_or(0.0)));
        }
    }
    // Back to front: farther rows, then along the categories toward the visible side, then
    // toward the visible value end.
    let (su, sv) = (p.su(), p.sv());
    all.sort_by(|(a, ..), (b, ..)| {
        b.w0.total_cmp(&a.w0)
            .then_with(|| if su >= 0.0 { a.u0.total_cmp(&b.u0) } else { b.u0.total_cmp(&a.u0) })
            .then_with(|| if sv >= 0.0 { a.v0.total_cmp(&b.v0) } else { b.v0.total_cmp(&a.v0) })
    });
    for (cub, ..) in &all {
        cuboid(ctx, p, cub);
    }
    if !labels_on {
        return;
    }
    for (cub, i, val) in &all {
        let Some(s) = data.series.get(*i) else { continue };
        let t = Fmt::new(&s.number_format).format(*val, None);
        let um = (cub.u0 + cub.u1) / 2.0;
        if fam != Fam::Col {
            let [x, y] = p.at(um, (cub.v0 + cub.v1) / 2.0, cub.w0);
            ctx.text(x, y, &t, LABEL_SIZE, on(cub.color), (HAlign::Center, VAlign::Middle));
        } else if p.horizontal {
            let [x, y] = p.at(um, cub.v1, cub.w0);
            ctx.text(x + 3.0, y, &t, LABEL_SIZE, TEXT, (HAlign::Left, VAlign::Middle));
        } else {
            let [x, y] = p.at(um, cub.v1, cub.w0);
            ctx.text(x, y - 2.0, &t, LABEL_SIZE, TEXT, (HAlign::Center, VAlign::Bottom));
        }
    }
}

/// A 3-D pie: the first series as a tilted ellipse (the X rotation is the elevation) with a side
/// band below the front half; the Y rotation turns the first slice clockwise from 12 o'clock, and
/// the series' explosion (or a slice's own) pulls slices out from the centre.
pub(super) fn pie_3d(ctx: &mut Ctx, chart: &Chart, data: &ChartData, area: Bx, v: &Chart3d) {
    let Some(s) = data.series.first() else { return };
    let vals: Vec<f64> = s.values.iter().take(MAX_CATS).map(|v| v.map(f64::abs).unwrap_or(0.0)).collect();
    let total: f64 = vals.iter().sum();
    if !(total.is_finite() && total > 0.0) {
        return;
    }
    let elev = deg(v.rot_x.map_or(DEFAULT_PIE_ROT_X, f32::from).abs().clamp(0.0, 90.0));
    let tilt = elev.sin().clamp(0.15, 1.0);
    let band_k = 0.25 * elev.cos().max(0.0);
    let model = chart.series.first();
    let pulled = |j: usize| model.map_or(0, |m| m.explosion_of(j)).min(Series::MAX_EXPLOSION) as f32 / 100.0;
    let explode = model.map_or(0, Series::max_explosion).min(Series::MAX_EXPLOSION) as f32 / 100.0;
    let r = (area.w * 0.46 / (1.0 + explode)).min(area.h * 0.92 / (2.0 * tilt * (1.0 + explode) + band_k));
    if !(r.is_finite() && r >= 2.0) {
        return;
    }
    let thick = r * band_k;
    let (cx, cy) = (area.cx(), area.cy() - thick / 2.0);
    let start = deg(f32::from(v.rot_y.unwrap_or(0) % 360));
    // Slices: (start angle, sweep, index).
    let mut slices: Vec<(f32, f32, usize)> = Vec::new();
    let mut a = start;
    for (j, val) in vals.iter().enumerate() {
        let sweep = (*val / total) as f32 * TAU;
        if sweep > 1e-4 && slices.len() < MAX_WEDGES {
            slices.push((a, sweep, j));
        }
        a += sweep;
    }
    let rim = |ox: f32, oy: f32, t: f32, drop: f32| [cx + ox + r * t.sin(), cy + oy - r * tilt * t.cos() + drop];
    let arc = |a0: f32, sweep: f32| {
        let n = ((sweep / (TAU / 96.0)).ceil() as usize).clamp(2, 97);
        (0..=n).map(move |k| a0 + sweep * k as f32 / n as f32)
    };
    // Back slices (whose middle is nearer 12 o'clock) first.
    let mut order = slices.clone();
    order.sort_by(|x, y| (x.0 + x.1 / 2.0).cos().total_cmp(&(y.0 + y.1 / 2.0).cos()).reverse());
    let mut labels = Vec::new();
    for (a0, sweep, j) in order {
        let fill = point_color(data, j);
        let mid = a0 + sweep / 2.0;
        let e = pulled(j);
        let (ox, oy) = (e * r * mid.sin(), -e * r * tilt * mid.cos());
        let centre = [cx + ox, cy + oy];
        if thick > 0.5 {
            // Cut faces (seen when the slice is pulled out or the pie is not whole), then the
            // outer band over the part of the rim facing the viewer (6 o'clock side).
            for t in [a0, a0 + sweep] {
                let p = rim(ox, oy, t, 0.0);
                poly(ctx, vec![centre, p, [p[0], p[1] + thick], [centre[0], centre[1] + thick]], shade(fill, 0.6), None);
            }
            for lap in [0.0, TAU, 2.0 * TAU] {
                let (lo, hi) = ((PI / 2.0 + lap).max(a0), (1.5 * PI + lap).min(a0 + sweep));
                if hi - lo > 1e-4 {
                    let mut pts: Vec<[f32; 2]> = arc(lo, hi - lo).map(|t| rim(ox, oy, t, 0.0)).collect();
                    pts.extend(arc(lo, hi - lo).collect::<Vec<_>>().into_iter().rev().map(|t| rim(ox, oy, t, thick)));
                    poly(ctx, pts, shade(fill, SIDE_SHADE), None);
                }
            }
        }
        let mut top = vec![centre];
        top.extend(arc(a0, sweep).map(|t| rim(ox, oy, t, 0.0)));
        poly(ctx, top, fill, Some((WHITE, 1.0)));
        if chart.data_labels && vals.len() <= MAX_LABELS {
            let t = Fmt::new(&s.number_format).format(s.values.get(j).copied().flatten().unwrap_or(0.0), None);
            let p = [cx + ox + 0.65 * r * mid.sin(), cy + oy - 0.65 * r * tilt * mid.cos()];
            if sweep * r * 0.65 >= LABEL_SIZE && ctx.tw(&t, LABEL_SIZE) <= r {
                labels.push((p, t, on(fill)));
            }
        }
    }
    for ([x, y], t, c) in labels {
        ctx.text(x, y, &t, LABEL_SIZE, c, (HAlign::Center, VAlign::Middle));
    }
}
