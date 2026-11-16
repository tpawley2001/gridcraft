//! EMF ([MS-EMF]) pictures for print: the records Excel's printouts need, turned into vector
//! paths and bitmaps in picture-relative coordinates.
//!
//! The interpreter follows the playback model of [MS-EMF] §3.1: a device context holds the
//! selected pen and brush, the polygon fill mode, the current position and the world and page
//! transforms; drawing records map their logical coordinates through the world transform
//! (§2.3.12), then the window/viewport mapping (§2.3.11), into device pixels, which are finally
//! expressed relative to the header's picture frame (§2.2.9).
//!
//! Covered: pens, brushes and stock objects, rectangles, ellipses, polygons, polylines and
//! Béziers (32- and 16-bit point forms), path brackets with fill/stroke, `SAVEDC`/`RESTOREDC`,
//! and device-independent bitmaps (1/4/8/16/24/32 bpp, uncompressed). Text, clipping, EMF+
//! comments and raster operations other than plain copies are not drawn. Every other record is
//! skipped by its size; a malformed record is skipped, and a malformed record size ends the
//! picture (what was drawn so far is kept).

use std::collections::HashMap;

/// A parsed metafile: what to draw, in order.
#[derive(Debug, Clone, Default)]
pub struct Drawing {
    pub items: Vec<Item>,
}

/// One drawing item. Coordinates are relative to the picture frame: (0, 0) is its top-left
/// corner, (1, 1) its bottom-right.
#[derive(Debug, Clone)]
pub enum Item {
    /// Subpaths (curves already flattened). A stroke width is a fraction of the frame width.
    Path { subpaths: Vec<Vec<[f32; 2]>>, closed: bool, fill: Option<[u8; 3]>, stroke: Option<([u8; 3], f32)>, even_odd: bool },
    /// A bitmap stretched into `rect` = [x, y, w, h]; RGBA8, rows top-down.
    Image { rect: [f32; 4], rgba: Vec<u8>, size: [u32; 2] },
}

/// Most records interpreted; the rest of a longer picture is not drawn.
const MAX_RECORDS: usize = 200_000;
/// Most path points emitted; drawing stops at this many.
const MAX_POINTS: usize = 2_000_000;
/// Largest single bitmap decoded, and most bitmap pixels decoded for one picture.
const MAX_BITMAP_PIXELS: u64 = 40_000_000;
/// Deepest `SAVEDC` nesting kept.
const MAX_SAVED: usize = 1024;
/// Points per flattened ellipse and per flattened cubic Bézier segment.
const ELLIPSE_POINTS: usize = 64;
const BEZIER_STEPS: usize = 16;

const SIGNATURE: u32 = 0x464D_4520;

/// Parses an EMF byte stream; `None` when `data` is not an EMF or its header is unusable.
pub fn parse_emf(data: &[u8]) -> Option<Drawing> {
    if u32_at(data, 0)? != 1 || u32_at(data, 40)? != SIGNATURE {
        return None;
    }
    let header_size = u32_at(data, 4)? as usize;
    if header_size < 88 || header_size > data.len() {
        return None;
    }
    let frame: Vec<f64> = (0..4).map(|i| i32_at(data, 24 + 4 * i).map(f64::from)).collect::<Option<_>>()?;
    let dev = [f64::from(i32_at(data, 72)?), f64::from(i32_at(data, 76)?)];
    let mm = [f64::from(i32_at(data, 80)?), f64::from(i32_at(data, 84)?)];
    if dev[0] <= 0.0 || dev[1] <= 0.0 || mm[0] <= 0.0 || mm[1] <= 0.0 {
        return None;
    }
    // 0.01 mm -> device pixels.
    let (kx, ky) = (dev[0] / (mm[0] * 100.0), dev[1] / (mm[1] * 100.0));
    let (left, top, right, bottom) = (frame.first()? * kx, frame.get(1)? * ky, frame.get(2)? * kx, frame.get(3)? * ky);
    let (fw, fh) = (right - left, bottom - top);
    if !(fw.is_finite() && fh.is_finite()) || fw == 0.0 || fh == 0.0 {
        return None;
    }
    let mut it = Interp::new([left, top, fw, fh]);
    let mut off = header_size;
    let mut count = 0usize;
    while count < MAX_RECORDS && it.points < MAX_POINTS {
        let (Some(ty), Some(size)) = (u32_at(data, off), u32_at(data, off.saturating_add(4))) else { break };
        let size = size as usize;
        let Some(rec) = off.checked_add(size).and_then(|end| data.get(off..end)) else { break };
        if size < 8 {
            break;
        }
        if ty == 14 {
            break; // EMR_EOF
        }
        it.record(ty, rec);
        off += size;
        count += 1;
    }
    Some(Drawing { items: it.items })
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    let b = d.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([*b.first()?, *b.get(1)?, *b.get(2)?, *b.get(3)?]))
}
fn i32_at(d: &[u8], at: usize) -> Option<i32> {
    u32_at(d, at).map(|v| v as i32)
}
fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    let b = d.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*b.first()?, *b.get(1)?]))
}
fn i16_at(d: &[u8], at: usize) -> Option<i16> {
    u16_at(d, at).map(|v| v as i16)
}
fn f32_at(d: &[u8], at: usize) -> Option<f64> {
    u32_at(d, at).map(|v| f64::from(f32::from_bits(v))).filter(|v| v.is_finite())
}
fn point_at(d: &[u8], at: usize) -> Option<[f64; 2]> {
    Some([f64::from(i32_at(d, at)?), f64::from(i32_at(d, at.checked_add(4)?)?)])
}
/// `n` points from `at`: 32-bit (`wide`) or 16-bit coordinate pairs, all inside `d`.
fn points_at(d: &[u8], at: usize, n: usize, wide: bool) -> Option<Vec<[f64; 2]>> {
    let step = if wide { 8 } else { 4 };
    let end = at.checked_add(n.checked_mul(step)?)?;
    if end > d.len() {
        return None;
    }
    (0..n)
        .map(|i| {
            let p = at + i * step;
            if wide { point_at(d, p) } else { Some([f64::from(i16_at(d, p)?), f64::from(i16_at(d, p + 2)?)]) }
        })
        .collect()
}
/// A COLORREF (0x00BBGGRR) as RGB.
fn colorref(v: u32) -> [u8; 3] {
    [(v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8]
}

/// An affine transform `[m11, m12, m21, m22, dx, dy]`: x' = x·m11 + y·m21 + dx, y' = x·m12 + y·m22 + dy.
type Xform = [f64; 6];
const IDENTITY: Xform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` then `b`.
fn compose(a: Xform, b: Xform) -> Xform {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// A selected pen: colour, width in logical units, and whether it is cosmetic (one device pixel).
#[derive(Debug, Clone, Copy)]
struct Pen {
    color: [u8; 3],
    width: f64,
    cosmetic: bool,
}

#[derive(Debug, Clone, Copy)]
enum Obj {
    Pen(Option<Pen>),
    Brush(Option<[u8; 3]>),
}

/// The device context state that `SAVEDC` saves.
#[derive(Debug, Clone)]
struct Dc {
    map_mode: u32,
    win_org: [f64; 2],
    win_ext: [f64; 2],
    vp_org: [f64; 2],
    vp_ext: [f64; 2],
    world: Xform,
    pen: Option<Pen>,
    brush: Option<[u8; 3]>,
    even_odd: bool,
    pos: [f64; 2],
}

impl Default for Dc {
    fn default() -> Self {
        Dc {
            map_mode: 1,
            win_org: [0.0; 2],
            win_ext: [1.0; 2],
            vp_org: [0.0; 2],
            vp_ext: [1.0; 2],
            world: IDENTITY,
            pen: Some(Pen { color: [0, 0, 0], width: 0.0, cosmetic: true }),
            brush: Some([255, 255, 255]),
            even_odd: true,
            pos: [0.0; 2],
        }
    }
}

impl Dc {
    /// The page (window -> viewport) scale per axis.
    fn page_scale(&self) -> [f64; 2] {
        if self.map_mode != 7 && self.map_mode != 8 {
            return [1.0, 1.0];
        }
        let s = |i: usize| {
            let (w, v) = (self.win_ext.get(i).copied().unwrap_or(1.0), self.vp_ext.get(i).copied().unwrap_or(1.0));
            if w == 0.0 { 1.0 } else { v / w }
        };
        let (sx, sy) = (s(0), s(1));
        if self.map_mode == 7 {
            let m = sx.abs().min(sy.abs());
            [m.copysign(sx), m.copysign(sy)]
        } else {
            [sx, sy]
        }
    }
    /// The whole logical -> device transform.
    fn transform(&self) -> Xform {
        let [sx, sy] = self.page_scale();
        let page = [sx, 0.0, 0.0, sy, self.vp_org[0] - self.win_org[0] * sx, self.vp_org[1] - self.win_org[1] * sy];
        compose(self.world, page)
    }
}

/// Figures of the path under construction, in picture-relative coordinates.
#[derive(Default)]
struct PathBuf {
    figures: Vec<(Vec<[f32; 2]>, bool)>,
    /// The last figure is open and LINETO-style records continue it.
    open: bool,
}

struct Interp {
    /// Frame left, top, width, height in device pixels.
    frame: [f64; 4],
    dc: Dc,
    saved: Vec<Dc>,
    objects: HashMap<u32, Obj>,
    path: Option<PathBuf>,
    building: bool,
    items: Vec<Item>,
    points: usize,
    bitmap_pixels: u64,
}

impl Interp {
    fn new(frame: [f64; 4]) -> Self {
        Interp {
            frame,
            dc: Dc::default(),
            saved: Vec::new(),
            objects: HashMap::new(),
            path: None,
            building: false,
            items: Vec::new(),
            points: 0,
            bitmap_pixels: 0,
        }
    }

    fn rel(&self, t: &Xform, p: [f64; 2]) -> [f32; 2] {
        let x = p[0] * t[0] + p[1] * t[2] + t[4];
        let y = p[0] * t[1] + p[1] * t[3] + t[5];
        [((x - self.frame[0]) / self.frame[2]) as f32, ((y - self.frame[1]) / self.frame[3]) as f32]
    }
    fn map(&self, pts: &[[f64; 2]]) -> Vec<[f32; 2]> {
        let t = self.dc.transform();
        pts.iter().map(|p| self.rel(&t, *p)).collect()
    }
    /// The selected pen as (colour, width as a fraction of the frame width).
    fn stroke(&self) -> Option<([u8; 3], f32)> {
        let pen = self.dc.pen?;
        let device = if pen.cosmetic || pen.width <= 0.0 {
            1.0
        } else {
            let t = self.dc.transform();
            pen.width * (t[0] * t[3] - t[1] * t[2]).abs().sqrt()
        };
        let w = (device / self.frame[2].abs()) as f32;
        Some((pen.color, if w.is_finite() { w } else { 0.0 }))
    }

    fn emit(&mut self, subpaths: Vec<Vec<[f32; 2]>>, closed: bool, fill: Option<[u8; 3]>, stroke: Option<([u8; 3], f32)>) {
        let subpaths: Vec<Vec<[f32; 2]>> = subpaths.into_iter().filter(|s| s.len() >= 2).collect();
        if subpaths.is_empty() || (fill.is_none() && stroke.is_none()) || self.points >= MAX_POINTS {
            return;
        }
        self.points = self.points.saturating_add(subpaths.iter().map(Vec::len).sum());
        self.items.push(Item::Path { subpaths, closed, fill, stroke, even_odd: self.dc.even_odd });
    }

    /// A closed shape: a figure of the path under construction, or filled and outlined now.
    fn shape(&mut self, figures: Vec<Vec<[f64; 2]>>) {
        let mapped: Vec<Vec<[f32; 2]>> = figures.iter().map(|f| self.map(f)).collect();
        if self.building {
            self.add_figures(mapped, true);
        } else {
            let (fill, stroke) = (self.dc.brush, self.stroke());
            self.emit(mapped, true, fill, stroke);
        }
    }
    /// Open lines: a figure of the path under construction, or stroked now.
    fn lines(&mut self, figures: Vec<Vec<[f64; 2]>>) {
        let mapped: Vec<Vec<[f32; 2]>> = figures.iter().map(|f| self.map(f)).collect();
        if self.building {
            self.add_figures(mapped, false);
        } else {
            let stroke = self.stroke();
            self.emit(mapped, false, None, stroke);
        }
    }
    fn add_figures(&mut self, figures: Vec<Vec<[f32; 2]>>, closed: bool) {
        let Some(path) = self.path.as_mut() else { return };
        for f in figures {
            self.points = self.points.saturating_add(f.len());
            path.figures.push((f, closed));
        }
        path.open = false;
    }
    /// Lines from the current position through `pts` (LINETO, POLYLINETO, POLYBEZIERTO).
    fn line_to(&mut self, pts: Vec<[f64; 2]>) {
        let Some(&last) = pts.last() else { return };
        let start = self.dc.pos;
        self.dc.pos = last;
        if self.building {
            let mapped = self.map(&pts);
            let first = self.map(&[start]);
            self.points = self.points.saturating_add(mapped.len());
            let Some(path) = self.path.as_mut() else { return };
            match path.figures.last_mut() {
                Some((fig, false)) if path.open => fig.extend(mapped),
                _ => path.figures.push(([first, mapped].concat(), false)),
            }
            path.open = true;
        } else {
            let mut all = Vec::with_capacity(pts.len() + 1);
            all.push(start);
            all.extend(pts);
            self.lines(vec![all]);
        }
    }
    fn move_to(&mut self, p: [f64; 2]) {
        self.dc.pos = p;
        if let (true, Some(path)) = (self.building, self.path.as_mut()) {
            path.open = false;
        }
    }

    fn take_path(&mut self) -> Option<PathBuf> {
        self.building = false;
        self.path.take()
    }

    fn record(&mut self, ty: u32, r: &[u8]) {
        let _ = self.try_record(ty, r);
    }

    fn try_record(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        match ty {
            2..=8 | 85..=91 => self.poly_record(ty, r),
            37..=40 | 93..=95 => self.object_record(ty, r),
            59..=68 => self.path_record(ty),
            76 | 77 | 80 | 81 => self.bitmap_record(ty, r),
            _ => self.state_record(ty, r),
        }
    }

    /// Point-list records (polygons, polylines, Béziers).
    fn poly_record(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        match ty {
            // EMR_POLYBEZIER, POLYGON, POLYLINE, POLYBEZIERTO, POLYLINETO and their 16-bit forms.
            2..=6 | 85..=89 => {
                let wide = ty <= 6;
                let n = u32_at(r, 24)? as usize;
                let pts = points_at(r, 28, n, wide)?;
                match if wide { ty } else { ty - 83 } {
                    2 => self.lines(vec![bezier(None, &pts)]),
                    3 => self.shape(vec![pts]),
                    4 => self.lines(vec![pts]),
                    5 => {
                        let start = self.dc.pos;
                        let flat = bezier(Some(start), &pts);
                        self.line_to(flat.get(1..).map(<[_]>::to_vec).unwrap_or_default());
                    }
                    _ => self.line_to(pts),
                }
            }
            // EMR_POLYPOLYLINE, POLYPOLYGON and their 16-bit forms.
            7 | 8 | 90 | 91 => {
                let wide = ty <= 8;
                let polys = u32_at(r, 24)? as usize;
                let total = u32_at(r, 28)? as usize;
                let counts_end = 32usize.checked_add(polys.checked_mul(4)?)?;
                if counts_end > r.len() {
                    return None;
                }
                let pts = points_at(r, counts_end, total, wide)?;
                let mut figures = Vec::new();
                let mut at = 0usize;
                for i in 0..polys {
                    let c = u32_at(r, 32 + 4 * i)? as usize;
                    let end = at.checked_add(c)?;
                    figures.push(pts.get(at..end)?.to_vec());
                    at = end;
                }
                if ty == 8 || ty == 91 { self.shape(figures) } else { self.lines(figures) }
            }
            _ => {}
        }
        Some(())
    }

    /// Device-context state, simple shapes and line drawing.
    fn state_record(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        match ty {
            9 => self.dc.win_ext = point_at(r, 8)?,
            10 => self.dc.win_org = point_at(r, 8)?,
            11 => self.dc.vp_ext = point_at(r, 8)?,
            12 => self.dc.vp_org = point_at(r, 8)?,
            17 => self.dc.map_mode = u32_at(r, 8)?,
            19 => self.dc.even_odd = u32_at(r, 8)? != 2,
            27 => self.move_to(point_at(r, 8)?),
            33 => {
                if self.saved.len() < MAX_SAVED {
                    self.saved.push(self.dc.clone());
                }
            }
            34 => {
                let rel = i64::from(i32_at(r, 8)?);
                let len = self.saved.len() as i64;
                let idx = if rel < 0 { len + rel } else { rel - 1 };
                if idx >= 0 && idx < len {
                    let idx = idx as usize;
                    self.dc = self.saved.get(idx)?.clone();
                    self.saved.truncate(idx);
                }
            }
            35 => self.dc.world = xform_at(r, 8)?,
            36 => {
                let x = xform_at(r, 8)?;
                self.dc.world = match u32_at(r, 32)? {
                    1 => IDENTITY,
                    2 => compose(x, self.dc.world),
                    3 => compose(self.dc.world, x),
                    4 => x,
                    _ => return None,
                };
            }
            42 => {
                let (a, b) = (point_at(r, 8)?, point_at(r, 16)?);
                self.shape(vec![ellipse(a, b)]);
            }
            43 | 44 => {
                let (a, b) = (point_at(r, 8)?, point_at(r, 16)?);
                self.shape(vec![vec![a, [b[0], a[1]], b, [a[0], b[1]]]]);
            }
            54 => self.line_to(vec![point_at(r, 8)?]),
            _ => {}
        }
        Some(())
    }

    /// Pens and brushes: creation, selection, deletion.
    fn object_record(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        match ty {
            37 => self.select(u32_at(r, 8)?),
            38 => {
                // EMR_CREATEPEN: LogPen (style, width as a point, COLORREF).
                let style = u32_at(r, 12)?;
                let width = f64::from(i32_at(r, 16)?).abs();
                let color = colorref(u32_at(r, 24)?);
                let pen = (style & 0xF != 5).then_some(Pen { color, width, cosmetic: width == 0.0 });
                self.objects.insert(u32_at(r, 8)?, Obj::Pen(pen));
            }
            39 => {
                // EMR_CREATEBRUSHINDIRECT: LogBrush32 (style, COLORREF, hatch).
                let style = u32_at(r, 12)?;
                let color = colorref(u32_at(r, 16)?);
                let brush = match style {
                    1 => None,
                    0 | 2 => Some(color),
                    // Pattern brushes are not drawn as patterns: a mid grey stands in.
                    _ => Some([128, 128, 128]),
                };
                self.objects.insert(u32_at(r, 8)?, Obj::Brush(brush));
            }
            40 => {
                self.objects.remove(&u32_at(r, 8)?);
            }
            95 => {
                // EMR_EXTCREATEPEN: LogPenEx (style, width, brush style, COLORREF) after the DIB fields.
                let style = u32_at(r, 28)?;
                let width = f64::from(u32_at(r, 32)?);
                let brush_style = u32_at(r, 36)?;
                let color = colorref(u32_at(r, 40)?);
                let geometric = style & 0x000F_0000 == 0x0001_0000;
                let pen = (style & 0xF != 5 && brush_style != 1).then_some(Pen { color, width, cosmetic: !geometric || width == 0.0 });
                self.objects.insert(u32_at(r, 8)?, Obj::Pen(pen));
            }
            // EMR_CREATEMONOBRUSH, EMR_CREATEDIBPATTERNBRUSHPT: a pattern, drawn as mid grey.
            93 | 94 => {
                self.objects.insert(u32_at(r, 8)?, Obj::Brush(Some([128, 128, 128])));
            }
            _ => {}
        }
        Some(())
    }

    /// Path brackets and the records that fill or stroke them.
    fn path_record(&mut self, ty: u32) -> Option<()> {
        match ty {
            59 => {
                self.path = Some(PathBuf::default());
                self.building = true;
            }
            60 => self.building = false,
            61 => {
                if let (true, Some(path)) = (self.building, self.path.as_mut()) {
                    if let (true, Some(last)) = (path.open, path.figures.last_mut()) {
                        last.1 = true;
                    }
                    path.open = false;
                }
            }
            62..=64 => {
                let path = self.take_path()?;
                let (fill, stroke) = (self.dc.brush.filter(|_| ty != 64), self.stroke().filter(|_| ty != 62));
                if fill.is_some() {
                    let all: Vec<Vec<[f32; 2]>> = path.figures.iter().map(|f| f.0.clone()).collect();
                    self.emit(all, true, fill, stroke.filter(|_| path.figures.iter().all(|f| f.1)));
                    if path.figures.iter().all(|f| f.1) {
                        return Some(());
                    }
                }
                // Outlines: closed and open figures separately (an open figure in a filled
                // path is outlined unclosed).
                let (closed, open): (Vec<_>, Vec<_>) = path.figures.into_iter().partition(|f| f.1);
                self.emit(closed.into_iter().map(|f| f.0).collect(), true, None, stroke);
                self.emit(open.into_iter().map(|f| f.0).collect(), false, None, stroke);
            }
            // EMR_ABORTPATH, EMR_SELECTCLIPPATH: the path is discarded.
            67 | 68 => {
                self.take_path();
            }
            _ => {}
        }
        Some(())
    }

    /// Bitmap records.
    fn bitmap_record(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        match ty {
            76 | 77 => self.blt(ty, r)?,
            80 => {
                // EMR_SETDIBITSTODEVICE
                let dest = point_at(r, 24)?;
                let src = [i32_at(r, 32)?, i32_at(r, 36)?, i32_at(r, 40)?, i32_at(r, 44)?];
                let dib =
                    Dib { off_bmi: u32_at(r, 48)?, cb_bmi: u32_at(r, 52)?, off_bits: u32_at(r, 56)?, cb_bits: u32_at(r, 60)?, usage: u32_at(r, 64)? };
                let size = [f64::from(src[2]), f64::from(src[3])];
                self.bitmap(r, &dib, src, dest, size)?;
            }
            81 => {
                // EMR_STRETCHDIBITS
                if !copies_source(u32_at(r, 68)?) {
                    return None;
                }
                let dest = point_at(r, 24)?;
                let src = [i32_at(r, 32)?, i32_at(r, 36)?, i32_at(r, 40)?, i32_at(r, 44)?];
                let dib =
                    Dib { off_bmi: u32_at(r, 48)?, cb_bmi: u32_at(r, 52)?, off_bits: u32_at(r, 56)?, cb_bits: u32_at(r, 60)?, usage: u32_at(r, 64)? };
                let size = point_at(r, 72)?;
                self.bitmap(r, &dib, src, dest, size)?;
            }
            _ => {}
        }
        Some(())
    }
    fn select(&mut self, ih: u32) {
        if ih & 0x8000_0000 != 0 {
            match ih & 0x7FFF_FFFF {
                0 => self.dc.brush = Some([255, 255, 255]),
                1 => self.dc.brush = Some([0xC0, 0xC0, 0xC0]),
                2 => self.dc.brush = Some([0x80, 0x80, 0x80]),
                3 => self.dc.brush = Some([0x40, 0x40, 0x40]),
                4 => self.dc.brush = Some([0, 0, 0]),
                5 => self.dc.brush = None,
                6 => self.dc.pen = Some(Pen { color: [255, 255, 255], width: 0.0, cosmetic: true }),
                7 => self.dc.pen = Some(Pen { color: [0, 0, 0], width: 0.0, cosmetic: true }),
                8 => self.dc.pen = None,
                _ => {}
            }
            return;
        }
        match self.objects.get(&ih) {
            Some(Obj::Pen(p)) => self.dc.pen = *p,
            Some(Obj::Brush(b)) => self.dc.brush = *b,
            None => {}
        }
    }

    /// EMR_BITBLT / EMR_STRETCHBLT: a source bitmap, or (without one) a pattern fill.
    fn blt(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        let dest = point_at(r, 24)?;
        let size = point_at(r, 32)?;
        let rop = u32_at(r, 40)?;
        let dib = Dib { off_bmi: u32_at(r, 84)?, cb_bmi: u32_at(r, 88)?, off_bits: u32_at(r, 92)?, cb_bits: u32_at(r, 96)?, usage: u32_at(r, 80)? };
        if dib.cb_bmi == 0 {
            let fill = match rop {
                0x00F0_0021 => self.dc.brush,
                0x0000_0042 => Some([0, 0, 0]),
                0x00FF_0062 => Some([255, 255, 255]),
                _ => None,
            };
            let (a, b) = (dest, [dest[0] + size[0], dest[1] + size[1]]);
            let rect = self.map(&[a, [b[0], a[1]], b, [a[0], b[1]]]);
            if self.building {
                return Some(());
            }
            self.emit(vec![rect], true, fill, None);
            return Some(());
        }
        if !copies_source(rop) {
            return None;
        }
        let (sx, sy) = (i32_at(r, 44)?, i32_at(r, 48)?);
        let (cx, cy) = if ty == 77 { (i32_at(r, 100)?, i32_at(r, 104)?) } else { (size[0] as i32, size[1] as i32) };
        self.bitmap(r, &dib, [sx, sy, cx, cy], dest, size)
    }

    /// Decodes the record's DIB, crops it to `src` = [x, y, w, h] and places it at `dest`
    /// (logical), `size` logical units large.
    fn bitmap(&mut self, r: &[u8], dib: &Dib, src: [i32; 4], dest: [f64; 2], size: [f64; 2]) -> Option<()> {
        if self.building {
            return None;
        }
        let bmi = r.get(dib.off_bmi as usize..(dib.off_bmi as usize).checked_add(dib.cb_bmi as usize)?)?;
        let bits = r.get(dib.off_bits as usize..(dib.off_bits as usize).checked_add(dib.cb_bits as usize)?)?;
        if dib.usage != 0 {
            return None; // DIB_PAL_COLORS: indexes into a logical palette this interpreter does not keep.
        }
        let budget = MAX_BITMAP_PIXELS.saturating_sub(self.bitmap_pixels);
        let (rgba, w, h, bottom_up) = decode_dib(bmi, bits, budget)?;
        self.bitmap_pixels = self.bitmap_pixels.saturating_add(u64::from(w) * u64::from(h));
        let (rgba, cw, ch) = crop(rgba, w, h, bottom_up, src)?;
        let t = self.dc.transform();
        let p0 = self.rel(&t, dest);
        let p1 = self.rel(&t, [dest[0] + size[0], dest[1] + size[1]]);
        let (flip_x, flip_y) = (p1[0] < p0[0], p1[1] < p0[1]);
        let rgba = flip(rgba, cw, ch, flip_x, flip_y);
        let rect = [p0[0].min(p1[0]), p0[1].min(p1[1]), (p1[0] - p0[0]).abs(), (p1[1] - p0[1]).abs()];
        if !rect.iter().all(|v| v.is_finite()) {
            return None;
        }
        self.items.push(Item::Image { rect, rgba, size: [cw, ch] });
        Some(())
    }
}

struct Dib {
    off_bmi: u32,
    cb_bmi: u32,
    off_bits: u32,
    cb_bits: u32,
    usage: u32,
}

/// Raster operations drawn as a plain copy of the source; masks and inversions are not drawn.
fn copies_source(rop: u32) -> bool {
    !matches!(rop, 0x0088_00C6 | 0x00EE_0086 | 0x0066_0046 | 0x0044_0328 | 0x0055_0009 | 0x00AA_0029 | 0x0022_0326 | 0x00BB_0226)
}

fn xform_at(r: &[u8], at: usize) -> Option<Xform> {
    Some([f32_at(r, at)?, f32_at(r, at + 4)?, f32_at(r, at + 8)?, f32_at(r, at + 12)?, f32_at(r, at + 16)?, f32_at(r, at + 20)?])
}

/// The ellipse inscribed in the box with corners `a` and `b`, as a closed polygon.
fn ellipse(a: [f64; 2], b: [f64; 2]) -> Vec<[f64; 2]> {
    let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
    let (rx, ry) = ((b[0] - a[0]).abs() / 2.0, (b[1] - a[1]).abs() / 2.0);
    (0..ELLIPSE_POINTS)
        .map(|i| {
            let t = i as f64 * std::f64::consts::TAU / ELLIPSE_POINTS as f64;
            [cx + rx * t.cos(), cy + ry * t.sin()]
        })
        .collect()
}

/// Flattens cubic Bézier segments: `start` (or the first of `pts`) then control, control, end
/// triples. The result starts with the start point.
fn bezier(start: Option<[f64; 2]>, pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let (Some(mut p0), rest) = (match start {
        Some(s) => (Some(s), pts),
        None => (pts.first().copied(), pts.get(1..).unwrap_or_default()),
    }) else {
        return Vec::new();
    };
    let mut out = vec![p0];
    for seg in rest.as_chunks::<3>().0 {
        let [p1, p2, p3] = seg;
        for i in 1..=BEZIER_STEPS {
            let t = i as f64 / BEZIER_STEPS as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            out.push([a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]]);
        }
        p0 = *p3;
    }
    out
}

/// Decodes a BITMAPINFOHEADER (or BITMAPCOREHEADER) DIB to top-down RGBA. Returns the pixels,
/// width, height and whether the source rows were stored bottom-up. Compressed DIBs, and
/// DIBs over `budget` pixels, are not decoded.
fn decode_dib(bmi: &[u8], bits: &[u8], budget: u64) -> Option<(Vec<u8>, u32, u32, bool)> {
    let header = u32_at(bmi, 0)? as usize;
    let (width, height, bpp, compression, used, entry) = if header == 12 {
        (i32::from(u16_at(bmi, 4)?), i32::from(i16_at(bmi, 6)?), u16_at(bmi, 10)?, 0, 0, 3usize)
    } else if header >= 40 {
        (i32_at(bmi, 4)?, i32_at(bmi, 8)?, u16_at(bmi, 14)?, u32_at(bmi, 16)?, u32_at(bmi, 32)?, 4usize)
    } else {
        return None;
    };
    if compression != 0 || width <= 0 || height == 0 {
        return None;
    }
    let (w, h) = (width.unsigned_abs(), height.unsigned_abs());
    let px = u64::from(w) * u64::from(h);
    if px > budget || px > MAX_BITMAP_PIXELS {
        return None;
    }
    let palette: Vec<[u8; 3]> = if bpp <= 8 {
        let n = if used == 0 { 1usize << bpp } else { (used as usize).min(256) };
        (0..n)
            .map_while(|i| {
                let at = header.checked_add(i.checked_mul(entry)?)?;
                Some([*bmi.get(at + 2)?, *bmi.get(at + 1)?, *bmi.get(at)?])
            })
            .collect()
    } else {
        Vec::new()
    };
    if !matches!(bpp, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    let stride = (u64::from(w) * u64::from(bpp)).div_ceil(32) * 4;
    if stride.checked_mul(u64::from(h))? > bits.len() as u64 {
        return None;
    }
    let stride = stride as usize;
    let bottom_up = height > 0;
    let mut out = Vec::with_capacity((px as usize).checked_mul(4)?);
    for row in 0..h as usize {
        let src_row = if bottom_up { h as usize - 1 - row } else { row };
        let line = bits.get(src_row * stride..(src_row + 1) * stride)?;
        for x in 0..w as usize {
            let rgb = match bpp {
                1 | 4 | 8 => {
                    let bit = x * bpp as usize;
                    let byte = *line.get(bit / 8)?;
                    let shift = 8 - bpp as usize - (bit % 8);
                    let idx = (byte >> shift) & ((1u16 << bpp) - 1) as u8;
                    palette.get(idx as usize).copied().unwrap_or([0, 0, 0])
                }
                16 => {
                    let v = u16_at(line, x * 2)?;
                    let c = |s: u16| (((v >> s) & 0x1F) as u32 * 255 / 31) as u8;
                    [c(10), c(5), c(0)]
                }
                24 => [*line.get(x * 3 + 2)?, *line.get(x * 3 + 1)?, *line.get(x * 3)?],
                _ => [*line.get(x * 4 + 2)?, *line.get(x * 4 + 1)?, *line.get(x * 4)?],
            };
            out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Some((out, w, h, bottom_up))
}

/// The `src` = [x, y, w, h] part of a top-down RGBA bitmap. For a bottom-up DIB the source y is
/// measured from the bottom row. A source rectangle that is empty or covers the whole bitmap
/// (or reaches outside it) keeps the whole bitmap.
fn crop(rgba: Vec<u8>, w: u32, h: u32, bottom_up: bool, src: [i32; 4]) -> Option<(Vec<u8>, u32, u32)> {
    let [sx, sy, cw, ch] = src;
    if sx < 0 || sy < 0 || cw <= 0 || ch <= 0 {
        return Some((rgba, w, h));
    }
    let (sx, sy, cw, ch) = (sx as u32, sy as u32, cw as u32, ch as u32);
    if sx.checked_add(cw)? > w || sy.checked_add(ch)? > h || (cw == w && ch == h) {
        return Some((rgba, w, h));
    }
    let top = if bottom_up { h - sy - ch } else { sy };
    let mut out = Vec::with_capacity(cw as usize * ch as usize * 4);
    for row in top..top + ch {
        let start = (row as usize * w as usize + sx as usize) * 4;
        out.extend_from_slice(rgba.get(start..start + cw as usize * 4)?);
    }
    Some((out, cw, ch))
}

fn flip(rgba: Vec<u8>, w: u32, h: u32, fx: bool, fy: bool) -> Vec<u8> {
    if !fx && !fy {
        return rgba;
    }
    let (w, h) = (w as usize, h as usize);
    let mut out = Vec::with_capacity(rgba.len());
    for row in 0..h {
        let r = if fy { h - 1 - row } else { row };
        let line = rgba.get(r * w * 4..(r + 1) * w * 4).unwrap_or_default();
        if fx {
            for p in line.as_chunks::<4>().0.iter().rev() {
                out.extend_from_slice(p);
            }
        } else {
            out.extend_from_slice(line);
        }
    }
    out
}

#[cfg(test)]
mod tests;
