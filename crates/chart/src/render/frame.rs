use gridcraft_core::number_to_text;
use gridcraft_model::{AxisCrosses, DisplayUnit, DisplayUnits};

use super::{AXIS, AXIS_TITLE_SIZE, Chart, ChartData, Ctx, DARK_TEXT, Fmt, GRID, HAlign, LABEL_SIZE, PI, Prim, Rgba, Scale, TEXT, VAlign, WHITE};

#[derive(Clone, Copy, Debug)]
pub(super) struct Bx {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Bx {
    pub(super) fn right(&self) -> f32 {
        self.x + self.w
    }
    pub(super) fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub(super) fn ok(&self) -> bool {
        self.w.is_finite() && self.h.is_finite() && self.w > 4.0 && self.h > 4.0
    }
    pub(super) fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub(super) fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
}

/// Text colour readable on `fill`.
pub(super) fn on(fill: Rgba) -> Rgba {
    let l = 0.299 * fill[0] as f32 + 0.587 * fill[1] as f32 + 0.114 * fill[2] as f32;
    if l < 150.0 { WHITE } else { DARK_TEXT }
}

pub(super) fn chart_title(chart: &Chart, data: &ChartData) -> Option<String> {
    match &chart.title {
        Some(t) => Some(t.trim().to_string()).filter(|t| !t.is_empty()),
        None if data.series.len() == 1 => data.series.first().map(|s| s.name.clone()).filter(|t| !t.is_empty()),
        None => None,
    }
}

pub(super) fn axis_titles(ctx: &mut Ctx, chart: &Chart, mut area: Bx) -> Bx {
    let lh = AXIS_TITLE_SIZE * 1.4 + 2.0;
    if let Some(t) = chart.x_title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let t = ctx.fit(t, AXIS_TITLE_SIZE, area.w);
        ctx.text(area.cx(), area.bottom(), &t, AXIS_TITLE_SIZE, TEXT, (HAlign::Center, VAlign::Bottom));
        area.h -= lh;
    }
    if let Some(t) = chart.y_title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let t = ctx.fit(t, AXIS_TITLE_SIZE, area.h);
        if !t.is_empty() {
            ctx.out.push(Prim::Text {
                x: area.x,
                y: area.cy(),
                text: t,
                size: AXIS_TITLE_SIZE,
                color: TEXT,
                bold: false,
                align: HAlign::Center,
                valign: VAlign::Top,
                rotation: -PI / 2.0,
            });
        }
        area.x += lh;
        area.w -= lh;
    }
    area
}

// ---------------------------------------------------------------- axes frame

pub(super) enum CatAxis<'a> {
    /// Category labels and, for a multi-level axis, the outer levels (nearest the labels first;
    /// one entry per category, non-empty where a group starts), and the interval between labels
    /// (`c:tickLblSkip`; `None` picks a step from the label widths).
    Labels(&'a [String], &'a [Vec<String>], Option<u32>),
    /// A data table supplies the labels below a vertical chart. Its stub must clear the plot.
    Table(&'a [String], f32),
    Numeric(Scale, &'a Fmt),
}

/// Groups of an outer category level: `(first, end)` index pairs, a group starting at every
/// non-empty entry (and at the first category).
pub(super) fn level_groups(level: &[String], n: usize) -> Vec<(usize, usize)> {
    let mut starts: Vec<usize> = (0..n).filter(|&j| j == 0 || level.get(j).is_some_and(|t| !t.is_empty())).collect();
    starts.push(n);
    starts.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?))).collect()
}

pub(super) struct ValAxis<'a> {
    pub(super) scale: Scale,
    pub(super) fmt: &'a Fmt,
    /// Display units: labels show values divided by the unit.
    pub(super) units: Option<DisplayUnits>,
    /// Where the category (or X) axis crosses this axis, in this axis's values.
    pub(super) cross: AxisCrosses,
    /// Where this axis crosses the category axis (a category number) or the X axis (an X value).
    pub(super) at: AxisCrosses,
}

impl<'a> ValAxis<'a> {
    pub(super) fn new(scale: Scale, fmt: &'a Fmt) -> Self {
        ValAxis { scale, fmt, units: None, cross: AxisCrosses::AutoZero, at: AxisCrosses::AutoZero }
    }
    fn factor(&self) -> f64 {
        self.units.map_or(1.0, |u| u.unit.factor())
    }
    fn labels(&self) -> Vec<(f64, String)> {
        let f = self.factor();
        self.scale.ticks().into_iter().map(|v| (v, self.fmt.format(v / f, Some(self.scale.step / f)))).collect()
    }
    /// The display units label, when shown.
    fn unit_label(&self) -> Option<String> {
        self.units.filter(|u| u.show_label).map(|u| display_unit_label(u.unit))
    }
}

/// The value at which the perpendicular axis crosses an axis with scale `s`; `None` is automatic
/// (zero, or the minimum when zero is outside the axis).
pub(super) fn cross_value(c: AxisCrosses, s: &Scale) -> Option<f64> {
    let (lo, hi) = (s.min.min(s.max), s.max.max(s.min));
    match c {
        AxisCrosses::AutoZero => None,
        AxisCrosses::Min => Some(s.min),
        AxisCrosses::Max => Some(s.max),
        AxisCrosses::At(v) if v.is_finite() => Some(v.clamp(lo, hi)),
        AxisCrosses::At(_) => None,
    }
}

/// Offset in category slots (0 = the first category's leading edge) where a value axis crossing
/// the category axis at `c` sits, for `n` categories (`crossBetween between`); `None` is automatic.
pub(super) fn cross_slot(c: AxisCrosses, n: usize) -> Option<f32> {
    match c {
        AxisCrosses::AutoZero | AxisCrosses::Min => None,
        AxisCrosses::Max => Some(n as f32),
        AxisCrosses::At(k) if k.is_finite() => Some((k - 1.0).clamp(0.0, n as f64) as f32),
        AxisCrosses::At(_) => None,
    }
}

/// Fraction along the category (or X) axis where the value axis sits; `None` is automatic.
fn at_frac(cat: &CatAxis, at: AxisCrosses) -> Option<f32> {
    match cat {
        CatAxis::Numeric(s, _) => cross_value(at, s).map(|v| s.frac(v)),
        CatAxis::Labels(l, _, _) | CatAxis::Table(l, _) => {
            let n = l.len().max(1);
            cross_slot(at, n).map(|k| k / n as f32)
        }
    }
}

/// The text of a display units label.
fn display_unit_label(u: DisplayUnit) -> String {
    match u {
        DisplayUnit::Hundreds => "Hundreds".into(),
        DisplayUnit::Thousands => "Thousands".into(),
        DisplayUnit::Millions => "Millions".into(),
        DisplayUnit::Billions => "Billions".into(),
        DisplayUnit::Trillions => "Trillions".into(),
        u => format!("x {}", number_to_text(u.factor())),
    }
}

/// The step between drawn category labels for a set interval `skip` (`c:tickLblSkip`) on `n`
/// categories: every `skip`-th label from the first, at least 1 and at most `n` (only the first).
fn label_step(skip: u32, n: usize) -> usize {
    usize::try_from(skip).unwrap_or(usize::MAX).clamp(1, n.max(1))
}

/// Draws gridlines, axis labels and the category axis line; returns the plot rectangle.
pub(super) fn frame(ctx: &mut Ctx, area: Bx, cat: CatAxis, y: &ValAxis, y2: Option<&ValAxis>, horizontal: bool, grid: bool) -> Bx {
    if horizontal { frame_horizontal(ctx, area, cat, y, grid) } else { frame_vertical(ctx, area, cat, y, y2, grid) }
}

fn max_label_w(ctx: &Ctx, l: &[(f64, String)]) -> f32 {
    l.iter().map(|(_, t)| ctx.tw(t, LABEL_SIZE)).fold(0.0f32, f32::max)
}

fn stub_w(cat: &CatAxis) -> f32 {
    match cat {
        CatAxis::Table(_, width) => *width,
        _ => 0.0,
    }
}

/// The value where the category axis line crosses `y`, and whether it was moved from automatic.
fn cat_axis_value(y: &ValAxis) -> (f64, bool) {
    match cross_value(y.cross, &y.scale) {
        Some(v) => (v, true),
        None => (if y.scale.min <= 0.0 && y.scale.max >= 0.0 { 0.0 } else { y.scale.min }, false),
    }
}

/// Columns, lines and the like: the value axis on the left (or where it crosses), categories below.
fn frame_vertical(ctx: &mut Ctx, area: Bx, cat: CatAxis, y: &ValAxis, y2: Option<&ValAxis>, grid: bool) -> Bx {
    let ls = LABEL_SIZE;
    let lh = ls * 1.4;
    let ylabels = y.labels();
    let unit_w = if y.unit_label().is_some() { lh } else { 0.0 };
    // An axis crossing its perpendicular one at the far end has its labels on the far side.
    let cat_far = y.cross == AxisCrosses::Max;
    let val_far = y.at == AxisCrosses::Max;
    let mut left = (max_label_w(ctx, &ylabels) + 6.0 + unit_w).min(area.w * 0.4).max(stub_w(&cat));
    let y2labels = y2.map(ValAxis::labels);
    let mut right = match (&y2labels, &cat) {
        (Some(l), _) => (max_label_w(ctx, l) + 6.0).min(area.w * 0.3),
        (None, CatAxis::Numeric(s, f)) => (ctx.tw(&f.format(s.max, Some(s.step)), ls) / 2.0).min(area.w * 0.2),
        _ => 4.0,
    };
    if val_far && y2.is_none() {
        std::mem::swap(&mut left, &mut right);
    }
    let levels = match &cat {
        CatAxis::Labels(_, o, _) => o.len() as f32,
        CatAxis::Numeric(..) | CatAxis::Table(..) => 0.0,
    };
    let below = lh + 4.0 + lh * levels;
    let top = if cat_far { below } else { lh * 0.5 };
    let plot = Bx { x: area.x + left, y: area.y + top, w: area.w - left - right, h: area.h - lh * 1.5 - 4.0 - lh * levels };
    if !plot.ok() {
        return plot;
    }
    let vx = at_frac(&cat, y.at).map(|f| plot.x + f * plot.w);
    vertical_value_labels(ctx, plot, y, &ylabels, vx, grid);
    if let (Some(a), Some(l)) = (y2, &y2labels) {
        for (v, t) in l {
            let yy = plot.bottom() - a.scale.frac(*v) * plot.h;
            ctx.text(plot.right() + 4.0, yy, t, ls, TEXT, (HAlign::Left, VAlign::Middle));
        }
    }
    let (cv, moved) = cat_axis_value(y);
    let y0 = plot.bottom() - y.scale.frac(cv) * plot.h;
    ctx.hline(plot.x, plot.right(), y0, AXIS, 0.75);
    // Category labels stay beside the category axis line when it is moved (below it, above it
    // at the far end); an automatic axis keeps them under the plot.
    let base = if moved { y0 } else { plot.bottom() };
    vertical_cat_labels(ctx, cat, plot, base, cat_far, vx);
    plot
}

/// Gridlines and value labels beside the value axis line `vx` (left of it, right when it is at the
/// far end; the plot's left edge when automatic), and the display units label beyond them.
fn vertical_value_labels(ctx: &mut Ctx, plot: Bx, y: &ValAxis, ylabels: &[(f64, String)], vx: Option<f32>, grid: bool) {
    let ls = LABEL_SIZE;
    let (lx, lalign) = match vx {
        Some(x) if y.at == AxisCrosses::Max => (x + 4.0, HAlign::Left),
        Some(x) => (x - 4.0, HAlign::Right),
        None => (plot.x - 4.0, HAlign::Right),
    };
    for (v, t) in ylabels {
        let yy = plot.bottom() - y.scale.frac(*v) * plot.h;
        if grid {
            ctx.hline(plot.x, plot.right(), yy, GRID, 0.75);
        }
        ctx.text(lx, yy, t, ls, TEXT, (lalign, VAlign::Middle));
    }
    if let Some(t) = y.unit_label() {
        let vw = max_label_w(ctx, ylabels);
        let t = ctx.fit(&t, ls, plot.h);
        let x = if lalign == HAlign::Left { lx + vw + 2.0 } else { lx - vw - 2.0 - ls * 1.4 };
        let yy = plot.y + ctx.tw(&t, ls) / 2.0;
        ctx.out.push(Prim::Text {
            x,
            y: yy,
            text: t,
            size: ls,
            color: TEXT,
            bold: false,
            align: HAlign::Center,
            valign: VAlign::Top,
            rotation: -PI / 2.0,
        });
    }
}

/// Category labels (and outer levels) beside the category axis line at `base`: below it, above it
/// when the axis is at the far end. A numeric (scatter) X axis also draws the Y axis line.
fn vertical_cat_labels(ctx: &mut Ctx, cat: CatAxis, plot: Bx, base: f32, far: bool, vx: Option<f32>) {
    let ls = LABEL_SIZE;
    let lh = ls * 1.4;
    let (dir, cvalign) = if far { (-1.0, VAlign::Bottom) } else { (1.0, VAlign::Top) };
    match cat {
        CatAxis::Table(..) => {}
        CatAxis::Labels(l, outer, skip) => {
            let n = l.len().max(1);
            let cw = plot.w / n as f32;
            let k = match skip {
                Some(s) => label_step(s, n),
                None => {
                    let lw = l.iter().take(2000).map(|t| ctx.tw(t, ls)).fold(0.0f32, f32::max);
                    (((lw + 6.0) / cw.max(1e-3)).ceil() as usize).clamp(1, n)
                }
            };
            for (j, t) in l.iter().enumerate().step_by(k) {
                let t = ctx.fit(t, ls, cw * k as f32 - 2.0);
                ctx.text(plot.x + (j as f32 + 0.5) * cw, base + dir * 3.0, &t, ls, TEXT, (HAlign::Center, cvalign));
            }
            // Outer levels, one row each beyond the labels: a group's name centred under its
            // categories, and a separator at each group boundary down through the rows.
            for (i, level) in outer.iter().enumerate() {
                let row = base + dir * (3.0 + lh * (i as f32 + 1.0));
                let end = row + dir * (lh - 2.0);
                for (first, last) in level_groups(level, n) {
                    let (x0, x1) = (plot.x + first as f32 * cw, plot.x + last as f32 * cw);
                    if let Some(t) = level.get(first).filter(|t| !t.is_empty()) {
                        let t = ctx.fit(t, ls, x1 - x0 - 2.0);
                        ctx.text((x0 + x1) / 2.0, row, &t, ls, TEXT, (HAlign::Center, cvalign));
                    }
                    if first > 0 {
                        ctx.vline(x0, base, end, AXIS, 0.75);
                    }
                }
                ctx.vline(plot.right(), base, end, AXIS, 0.75);
            }
        }
        CatAxis::Numeric(s, f) => {
            let ticks = s.ticks();
            let labels: Vec<String> = ticks.iter().map(|v| f.format(*v, Some(s.step))).collect();
            let lw = labels.iter().map(|t| ctx.tw(t, ls)).fold(0.0f32, f32::max);
            let slot = plot.w / ticks.len().max(1) as f32;
            let k = (((lw + 6.0) / slot.max(1e-3)).ceil() as usize).max(1);
            for (v, t) in ticks.iter().zip(&labels).step_by(k) {
                let xx = plot.x + s.frac(*v) * plot.w;
                ctx.text(xx, base + dir * 3.0, t, ls, TEXT, (HAlign::Center, cvalign));
            }
            if let Some(x) = vx {
                ctx.vline(x, plot.y, plot.bottom(), AXIS, 0.75);
            } else if s.min <= 0.0 && s.max >= 0.0 {
                let xx = plot.x + s.frac(0.0) * plot.w;
                ctx.vline(xx, plot.y, plot.bottom(), AXIS, 0.75);
            }
        }
    }
}

/// Bar charts: categories up the left (or where the category axis crosses), values along the bottom.
fn frame_horizontal(ctx: &mut Ctx, area: Bx, cat: CatAxis, y: &ValAxis, grid: bool) -> Bx {
    let ls = LABEL_SIZE;
    let lh = ls * 1.4;
    let ylabels = y.labels();
    let unit_w = if y.unit_label().is_some() { lh } else { 0.0 };
    let cat_far = y.cross == AxisCrosses::Max;
    let val_far = y.at == AxisCrosses::Max;
    let stub = stub_w(&cat);
    let (labels, outer, skip): (Vec<String>, &[Vec<String>], Option<u32>) = match cat {
        CatAxis::Labels(l, o, s) => (l.to_vec(), o, s),
        CatAxis::Table(l, _) => (l.to_vec(), &[], None),
        CatAxis::Numeric(..) => (vec![], &[], None),
    };
    let lw = (labels.iter().take(2000).map(|t| ctx.tw(t, ls)).fold(0.0f32, f32::max) + 6.0).min(area.w * 0.35);
    // Outer levels stand in columns beside the labels, each as wide as its longest name.
    let level_w: Vec<f32> =
        outer.iter().map(|l| (l.iter().take(2000).map(|t| ctx.tw(t, ls)).fold(0.0f32, f32::max) + 6.0).min(area.w * 0.15)).collect();
    let levels_w: f32 = level_w.iter().sum();
    let mut right = ylabels.last().map(|(_, t)| ctx.tw(t, ls) / 2.0).unwrap_or(0.0).min(area.w * 0.2) + 2.0;
    let mut left = (lw + levels_w).max(stub);
    if cat_far {
        std::mem::swap(&mut left, &mut right);
    }
    let vh = lh + 6.0 + unit_w;
    let top = if val_far { vh - 2.0 } else { 2.0 };
    let plot = Bx { x: area.x + left, y: area.y + top, w: area.w - left - right, h: area.h - vh };
    if !plot.ok() {
        return plot;
    }
    let n = labels.len().max(1);
    let ch = plot.h / n as f32;
    horizontal_value_labels(ctx, plot, y, &ylabels, cross_slot(y.at, n).map(|k| plot.bottom() - k * ch), grid);
    let (cv, moved) = cat_axis_value(y);
    let x0 = plot.x + y.scale.frac(cv) * plot.w;
    ctx.vline(x0, plot.y, plot.bottom(), AXIS, 0.75);
    // Category labels beside the moved category axis (left of it, right at the far end).
    let base = if moved { x0 } else { plot.x };
    let (dir, calign) = if cat_far { (1.0, HAlign::Left) } else { (-1.0, HAlign::Right) };
    let k = match skip {
        Some(s) => label_step(s, n),
        None => ((lh / ch.max(1e-3)).ceil() as usize).clamp(1, n),
    };
    for (j, t) in labels.iter().enumerate().step_by(k) {
        let t = ctx.fit(t, ls, lw - 6.0);
        ctx.text(base + dir * 4.0, plot.bottom() - (j as f32 + 0.5) * ch, &t, ls, TEXT, (calign, VAlign::Middle));
    }
    // Outer levels: a group's name centred beside its categories, separators at the
    // boundaries running to the axis.
    let mut x1 = base + dir * lw;
    for (level, w) in outer.iter().zip(&level_w) {
        let x0 = x1 + dir * w;
        for (first, last) in level_groups(level, n) {
            let (y0, y1) = (plot.bottom() - last as f32 * ch, plot.bottom() - first as f32 * ch);
            if let Some(t) = level.get(first).filter(|t| !t.is_empty()) {
                let t = ctx.fit(t, ls, w - 4.0);
                ctx.text((x0 + x1) / 2.0, (y0 + y1) / 2.0, &t, ls, TEXT, (HAlign::Center, VAlign::Middle));
            }
            if first > 0 {
                ctx.hline(x0.min(base), x0.max(base), y1, AXIS, 0.75);
            }
        }
        ctx.hline(x0.min(base), x0.max(base), plot.y, AXIS, 0.75);
        x1 = x0;
    }
    plot
}

/// Gridlines and value labels of a bar chart: under the plot, or beside the value axis line `vy`
/// (below it, above it at the far end), with the display units label beyond them on the right.
fn horizontal_value_labels(ctx: &mut Ctx, plot: Bx, y: &ValAxis, ylabels: &[(f64, String)], vy: Option<f32>, grid: bool) {
    let ls = LABEL_SIZE;
    let (ly, lvalign) = match vy {
        Some(yy) if y.at == AxisCrosses::Max => (yy - 3.0, VAlign::Bottom),
        Some(yy) => (yy + 3.0, VAlign::Top),
        None => (plot.bottom() + 3.0, VAlign::Top),
    };
    let slot = plot.w / ylabels.len().max(1) as f32;
    let k = (((max_label_w(ctx, ylabels) + 6.0) / slot.max(1e-3)).ceil() as usize).max(1);
    for (i, (v, t)) in ylabels.iter().enumerate() {
        let xx = plot.x + y.scale.frac(*v) * plot.w;
        if grid {
            ctx.vline(xx, plot.y, plot.bottom(), GRID, 0.75);
        }
        if i % k == 0 {
            ctx.text(xx, ly, t, ls, TEXT, (HAlign::Center, lvalign));
        }
    }
    if let Some(t) = y.unit_label() {
        let t = ctx.fit(&t, ls, plot.w);
        let lh = ls * 1.4;
        let yy = if lvalign == VAlign::Top { ly + lh } else { ly - lh };
        ctx.text(plot.right(), yy, &t, ls, TEXT, (HAlign::Right, lvalign));
    }
}

pub(super) fn value_range(it: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    it.fold(None, |acc, v| match acc {
        None => Some((v, v)),
        Some((a, b)) => Some((a.min(v), b.max(v))),
    })
}
