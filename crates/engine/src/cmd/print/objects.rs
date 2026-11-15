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

/// A picture's border (Picture Border) for print: its colour and a solid stroke width in points
/// (`w` / 12700, 0.75 pt without one), scaled like every other printed length. The stroke is centred
/// on the picture's edge; dashes are not drawn.
pub(super) fn picture_border_stroke(wb: &Workbook, im: &gridcraft_model::Image, scale: f32) -> Option<(super::Rgb, f32)> {
    let b = im.border.as_ref()?;
    let c = rgb_of(b.color, wb)?;
    Some((c, (b.width_pt().max(0.0) * scale).clamp(0.0, 1600.0)))
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
    // An object moves with the row it is anchored to (rows print at their printed pitch); its
    // own size scales like every printed length.
    let place =
        |a: &Anchor, r: (f64, f64, f64, f64)| (bx + ((r.0 - sx0) as f32) * b.k, b.y(a.cell.row) + a.dy * b.k, r.2 as f32 * b.k, r.3 as f32 * b.k);
    pg.save();
    pg.clip_rect(bx, by, bw, bh);
    draw_sparklines(pg, wb, si, sh, b, block);
    // A linked picture without a cached copy has no bytes to draw.
    for (imi, im) in sh.images.iter().enumerate().filter(|(_, o)| !o.hidden && !o.no_print && !o.data.is_empty()) {
        let r = anchor_rect(sh, &im.anchor);
        if !visible(r) {
            continue;
        }
        let (x, y, w, h) = place(&im.anchor, r);
        // About 150 dpi is plenty for print.
        let (mw, mh) = (((w / 72.0) * 150.0).ceil().clamp(1.0, 4000.0) as u32, ((h / 72.0) * 150.0).ceil().clamp(1.0, 4000.0) as u32);
        let decoded = pics.decoded.entry((si, imi)).or_insert_with(|| decode_image(&im.data, mw, mh));
        if let Some((px, pw, ph)) = decoded.as_ref() {
            // Every page the picture touches embeds its own copy: past the budget it is left out.
            let bytes = px.len();
            if pics.image_bytes.saturating_add(bytes) <= super::MAX_EMBEDDED_IMAGE_BYTES {
                pics.image_bytes += bytes;
                pg.image_rgba(Rect::new(x, y, w, h), px, [*pw, *ph]);
            }
        }
        if let Some((c, width)) = picture_border_stroke(wb, im, b.scale) {
            pg.rect_stroke(x, y, w, h, width, c);
        }
    }
    for shp in sh.shapes.iter().filter(|o| !o.hidden && !o.no_print) {
        let r = anchor_rect(sh, &shp.anchor);
        if !visible(r) {
            continue;
        }
        let (x, y, w, h) = place(&shp.anchor, r);
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
    for (chi, ch) in sh.charts.iter().enumerate().filter(|(_, o)| !o.hidden && !o.no_print) {
        let r = anchor_rect(sh, &ch.anchor);
        if !visible(r) {
            continue;
        }
        let (x, y, _, _) = place(&ch.anchor, r);
        // As for pictures, each page gets its own copy of the drawing commands.
        let prims = pics.chart((si, chi), || {
            let data = gridcraft_chart::resolve(wb, si, ch);
            gridcraft_chart::render(ch, &data, r.2 as f32, r.3 as f32, &PdfMeasure)
        });
        if let Some(prims) = prims {
            draw_prims(pg, prims, Xf { ox: x, oy: y, k: b.k });
        }
    }
    // Form controls lie on top of charts, pictures and shapes.
    for c in sh.controls.iter().filter(|o| o.print) {
        let r = anchor_rect(sh, &c.anchor);
        if !visible(r) {
            continue;
        }
        draw_control(pg, c, b, place(&c.anchor, r));
    }
    pg.restore();
}

/// Draws a form control with "Print object" on: its box, mark and caption.
fn draw_control(pg: &mut Page, c: &gridcraft_model::FormControl, b: &Block, (x, y, w, h): (f32, f32, f32, f32)) {
    use gridcraft_model::ControlKind;
    const INK: [u8; 3] = [0, 0, 0];
    const EDGE: [u8; 3] = [0x80, 0x80, 0x80];
    let thin = (0.5 * b.k).max(0.25);
    let rect = |x: f32, y: f32, w: f32, h: f32| [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    let size = 8.0 * b.scale;
    let font = Font::Helvetica;
    // The caption on one line, clipped to the control.
    let caption = |pg: &mut Page, tx: f32, ty: f32| {
        if c.text.is_empty() {
            return;
        }
        let line = c.text.lines().next().unwrap_or("");
        pg.save();
        pg.clip_rect(x, y, w, h);
        pg.text(tx, ty, size, font, INK, line);
        pg.restore();
    };
    let mid = y + h / 2.0 + size * 0.35;
    let mark = (9.0 * b.k).min(h.max(0.0)).min(w.max(0.0));
    let (mx, my) = (x + 2.0 * b.k, y + (h - mark) / 2.0);
    match c.kind {
        ControlKind::CheckBox => {
            pg.polygon(&rect(mx, my, mark, mark), Some([255, 255, 255]), Some((INK, thin)));
            match c.value {
                1 => pg.polyline(
                    &[[mx + mark * 0.2, my + mark * 0.5], [mx + mark * 0.42, my + mark * 0.75], [mx + mark * 0.8, my + mark * 0.25]],
                    (1.0 * b.k).max(0.3),
                    INK,
                    &[],
                ),
                2 => pg.polygon(&rect(mx + mark * 0.25, my + mark * 0.25, mark * 0.5, mark * 0.5), Some(EDGE), None),
                _ => {}
            }
            caption(pg, mx + mark + 3.0 * b.k, mid);
        }
        ControlKind::OptionButton => {
            let circle = |cx: f32, cy: f32, r: f32| -> Vec<[f32; 2]> {
                (0..36)
                    .map(|i| {
                        let a = i as f32 / 36.0 * std::f32::consts::TAU;
                        [cx + r * a.cos(), cy + r * a.sin()]
                    })
                    .collect()
            };
            let (cx, cy, r) = (mx + mark / 2.0, my + mark / 2.0, mark / 2.0);
            pg.polygon(&circle(cx, cy, r), Some([255, 255, 255]), Some((INK, thin)));
            if c.value == 1 {
                pg.polygon(&circle(cx, cy, r * 0.45), Some(INK), None);
            }
            caption(pg, mx + mark + 3.0 * b.k, mid);
        }
        ControlKind::Button => {
            pg.polygon(&rect(x, y, w, h), Some([0xE6, 0xE6, 0xE6]), Some((EDGE, thin)));
            let tw = gridcraft_pdf::text_width(font, size, c.text.lines().next().unwrap_or(""));
            caption(pg, x + (w - tw) / 2.0, mid);
        }
        ControlKind::Label => caption(pg, x + 2.0 * b.k, y + size),
        ControlKind::GroupBox => {
            pg.polygon(&rect(x, y + size / 2.0, w, (h - size / 2.0).max(0.0)), None, Some((EDGE, thin)));
            caption(pg, x + 6.0 * b.k, y + size * 0.85);
        }
        ControlKind::DropDown => {
            pg.polygon(&rect(x, y, w, h), Some([255, 255, 255]), Some((EDGE, thin)));
            let aw = h.min(w).max(0.0);
            let ax = x + w - aw;
            pg.polygon(&rect(ax, y, aw, h), Some([0xE6, 0xE6, 0xE6]), Some((EDGE, thin)));
            let (cx, cy, s) = (ax + aw / 2.0, y + h / 2.0, aw * 0.2);
            pg.polygon(&[[cx - s, cy - s / 2.0], [cx + s, cy - s / 2.0], [cx, cy + s / 2.0]], Some(INK), None);
        }
        ControlKind::ListBox | ControlKind::Slider | ControlKind::Spin => {
            pg.polygon(&rect(x, y, w, h), Some([255, 255, 255]), Some((EDGE, thin)));
        }
    }
}

fn draw_sparklines(pg: &mut Page, wb: &Workbook, si: usize, sh: &Sheet, b: &Block, block: RangeRef) {
    let values_of = |sp: &gridcraft_model::Sparkline| -> Vec<Option<f64>> {
        match gridcraft_calc::evaluate(wb, si, sp.cell, &sp.source) {
            Value::Array(a) => a.iter().take(10_000).map(Value::as_f64).collect(),
            v => vec![v.as_f64()],
        }
    };
    if !sh.sparklines.iter().any(|sp| block.contains(sp.cell)) {
        return;
    }
    let groups = gridcraft_chart::SparklineGroupLimits::new(&sh.sparklines, values_of);
    for sp in &sh.sparklines {
        if !block.contains(sp.cell) {
            continue;
        }
        let vals = values_of(sp);
        let style = gridcraft_chart::sparkline_style(sp, &wb.theme, [0x15, 0x60, 0x82], &vals, &groups);
        let (x, y, w, h) = b.rect(RangeRef::cell(sp.cell));
        let (wpx, hpx) = (w / b.k, h / b.k);
        let prims = gridcraft_chart::render_sparkline_styled(sp.kind, &vals, &style, wpx, hpx);
        draw_prims(pg, &prims, Xf { ox: x, oy: y, k: b.k });
    }
}

fn draw_shape(pg: &mut Page, wb: &Workbook, shp: &gridcraft_model::Shape, b: &Block, (x, y, w, h): (f32, f32, f32, f32)) {
    // The shape's own fill and outline, else what its theme style gives them.
    let fill = rgb_of(shp.drawn_fill(), wb);
    // `a:ln w` is in EMU (12700 per point), scaled like every other printed length; without it
    // the outline keeps the default hairline.
    let width = match shp.drawn_line_width_emu() {
        Some(emu) => (emu.max(0) as f32 / 12700.0 * b.scale).clamp(0.0, 1600.0),
        None => (0.75 * b.k).max(0.25),
    };
    let line = rgb_of(shp.drawn_line(), wb).filter(|_| !shp.format.no_line && !shp.style_hides_line()).map(|c| (c, width));
    let dash = outline_dash(shp, width);
    let dash = dash.as_slice();
    // The outer shadow first, under the shape: its outline moved, filled with the shadow colour
    // (a shape without a fill strokes it instead, as does a line).
    if let Some(((dx, dy), c)) = shape_shadow(wb, shp, b.scale) {
        let line_kind = matches!(shp.kind, ShapeKind::Line | ShapeKind::Arrow);
        let (sf, sl) = match (fill, line) {
            (Some(_), _) if !line_kind => (Some(c), None),
            (_, Some((_, lw))) => (None, Some((c, lw))),
            _ => (None, None),
        };
        if sf.is_some() || sl.is_some() {
            let sdash = if sf.is_some() { &[][..] } else { dash };
            draw_geometry(pg, shp, b, (x + dx, y + dy, w, h), sf, sl, sdash);
        }
    }
    draw_geometry(pg, shp, b, (x, y, w, h), fill, line, dash);
    if shp.text.is_empty() {
        return;
    }
    draw_shape_text(pg, wb, shp, b, (x, y, w, h));
}

/// Draws a shape's outline at `(x, y, w, h)`, filled with `fill` and stroked with `line`.
fn draw_geometry(
    pg: &mut Page,
    shp: &gridcraft_model::Shape,
    b: &Block,
    (x, y, w, h): (f32, f32, f32, f32),
    fill: Option<[u8; 3]>,
    line: Option<([u8; 3], f32)>,
    dash: &[f32],
) {
    match shp.kind {
        ShapeKind::Ellipse => {
            let pts: Vec<[f32; 2]> = (0..72)
                .map(|i| {
                    let a = i as f32 / 72.0 * std::f32::consts::TAU;
                    [x + w / 2.0 + w / 2.0 * a.cos(), y + h / 2.0 + h / 2.0 * a.sin()]
                })
                .collect();
            outline(pg, &placed(shp, (x, y, w, h), pts), fill, line, dash);
        }
        ShapeKind::Triangle => outline(pg, &placed(shp, (x, y, w, h), vec![[x + w / 2.0, y], [x + w, y + h], [x, y + h]]), fill, line, dash),
        ShapeKind::Line | ShapeKind::Arrow => {
            let c = line.map(|l| l.0).or(fill).unwrap_or([0, 0, 0]);
            let lw = (1.0 * b.k).max(0.3);
            let d = outline_dash(shp, lw);
            // The flips pick the diagonal and its direction; the rotation then turns it.
            let ((sx, sy), (ex, ey)) = shp.line_endpoints(w, h);
            let (cx, cy) = (f64::from(x) + f64::from(w) / 2.0, f64::from(y) + f64::from(h) / 2.0);
            let turn = |px: f32, py: f32| {
                let (tx, ty) = shp.rotate_point(cx, cy, (f64::from(x + px), f64::from(y + py)));
                (tx as f32, ty as f32)
            };
            let ((sx, sy), (ex, ey)) = (turn(sx, sy), turn(ex, ey));
            pg.stroke_line_dashed(gridcraft_pdf::Point::new(sx, sy), gridcraft_pdf::Point::new(ex, ey), lw, c, &d);
            if shp.kind == ShapeKind::Arrow {
                let ang = (ey - sy).atan2(ex - sx);
                let al = 8.0 * b.k;
                let p = |da: f32| [ex - al * (ang + da).cos(), ey - al * (ang + da).sin()];
                pg.polygon(&[[ex, ey], p(0.4), p(-0.4)], Some(c), None);
            }
        }
        ShapeKind::RoundedRectangle => outline(pg, &placed(shp, (x, y, w, h), rounded_rect_points((x, y, w, h))), fill, line, dash),
        ShapeKind::Freeform if draw_freeform(pg, shp, b, (x, y, w, h), fill, line, dash) => {}
        _ if draw_preset(pg, shp, b, (x, y, w, h), fill, line, dash) => {}
        _ => {
            outline(pg, &placed(shp, (x, y, w, h), vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]), fill, line, dash);
        }
    }
}

/// Prints a shape's text in its box.
fn draw_shape_text(pg: &mut Page, wb: &Workbook, shp: &gridcraft_model::Shape, b: &Block, (x, y, w, h): (f32, f32, f32, f32)) {
    use gridcraft_model::{ShapeTextAlign, ShapeTextAnchor};
    let fm = &shp.format;
    let text_box = shp.kind == ShapeKind::TextBox;
    let size = fm.font_size.unwrap_or(11.0).clamp(1.0, 4000.0) * b.scale;
    let font = Font::styled(Font::family_for(fm.font.as_deref().unwrap_or("")), fm.bold, fm.italic);
    let color = shp.drawn_text_color().and_then(|c| rgb_of(c, wb)).unwrap_or([0, 0, 0]);
    let align = fm.align.unwrap_or(if text_box { ShapeTextAlign::Left } else { ShapeTextAlign::Center });
    let anchor = fm.anchor.unwrap_or(if text_box { ShapeTextAnchor::Top } else { ShapeTextAnchor::Middle });
    // `a:bodyPr` insets in EMU (§21.1.2.1.1 defaults: 91440 left and right, 45720 top and
    // bottom), in points at the page's scale.
    let inset = |v: Option<i32>, default: i32| (v.unwrap_or(default) as f32 / 12700.0 * b.scale).clamp(-1.0e4, 1.0e4);
    let (il, it, ir, ib) = (inset(fm.inset_left, 91440), inset(fm.inset_top, 45720), inset(fm.inset_right, 91440), inset(fm.inset_bottom, 45720));
    let lines = wrap_lines(&shp.text, font, size, (w - il - ir).max(size));
    let block_h = lines.len().min(500) as f32 * size * 1.2;
    let mut yy = match anchor {
        ShapeTextAnchor::Top => y + it,
        ShapeTextAnchor::Middle => y + it + (h - it - ib - block_h) / 2.0,
        ShapeTextAnchor::Bottom => y + h - ib - block_h,
    } + size * 0.8;
    for l in lines.iter().take(500) {
        let lw = gridcraft_pdf::text_width(font, size, l);
        let lx = match align {
            ShapeTextAlign::Left => x + il,
            ShapeTextAlign::Center => x + il + (w - il - ir - lw) / 2.0,
            ShapeTextAlign::Right => x + w - ir - lw,
        };
        // WordArt's drop shadow: the line again, offset, in the shadow colour (under the text).
        if let Some(sc) = fm.text_shadow.and_then(|c| rgb_of(c, wb)) {
            let off = (size * 0.06).max(0.5);
            pg.text(lx + off, yy + off, size, font, sc, l);
        }
        // WordArt's text outline, as on screen: the line in the outline colour, offset in eight
        // directions by half the outline width, under the filled text.
        if let Some((c, ow)) = fm.text_outline
            && let Some(oc) = rgb_of(c, wb)
        {
            let d = (ow * b.scale / 2.0).clamp(0.25, 20.0);
            for (dx, dy) in [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)] {
                pg.text(lx + dx * d, yy + dy * d, size, font, oc, l);
            }
        }
        pg.text(lx, yy, size, font, color, l);
        yy += size * 1.2;
    }
}

/// An outline's points as drawn: mirrored by the shape's flips, then turned by its rotation, both
/// about the centre of the box `(x, y, w, h)` (ECMA-376 Part 1 §20.1.7.6).
fn placed(shp: &gridcraft_model::Shape, (x, y, w, h): (f32, f32, f32, f32), mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    if !shp.flip_h && !shp.flip_v && shp.rotation.is_none() {
        return pts;
    }
    let (cx, cy) = (f64::from(x) + f64::from(w) / 2.0, f64::from(y) + f64::from(h) / 2.0);
    for p in &mut pts {
        let (px, py) = shp.transform_point(cx, cy, (f64::from(p[0]), f64::from(p[1])));
        *p = [px as f32, py as f32];
    }
    pts
}

/// The PDF dash array of a shape's outline stroked `width` wide: its preset dash
/// (`a:prstDash`, ECMA-376 Part 1 §20.1.10.48) in units of the width; empty for a solid outline.
pub(super) fn outline_dash(shp: &gridcraft_model::Shape, width: f32) -> Vec<f32> {
    let width = if width.is_finite() { width.max(0.0) } else { 0.0 };
    shp.format.line_dash.map(|d| d.pattern().iter().map(|v| v * width).collect()).unwrap_or_default()
}

/// A closed polygon, filled with `fill` and outlined with `line`, dashed by `dash` (a PDF dash
/// array; empty for a solid outline).
fn outline(pg: &mut Page, pts: &[[f32; 2]], fill: Option<[u8; 3]>, line: Option<([u8; 3], f32)>, dash: &[f32]) {
    match line {
        Some((c, lw)) if !dash.is_empty() => {
            pg.polygon(pts, fill, None);
            let mut closed = pts.to_vec();
            if let Some(&first) = pts.first() {
                closed.push(first);
            }
            pg.polyline(&closed, lw, c, dash);
        }
        _ => pg.polygon(pts, fill, line),
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
    dash: &[f32],
) -> bool {
    let Some(geom) = shp.geometry.as_ref() else { return false };
    draw_paths(pg, shp, &geom.paths, b, rect, fill, line, dash)
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
    dash: &[f32],
) -> bool {
    let Some(geom) = shp.geometry.as_ref() else { return false };
    let res = |v: f32| (v.abs() * 100.0).round().min(1.0e9) as i64;
    let Some(paths) = gridcraft_model::preset_paths(geom, res(rect.2), res(rect.3)) else { return false };
    draw_paths(pg, shp, &paths, b, rect, fill, line, dash)
}

/// Draws geometry paths flattened into polygons and polylines on the shape box; false when there
/// is nothing to draw.
#[allow(clippy::too_many_arguments)]
fn draw_paths(
    pg: &mut Page,
    shp: &gridcraft_model::Shape,
    paths: &[gridcraft_model::GeometryPath],
    b: &Block,
    rect: (f32, f32, f32, f32),
    fill: Option<[u8; 3]>,
    line: Option<([u8; 3], f32)>,
    dash: &[f32],
) -> bool {
    let mut budget = MAX_OUTLINE_POINTS;
    let mut drew = false;
    for path in paths {
        let line = line.filter(|_| path.stroke);
        let subs: Vec<Subpath> = flatten_path(path, rect, b.k, &mut budget)
            .into_iter()
            .map(|sub| Subpath { points: placed(shp, rect, sub.points), closed: sub.closed })
            .collect();
        // Several closed subpaths of one filled path are filled together, even-odd, so one inside
        // another is a hole (donut, frame). A single subpath prints as before.
        if path.fill && subs.iter().filter(|s| s.closed && s.points.len() >= 2).count() >= 2 {
            let mut rings = Vec::new();
            for sub in subs.into_iter().filter(|s| s.points.len() >= 2) {
                drew = true;
                if sub.closed {
                    rings.push(sub.points);
                } else if let Some((c, lw)) = line {
                    pg.polyline(&sub.points, lw, c, dash);
                }
            }
            match line {
                Some((c, lw)) if !dash.is_empty() => {
                    pg.polygons_even_odd(&rings, fill.map(|c| path.shade.apply(c)), None);
                    for ring in &rings {
                        outline(pg, ring, None, Some((c, lw)), dash);
                    }
                }
                _ => pg.polygons_even_odd(&rings, fill.map(|c| path.shade.apply(c)), line),
            }
            continue;
        }
        for sub in subs {
            if sub.points.len() < 2 {
                continue;
            }
            drew = true;
            if path.fill {
                outline(pg, &sub.points, fill.map(|c| path.shade.apply(c)), line, dash);
            } else if let Some((c, lw)) = line {
                if sub.closed {
                    outline(pg, &sub.points, None, Some((c, lw)), dash);
                } else {
                    pg.polyline(&sub.points, lw, c, dash);
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
                // Visual angles turned into the ellipse's parameter; the sweep is at most one turn.
                let (wr, hr) = (w_r as f64, h_r as f64);
                let (st, sw) = gridcraft_model::arc_parameters(w_r, h_r, st_ang, sw_ang);
                let (cx, cy) = (pos.0 - wr * st.cos(), pos.1 - hr * st.sin());
                // One segment per 15° of sweep, at least 4.
                let segs = (sw.to_degrees().abs() / 15.0).ceil().clamp(4.0, 1440.0) as u32;
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

/// A shape's outer shadow for print: its offset in points times `scale` (x to the right, y down the
/// page) and its colour blended over white (PDF output has no transparency); `None` when the shape
/// has no outer shadow to draw.
pub(super) fn shape_shadow(wb: &gridcraft_model::Workbook, shp: &gridcraft_model::Shape, scale: f32) -> Option<((f32, f32), [u8; 3])> {
    let Some(gridcraft_model::ShapeEffects::OuterShadow(sh)) = shp.format.effects.as_ref() else { return None };
    let (dx, dy) = sh.offset_pt()?;
    let (rgb, alpha) = sh.rgba(&wb.theme)?;
    let scale = if scale.is_finite() { scale } else { 1.0 };
    let over_white = |c: u8| (255.0 - alpha * (255.0 - f32::from(c))).round().clamp(0.0, 255.0) as u8;
    Some(((dx * scale, dy * scale), rgb.map(over_white)))
}
