use super::draw::{Block, PdfMeasure, Xf, draw_prims, rgb_of};
use super::text::wrap_lines;
use super::{Anchor, Font, Page, RangeRef, Rect, ShapeKind, Sheet, Value, Workbook};

/// Cell pictures fit the current cell (or merged-cell) rectangle without changing aspect ratio.
pub(super) fn draw_cell_picture(pg: &mut Page, data: &[u8], rect: (f32, f32, f32, f32), k: f32) {
    let (x, y, w, h) = rect;
    let pad = 2.0 * k;
    let (w, h) = ((w - 2.0 * pad).max(0.0), (h - 2.0 * pad).max(0.0));
    if w <= 0.0 || h <= 0.0 || data.len() > 16 * 1024 * 1024 {
        return;
    }
    let Some((iw, ih)) = crate::io::image_size(data) else { return };
    if iw == 0 || ih == 0 || iw > 8192 || ih > 8192 || u64::from(iw) * u64::from(ih) > 16_000_000 {
        return;
    }
    let Ok(mut reader) = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format() else { return };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let Ok(image) = reader.decode() else { return };
    let image =
        image.thumbnail(((w / 72.0) * 150.0).ceil().clamp(1.0, 4000.0) as u32, ((h / 72.0) * 150.0).ceil().clamp(1.0, 4000.0) as u32).to_rgba8();
    let fit = (w / iw as f32).min(h / ih as f32);
    let (pw, ph) = (iw as f32 * fit, ih as f32 * fit);
    pg.image_rgba(Rect::new(x + pad + (w - pw) / 2.0, y + pad + (h - ph) / 2.0, pw, ph), image.as_raw(), [image.width(), image.height()]);
}

/// Sheet pixel rectangle of an anchor.
/// Most source lines printed for an equation that has no layout.
pub(super) const MAX_FALLBACK_LINES: usize = 500;

/// The lines of `text` printed in place of an equation that has no layout: those of its first
/// `MAX_SOURCE_BYTES` (a longer source never lays out, and a file can carry megabytes) that fit a
/// frame `h` points tall at `line_h` points per line (at least the first), and never more than
/// [`MAX_FALLBACK_LINES`]. A single `pg.text` call would print every newline as `?`.
pub(super) fn fallback_lines(text: &str, h: f32, line_h: f32) -> impl Iterator<Item = &str> {
    let text = crate::cmd::equation::source_excerpt(text);
    let fit = h / line_h;
    let n = if fit >= MAX_FALLBACK_LINES as f32 {
        MAX_FALLBACK_LINES
    } else if fit >= 1.0 {
        fit as usize
    } else {
        1
    };
    text.lines().take(n)
}

fn anchor_rect(sh: &Sheet, a: &Anchor) -> (f64, f64, f64, f64) {
    (sh.col_left(a.cell.col) + a.dx as f64, sh.row_top(a.cell.row) + a.dy as f64, a.width.max(0.0) as f64, a.height.max(0.0) as f64)
}

/// Decodes an embedded picture to RGBA, downscaled to at most `max_w`×`max_h` pixels.
fn decode_image(data: &[u8], max_w: u32, max_h: u32) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = crate::io::image_size(data)?;
    if w == 0 || h == 0 || w as u64 * h as u64 > 60_000_000 {
        return None;
    }
    let img = image::load_from_memory(data).ok()?;
    let img = if img.width() > max_w.max(1) || img.height() > max_h.max(1) { img.thumbnail(max_w.max(1), max_h.max(1)) } else { img };
    let rgba = img.to_rgba8();
    let (pw, ph) = (rgba.width(), rgba.height());
    Some((rgba.into_raw(), pw, ph))
}

/// Draws charts, pictures, shapes and sparklines overlapping the body block of a page.
/// `pics` memoizes decoded pictures for the whole render (one page at a time draws each
/// picture again, but the decode happens once).
pub(super) fn draw_objects(pg: &mut Page, wb: &Workbook, si: usize, sh: &Sheet, b: &Block, pics: &mut super::PicCache) {
    let block = b.range();
    let (sx0, sy0) = (sh.col_left(block.start.col), sh.row_top(block.start.row));
    let (sx1, sy1) = (sh.col_left(block.end.col.saturating_add(1)), sh.row_top(block.end.row.saturating_add(1)));
    let (bx, by) = (b.xs.first().copied().unwrap_or(0.0), b.ys.first().copied().unwrap_or(0.0));
    let (bw, bh) = (b.xs.last().copied().unwrap_or(bx) - bx, b.ys.last().copied().unwrap_or(by) - by);
    let visible = |r: (f64, f64, f64, f64)| r.2 > 0.0 && r.3 > 0.0 && r.0 < sx1 && r.0 + r.2 > sx0 && r.1 < sy1 && r.1 + r.3 > sy0;
    let place = |r: (f64, f64, f64, f64)| (bx + ((r.0 - sx0) as f32) * b.k, by + ((r.1 - sy0) as f32) * b.k, r.2 as f32 * b.k, r.3 as f32 * b.k);
    pg.save();
    pg.clip_rect(bx, by, bw, bh);
    draw_sparklines(pg, wb, si, sh, b, block);
    // A linked picture without a cached copy has no bytes to draw.
    for (imi, im) in sh.images.iter().enumerate().filter(|(_, o)| !o.hidden && !o.data.is_empty()) {
        let r = anchor_rect(sh, &im.anchor);
        if !visible(r) {
            continue;
        }
        let (x, y, w, h) = place(r);
        // About 150 dpi is plenty for print.
        let (mw, mh) = (((w / 72.0) * 150.0).ceil().clamp(1.0, 4000.0) as u32, ((h / 72.0) * 150.0).ceil().clamp(1.0, 4000.0) as u32);
        let decoded = pics.entry((si, imi)).or_insert_with(|| decode_image(&im.data, mw, mh));
        if let Some((px, pw, ph)) = decoded.as_ref() {
            pg.image_rgba(Rect::new(x, y, w, h), px, [*pw, *ph]);
        }
    }
    for shp in sh.shapes.iter().filter(|o| !o.hidden) {
        let r = anchor_rect(sh, &shp.anchor);
        if !visible(r) {
            continue;
        }
        let (x, y, w, h) = place(r);
        if shp.kind == ShapeKind::Equation {
            let ink = rgb_of(shp.line, wb).unwrap_or([0, 0, 0]);
            match shp.math.get(&shp.text) {
                Some(Ok(list)) => crate::cmd::equation::draw_pdf(pg, list, (x, y, w, h), crate::cmd::equation::PADDING * b.k, ink),
                _ => {
                    // No layout (the source does not parse, or the file was past the layout
                    // budget): the source, one text run per line, as many as the frame holds.
                    let line_h = 12.0 * b.scale;
                    // A long line stays inside the frame; this clip nests in the block's.
                    pg.save();
                    pg.clip_rect(x, y, w, h);
                    for (i, line) in fallback_lines(&shp.text, h, line_h).enumerate() {
                        pg.text(x + 2.0 * b.k, y + line_h * (i + 1) as f32, 10.0 * b.scale, Font::Courier, ink, line);
                    }
                    pg.restore();
                }
            }
            continue;
        }
        draw_shape(pg, wb, shp, b, (x, y, w, h));
    }
    for ch in sh.charts.iter().filter(|o| !o.hidden) {
        let r = anchor_rect(sh, &ch.anchor);
        if !visible(r) {
            continue;
        }
        let (x, y, _, _) = place(r);
        let data = gridcraft_chart::resolve(wb, si, ch);
        let prims = gridcraft_chart::render(ch, &data, r.2 as f32, r.3 as f32, &PdfMeasure);
        draw_prims(pg, &prims, Xf { ox: x, oy: y, k: b.k });
    }
    pg.restore();
}

fn draw_sparklines(pg: &mut Page, wb: &Workbook, si: usize, sh: &Sheet, b: &Block, block: RangeRef) {
    for sp in &sh.sparklines {
        if !block.contains(sp.cell) {
            continue;
        }
        let vals: Vec<Option<f64>> = match gridcraft_calc::evaluate(wb, si, sp.cell, &sp.source) {
            Value::Array(a) => a.iter().take(10_000).map(Value::as_f64).collect(),
            v => vec![v.as_f64()],
        };
        let color = rgb_of(sp.color, wb).unwrap_or([0x15, 0x60, 0x82]);
        let (x, y, w, h) = b.rect(RangeRef::cell(sp.cell));
        let (wpx, hpx) = (w / b.k, h / b.k);
        let prims = gridcraft_chart::render_sparkline(sp.kind, &vals, [color[0], color[1], color[2], 255], sp.markers, wpx, hpx);
        draw_prims(pg, &prims, Xf { ox: x, oy: y, k: b.k });
    }
}

fn draw_shape(pg: &mut Page, wb: &Workbook, shp: &gridcraft_model::Shape, b: &Block, (x, y, w, h): (f32, f32, f32, f32)) {
    let fill = rgb_of(shp.fill, wb);
    let line = rgb_of(shp.line, wb).filter(|_| !shp.format.no_line).map(|c| (c, (0.75 * b.k).max(0.25)));
    match shp.kind {
        ShapeKind::Ellipse => {
            let pts: Vec<[f32; 2]> = (0..72)
                .map(|i| {
                    let a = i as f32 / 72.0 * std::f32::consts::TAU;
                    [x + w / 2.0 + w / 2.0 * a.cos(), y + h / 2.0 + h / 2.0 * a.sin()]
                })
                .collect();
            pg.polygon(&pts, fill, line);
        }
        ShapeKind::Triangle => pg.polygon(&[[x + w / 2.0, y], [x + w, y + h], [x, y + h]], fill, line),
        ShapeKind::Line | ShapeKind::Arrow => {
            let c = line.map(|l| l.0).or(fill).unwrap_or([0, 0, 0]);
            pg.stroke_line(x, y, x + w, y + h, (1.0 * b.k).max(0.3), c);
            if shp.kind == ShapeKind::Arrow {
                let ang = h.atan2(w);
                let al = 8.0 * b.k;
                let p = |da: f32| [x + w - al * (ang + da).cos(), y + h - al * (ang + da).sin()];
                pg.polygon(&[[x + w, y + h], p(0.4), p(-0.4)], Some(c), None);
            }
        }
        ShapeKind::RoundedRectangle => pg.polygon(&rounded_rect_points((x, y, w, h)), fill, line),
        ShapeKind::Freeform if draw_freeform(pg, shp, b, (x, y, w, h), fill, line) => {}
        _ if draw_preset(pg, shp, b, (x, y, w, h), fill, line) => {}
        _ => {
            pg.polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], fill, line);
        }
    }
    if shp.text.is_empty() {
        return;
    }
    use gridcraft_model::{ShapeTextAlign, ShapeTextAnchor};
    let fm = &shp.format;
    let text_box = shp.kind == ShapeKind::TextBox;
    let size = fm.font_size.unwrap_or(11.0).clamp(1.0, 4000.0) * b.scale;
    let font = Font::styled(Font::family_for(fm.font.as_deref().unwrap_or("")), fm.bold, fm.italic);
    let color = fm.text_color.and_then(|c| rgb_of(c, wb)).unwrap_or([0, 0, 0]);
    let align = fm.align.unwrap_or(if text_box { ShapeTextAlign::Left } else { ShapeTextAlign::Center });
    let anchor = fm.anchor.unwrap_or(if text_box { ShapeTextAnchor::Top } else { ShapeTextAnchor::Middle });
    let lines = wrap_lines(&shp.text, font, size, (w - 4.0 * b.k).max(size));
    let block_h = lines.len().min(500) as f32 * size * 1.2;
    let mut yy = match anchor {
        ShapeTextAnchor::Top => y + 2.0 * b.k,
        ShapeTextAnchor::Middle => y + (h - block_h) / 2.0,
        ShapeTextAnchor::Bottom => y + h - 2.0 * b.k - block_h,
    } + size * 0.8;
    for l in lines.iter().take(500) {
        let lw = gridcraft_pdf::text_width(font, size, l);
        let lx = match align {
            ShapeTextAlign::Left => x + 2.0 * b.k,
            ShapeTextAlign::Center => x + (w - lw) / 2.0,
            ShapeTextAlign::Right => x + w - 2.0 * b.k - lw,
        };
        pg.text(lx, yy, size, font, color, l);
        yy += size * 1.2;
    }
}

/// Most points one shape's outline flattens into, so a hostile path can't exhaust memory.
const MAX_OUTLINE_POINTS: usize = 200_000;

/// A rounded rectangle (`roundRect`, ECMA-376 Part 1 §20.1.10.56) with the default corner radius,
/// 16667/100000 of the shorter side, each corner a quarter ellipse of 6 segments.
fn rounded_rect_points((x, y, w, h): (f32, f32, f32, f32)) -> Vec<[f32; 2]> {
    const SEGS: usize = 6;
    let r = 0.16667 * w.abs().min(h.abs());
    // Corner centres and the angle each quarter arc starts at, clockwise from the top right.
    let corners = [(x + w - r, y + r, -90.0f32), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 90.0), (x + r, y + r, 180.0)];
    let mut pts = Vec::with_capacity(4 * (SEGS + 1));
    for (cx, cy, start) in corners {
        for i in 0..=SEGS {
            let a = (start + 90.0 * i as f32 / SEGS as f32).to_radians();
            pts.push([cx + r * a.cos(), cy + r * a.sin()]);
        }
    }
    pts
}

/// Draws a freeform's custom geometry paths (`a:custGeom`, §20.1.9.8) flattened into polygons and
/// polylines; false when there is nothing to draw, so the caller prints the box instead.
fn draw_freeform(
    pg: &mut Page,
    shp: &gridcraft_model::Shape,
    b: &Block,
    rect: (f32, f32, f32, f32),
    fill: Option<[u8; 3]>,
    line: Option<([u8; 3], f32)>,
) -> bool {
    let Some(geom) = shp.geometry.as_ref() else { return false };
    draw_paths(pg, &geom.paths, b, rect, fill, line)
}

/// Draws a preset shape kept in `geometry` (block arrows, chevron, diamond, …) by its own outline
/// from [`gridcraft_model::preset_paths`]; false for a preset without one, so the caller prints
/// the box instead. The outline's resolution is the box in hundredths of a point.
fn draw_preset(
    pg: &mut Page,
    shp: &gridcraft_model::Shape,
    b: &Block,
    rect: (f32, f32, f32, f32),
    fill: Option<[u8; 3]>,
    line: Option<([u8; 3], f32)>,
) -> bool {
    let Some(geom) = shp.geometry.as_ref() else { return false };
    let res = |v: f32| (v.abs() * 100.0).round().min(1.0e9) as i64;
    let Some(paths) = gridcraft_model::preset_paths(geom, res(rect.2), res(rect.3)) else { return false };
    draw_paths(pg, &paths, b, rect, fill, line)
}

/// Draws geometry paths flattened into polygons and polylines on the shape box; false when there
/// is nothing to draw.
fn draw_paths(
    pg: &mut Page,
    paths: &[gridcraft_model::GeometryPath],
    b: &Block,
    rect: (f32, f32, f32, f32),
    fill: Option<[u8; 3]>,
    line: Option<([u8; 3], f32)>,
) -> bool {
    let mut budget = MAX_OUTLINE_POINTS;
    let mut drew = false;
    for path in paths {
        let line = line.filter(|_| path.stroke);
        for sub in flatten_path(path, rect, b.k, &mut budget) {
            if sub.points.len() < 2 {
                continue;
            }
            drew = true;
            if path.fill {
                pg.polygon(&sub.points, fill.map(|c| path.shade.apply(c)), line);
            } else if let Some((c, lw)) = line {
                if sub.closed {
                    pg.polygon(&sub.points, None, Some((c, lw)));
                } else {
                    pg.polyline(&sub.points, lw, c, &[]);
                }
            }
        }
    }
    drew
}

struct Subpath {
    points: Vec<[f32; 2]>,
    closed: bool,
}

/// Flattens one geometry path into subpaths on the shape box, path coordinates stretched from the
/// path's `w` × `h` (the shape's extent in EMU when omitted). Spends at most `budget` points.
fn flatten_path(path: &gridcraft_model::GeometryPath, (x, y, w, h): (f32, f32, f32, f32), k: f32, budget: &mut usize) -> Vec<Subpath> {
    use gridcraft_model::PathCommand;
    let emu = |len: f32| f64::from(len) / f64::from(k) * 12_700.0;
    let pw = if path.w > 0 { path.w as f64 } else { emu(w) };
    let ph = if path.h > 0 { path.h as f64 } else { emu(h) };
    let (sx, sy) = (f64::from(w) / pw, f64::from(h) / ph);
    let map = |p: (f64, f64)| [(f64::from(x) + p.0 * sx) as f32, (f64::from(y) + p.1 * sy) as f32];
    let mut out: Vec<Subpath> = Vec::new();
    let mut cur: Vec<[f32; 2]> = Vec::new();
    // The current point and the current subpath's start, in path coordinates.
    let mut pos = (0.0f64, 0.0f64);
    let mut start = pos;
    let push = |cur: &mut Vec<[f32; 2]>, budget: &mut usize, p: (f64, f64)| {
        if *budget > 0 {
            *budget -= 1;
            cur.push(map(p));
        }
    };
    for cmd in &path.commands {
        if *budget == 0 {
            break;
        }
        // A drawing command after a close (or before any move) continues from the current point.
        if cur.is_empty() && !matches!(cmd, PathCommand::MoveTo(..) | PathCommand::Close) {
            push(&mut cur, budget, pos);
            start = pos;
        }
        match *cmd {
            PathCommand::MoveTo(px, py) => {
                if !cur.is_empty() {
                    out.push(Subpath { points: std::mem::take(&mut cur), closed: false });
                }
                pos = (px as f64, py as f64);
                start = pos;
                push(&mut cur, budget, pos);
            }
            PathCommand::LineTo(px, py) => {
                pos = (px as f64, py as f64);
                push(&mut cur, budget, pos);
            }
            PathCommand::CubicBezTo([c1, c2, e]) => {
                let (p0, p1, p2, p3) = (pos, (c1.0 as f64, c1.1 as f64), (c2.0 as f64, c2.1 as f64), (e.0 as f64, e.1 as f64));
                for i in 1..=8 {
                    let t = f64::from(i) / 8.0;
                    let u = 1.0 - t;
                    let (a, bb, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    push(&mut cur, budget, (a * p0.0 + bb * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + bb * p1.1 + c * p2.1 + d * p3.1));
                }
                pos = p3;
            }
            PathCommand::QuadBezTo([c1, e]) => {
                let (p0, p1, p2) = (pos, (c1.0 as f64, c1.1 as f64), (e.0 as f64, e.1 as f64));
                for i in 1..=8 {
                    let t = f64::from(i) / 8.0;
                    let u = 1.0 - t;
                    let (a, bb, c) = (u * u, 2.0 * u * t, t * t);
                    push(&mut cur, budget, (a * p0.0 + bb * p1.0 + c * p2.0, a * p0.1 + bb * p1.1 + c * p2.1));
                }
                pos = p2;
            }
            PathCommand::ArcTo { w_r, h_r, st_ang, sw_ang } => {
                let (wr, hr) = (w_r as f64, h_r as f64);
                let st = (st_ang as f64 / 60_000.0).to_radians();
                let sw_deg = sw_ang as f64 / 60_000.0;
                let sw = sw_deg.to_radians();
                let (cx, cy) = (pos.0 - wr * st.cos(), pos.1 - hr * st.sin());
                // One segment per 15° of sweep, at least 4; a hostile sweep is capped by the budget.
                let segs = (sw_deg.abs() / 15.0).ceil().clamp(4.0, 1440.0) as u32;
                for i in 1..=segs {
                    let a = st + sw * f64::from(i) / f64::from(segs);
                    pos = (cx + wr * a.cos(), cy + hr * a.sin());
                    push(&mut cur, budget, pos);
                }
            }
            PathCommand::Close => {
                if !cur.is_empty() {
                    out.push(Subpath { points: std::mem::take(&mut cur), closed: true });
                }
                pos = start;
            }
        }
    }
    if !cur.is_empty() {
        out.push(Subpath { points: cur, closed: false });
    }
    out
}
