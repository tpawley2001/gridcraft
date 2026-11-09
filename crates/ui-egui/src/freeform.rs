//! Freeform shape geometry: DrawingML custom-geometry paths (ECMA-376 Part 1 §20.1.9.15)
//! flattened to screen points, and closed outlines triangulated for filling.

use egui::{Pos2, Rect, pos2};
use gridcraft_engine::model::{GeometryPath, PathCommand, PathShade};

/// One subpath of a freeform in screen coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct FreeformOutline {
    pub points: Vec<Pos2>,
    pub closed: bool,
    pub fill: bool,
    pub stroke: bool,
    /// The path's fill shading, applied to the shape fill colour.
    pub shade: PathShade,
}

/// Segments per Bézier curve.
const CURVE_SEGMENTS: u32 = 16;
/// Arc flattening step, in degrees.
const ARC_STEP_DEG: f64 = 5.0;
/// Polygons with more vertices than this are not filled: the intersection check and ear clipping
/// are quadratic or worse, and they run every frame.
const MAX_FILL_POINTS: usize = 2048;

/// Flattens every path with w > 0 and h > 0 into screen points inside `r` (x = r.left + r.width ·
/// x / path.w, same for y). One outline per subpath: a MoveTo starts a new one and Close ends it
/// with `closed = true`. Curves become at least 16 segments, arcs one segment per 5°.
pub fn outlines(paths: &[GeometryPath], r: Rect) -> Vec<FreeformOutline> {
    let mut out = Vec::new();
    for path in paths.iter().filter(|p| p.w > 0 && p.h > 0) {
        let (w, h) = (path.w as f64, path.h as f64);
        let at = |(x, y): (f64, f64)| pos2((r.left() as f64 + r.width() as f64 * x / w) as f32, (r.top() as f64 + r.height() as f64 * y / h) as f32);
        let f = |(x, y): (i64, i64)| (x as f64, y as f64);
        let mut push = |pts: &mut Vec<Pos2>, closed: bool| {
            if !pts.is_empty() {
                out.push(FreeformOutline { points: std::mem::take(pts), closed, fill: path.fill, stroke: path.stroke, shade: path.shade });
            }
        };
        let mut cur: Vec<Pos2> = Vec::new();
        // Current point and subpath start, in path units.
        let mut pen = (0.0, 0.0);
        let mut start = (0.0, 0.0);
        for c in &path.commands {
            if !matches!(c, PathCommand::MoveTo(..) | PathCommand::Close) && cur.is_empty() {
                // A subpath drawn without a MoveTo starts at the current point.
                cur.push(at(pen));
                start = pen;
            }
            match c {
                PathCommand::MoveTo(x, y) => {
                    push(&mut cur, false);
                    pen = f((*x, *y));
                    start = pen;
                    cur.push(at(pen));
                }
                PathCommand::LineTo(x, y) => {
                    pen = f((*x, *y));
                    cur.push(at(pen));
                }
                PathCommand::CubicBezTo([c1, c2, e]) => {
                    let (p0, p1, p2, p3) = (pen, f(*c1), f(*c2), f(*e));
                    for i in 1..CURVE_SEGMENTS {
                        let t = i as f64 / CURVE_SEGMENTS as f64;
                        let u = 1.0 - t;
                        let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        cur.push(at((a * p0.0 + b * p1.0 + cc * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + cc * p2.1 + d * p3.1)));
                    }
                    pen = p3;
                    cur.push(at(pen));
                }
                PathCommand::QuadBezTo([c1, e]) => {
                    let (p0, p1, p2) = (pen, f(*c1), f(*e));
                    for i in 1..CURVE_SEGMENTS {
                        let t = i as f64 / CURVE_SEGMENTS as f64;
                        let u = 1.0 - t;
                        let (a, b, cc) = (u * u, 2.0 * u * t, t * t);
                        cur.push(at((a * p0.0 + b * p1.0 + cc * p2.0, a * p0.1 + b * p1.1 + cc * p2.1)));
                    }
                    pen = p2;
                    cur.push(at(pen));
                }
                PathCommand::ArcTo { w_r, h_r, st_ang, sw_ang } => {
                    // Angles are 60000ths of a degree, clockwise on screen (y grows downward). A
                    // sweep beyond a full turn draws nothing more, so it is clamped.
                    let (wr, hr) = (*w_r as f64, *h_r as f64);
                    let st = (*st_ang as f64 / 60_000.0).to_radians();
                    let sw_deg = (*sw_ang as f64 / 60_000.0).clamp(-360.0, 360.0);
                    let centre = (pen.0 - wr * st.cos(), pen.1 - hr * st.sin());
                    let n = ((sw_deg.abs() / ARC_STEP_DEG).ceil() as u32).max(1);
                    let sw = sw_deg.to_radians();
                    for i in 1..=n {
                        let th = st + sw * i as f64 / n as f64;
                        pen = (centre.0 + wr * th.cos(), centre.1 + hr * th.sin());
                        cur.push(at(pen));
                    }
                }
                PathCommand::Close => {
                    push(&mut cur, true);
                    pen = start;
                }
            }
        }
        push(&mut cur, false);
    }
    out
}

/// Twice the signed area of triangle `a b c` (positive when clockwise on screen).
fn cross(a: Pos2, b: Pos2, c: Pos2) -> f64 {
    let (ax, ay, bx, by, cx, cy) = (a.x as f64, a.y as f64, b.x as f64, b.y as f64, c.x as f64, c.y as f64);
    (bx - ax) * (cy - ay) - (cx - ax) * (by - ay)
}

/// Whether segments `a b` and `c d` cross at a point inside both.
fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let (d1, d2, d3, d4) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Ear-clipping triangulation of a simple polygon (either winding, concave allowed). Returns index
/// triples into `points`; `None` when the polygon is degenerate or self-intersecting (fewer than
/// 3 distinct points, zero area, two non-adjacent edges that cross, or no ear found).
pub fn triangulate(points: &[Pos2]) -> Option<Vec<[usize; 3]>> {
    // Vertices, without consecutive repeats (the closing point included).
    let mut idx: Vec<usize> = Vec::with_capacity(points.len().min(MAX_FILL_POINTS));
    for (i, p) in points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        if idx.last().and_then(|&j| points.get(j)) != Some(p) {
            idx.push(i);
        }
        if idx.len() > MAX_FILL_POINTS {
            return None;
        }
    }
    while idx.len() > 1 && idx.first().and_then(|&i| points.get(i)) == idx.last().and_then(|&i| points.get(i)) {
        idx.pop();
    }
    let n = idx.len();
    if n < 3 {
        return None;
    }
    let pt = |k: usize| idx.get(k % n).and_then(|&i| points.get(i)).copied().unwrap_or(Pos2::ZERO);
    let area2: f64 = (0..n).map(|k| cross(Pos2::ZERO, pt(k), pt(k + 1))).sum();
    if area2.abs() < 1e-9 {
        return None;
    }
    // Two non-adjacent edges that cross make the polygon self-intersecting.
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_cross(pt(i), pt(i + 1), pt(j), pt(j + 1)) {
                return None;
            }
        }
    }
    let orient = area2.signum();
    let mut ring = idx;
    let mut tris = Vec::with_capacity(ring.len().saturating_sub(2));
    while ring.len() > 3 {
        let m = ring.len();
        let at = |k: usize| ring.get(k % m).copied().unwrap_or(0);
        let p = |i: usize| points.get(i).copied().unwrap_or(Pos2::ZERO);
        let mut clipped = None;
        for k in 0..m {
            let (ia, ib, ic) = (at(k + m - 1), at(k), at(k + 1));
            let (a, b, c) = (p(ia), p(ib), p(ic));
            let turn = cross(a, b, c) * orient;
            if turn.abs() <= 1e-9 {
                // A collinear vertex adds nothing: drop it without a triangle.
                clipped = Some((k, None));
                break;
            }
            if turn < 0.0 {
                continue; // reflex
            }
            let blocked = ring.iter().any(|&j| {
                let q = p(j);
                j != ia
                    && j != ib
                    && j != ic
                    && q != a
                    && q != b
                    && q != c
                    && cross(a, b, q) * orient >= 0.0
                    && cross(b, c, q) * orient >= 0.0
                    && cross(c, a, q) * orient >= 0.0
            });
            if !blocked {
                clipped = Some((k, Some([ia, ib, ic])));
                break;
            }
        }
        let (k, tri) = clipped?;
        tris.extend(tri);
        ring.remove(k);
    }
    if let [a, b, c] = ring[..]
        && cross(points.get(a).copied()?, points.get(b).copied()?, points.get(c).copied()?).abs() > 1e-9
    {
        tris.push([a, b, c]);
    }
    if tris.is_empty() { None } else { Some(tris) }
}

/// Fill triangles for one path: its closed subpaths filled together with the even-odd rule, so a
/// subpath inside another is a hole (and one inside that hole is filled again).
pub fn fill_triangles(path: &GeometryPath, r: Rect) -> Vec<[Pos2; 3]> {
    let _ = (path, r);
    Vec::new() // stub: implemented by the task
}

#[cfg(test)]
#[path = "freeform_hole_tests.rs"]
mod freeform_hole_tests;
