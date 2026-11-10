//! DrawingML preset geometries (`a:prstGeom`, ECMA-376 Part 1 §20.1.9.18) as outline paths, so a
//! preset kept in [`ShapeGeometry`] can be drawn and printed like a freeform.
use super::{GeometryPath, PathCommand, PathShade, ShapeGeometry};

/// The preset's outline in a `w` × `h` box, or `None` for a preset this doesn't know, a custom
/// geometry, or an empty box.
///
/// Guides follow presetShapeDefinitions: integer arithmetic, multiply before divide, truncating
/// like DrawingML's `*/`. Intermediates are `i128` so a hostile box size cannot overflow.
pub fn preset_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if !g.paths.is_empty() || w <= 0 || h <= 0 {
        return None;
    }
    let (wi, hi) = (i128::from(w), i128::from(h));
    let ss = wi.min(hi);
    let (l, t, r, b, hc, vc) = (0, 0, wi, hi, wi / 2, hi / 2);
    // `x * y / z`, truncating; 0 when `z` is zero.
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let adj = |name: &str, default: i128, max: i128| adjust(g, name, default).clamp(0, max.max(0));
    // Every vertex lies in 0..=w × 0..=h because the adjusts are pinned, so this never clamps.
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { 0 } else { i64::MAX });
    if g.preset == "cube" {
        let a = adj("adj", 25000, 100000);
        let y1 = md(ss, a, 100000);
        let (y4, x4) = (b - y1, r - y1);
        let m = |x: i128, y: i128| PathCommand::MoveTo(to_i64(x), to_i64(y));
        let ln = |x: i128, y: i128| PathCommand::LineTo(to_i64(x), to_i64(y));
        let path = |fill: bool, stroke: bool, shade: PathShade, commands: Vec<PathCommand>| GeometryPath { w, h, fill, stroke, shade, commands };
        let close = PathCommand::Close;
        return Some(vec![
            path(true, false, PathShade::Norm, vec![m(l, y1), ln(x4, y1), ln(x4, b), ln(l, b), close.clone()]),
            path(true, false, PathShade::DarkenLess, vec![m(x4, y1), ln(r, t), ln(r, y4), ln(x4, b), close.clone()]),
            path(true, false, PathShade::LightenLess, vec![m(l, y1), ln(y1, t), ln(r, t), ln(x4, y1), close.clone()]),
            path(
                false,
                true,
                PathShade::Norm,
                vec![m(l, y1), ln(y1, t), ln(r, t), ln(r, y4), ln(x4, b), ln(l, b), close, m(l, y1), ln(x4, y1), ln(r, t), m(x4, y1), ln(x4, b)],
            ),
        ]);
    }
    if let Some(paths) = flowchart_curved(&g.preset, w, h) {
        return Some(paths);
    }
    // Predefined process and internal storage: a filled rectangle, two open inner lines, and the
    // rectangle's outline.
    if g.preset == "flowChartPredefinedProcess" || g.preset == "flowChartInternalStorage" {
        let m = |x: i128, y: i128| PathCommand::MoveTo(to_i64(x), to_i64(y));
        let ln = |x: i128, y: i128| PathCommand::LineTo(to_i64(x), to_i64(y));
        let path = |fill: bool, stroke: bool, commands: Vec<PathCommand>| GeometryPath { w, h, fill, stroke, shade: PathShade::Norm, commands };
        let rect = || vec![m(l, t), ln(r, t), ln(r, b), ln(l, b), PathCommand::Close];
        let x1 = md(wi, 1, 8);
        let lines = if g.preset == "flowChartPredefinedProcess" {
            let x2 = md(wi, 7, 8);
            vec![m(x1, t), ln(x1, b), m(x2, t), ln(x2, b)]
        } else {
            let y1 = md(hi, 1, 8);
            vec![m(x1, t), ln(x1, b), m(l, y1), ln(r, y1)]
        };
        return Some(vec![path(true, false, rect()), path(false, true, lines), path(false, true, rect())]);
    }
    // Donut and frame: one filled path of two subpaths, the inner one wound the other way, so it
    // is a hole.
    if g.preset == "donut" {
        let a = adj("adj", 25000, 50000);
        let dr = md(ss, a, 100000);
        let (wd2, hd2) = (hc, vc);
        let (iwd2, ihd2) = ((wd2 - dr).max(0), (hd2 - dr).max(0));
        let (cd4, cd2, cd34) = (5_400_000, 10_800_000, 16_200_000);
        let arc = |w_r: i128, h_r: i128, st_ang: i64, sw_ang: i64| PathCommand::ArcTo { w_r: to_i64(w_r), h_r: to_i64(h_r), st_ang, sw_ang };
        let commands = vec![
            PathCommand::MoveTo(to_i64(l), to_i64(vc)),
            arc(wd2, hd2, cd2, cd4),
            arc(wd2, hd2, cd34, cd4),
            arc(wd2, hd2, 0, cd4),
            arc(wd2, hd2, cd4, cd4),
            PathCommand::Close,
            PathCommand::MoveTo(to_i64(dr), to_i64(vc)),
            arc(iwd2, ihd2, cd2, -cd4),
            arc(iwd2, ihd2, cd4, -cd4),
            arc(iwd2, ihd2, 0, -cd4),
            arc(iwd2, ihd2, cd34, -cd4),
            PathCommand::Close,
        ];
        return Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }]);
    }
    if g.preset == "frame" {
        let a1 = adj("adj1", 12500, 50000);
        let x1 = md(ss, a1, 100000);
        let (x4, y4) = (r - x1, b - x1);
        let m = |x: i128, y: i128| PathCommand::MoveTo(to_i64(x), to_i64(y));
        let ln = |x: i128, y: i128| PathCommand::LineTo(to_i64(x), to_i64(y));
        let commands =
            vec![m(l, t), ln(r, t), ln(r, b), ln(l, b), PathCommand::Close, m(x1, x1), ln(x1, y4), ln(x4, y4), ln(x4, x1), PathCommand::Close];
        return Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }]);
    }
    let pts: Vec<(i128, i128)> = match g.preset.as_str() {
        "rightArrow" => {
            let a1 = adj("adj1", 50000, 100000);
            let a2 = adj("adj2", 50000, md(100000, wi, ss));
            let x1 = r - md(ss, a2, 100000);
            let dy1 = md(hi, a1, 200000);
            let (y1, y2) = (vc - dy1, vc + dy1);
            vec![(l, y1), (x1, y1), (x1, t), (r, vc), (x1, b), (x1, y2), (l, y2)]
        }
        "leftArrow" => {
            let a1 = adj("adj1", 50000, 100000);
            let a2 = adj("adj2", 50000, md(100000, wi, ss));
            let x2 = l + md(ss, a2, 100000);
            let dy1 = md(hi, a1, 200000);
            let (y1, y2) = (vc - dy1, vc + dy1);
            vec![(l, vc), (x2, t), (x2, y1), (r, y1), (r, y2), (x2, y2), (x2, b)]
        }
        "upArrow" => {
            let a1 = adj("adj1", 50000, 100000);
            let a2 = adj("adj2", 50000, md(100000, hi, ss));
            let dx1 = md(wi, a1, 200000);
            let (x1, x2) = (hc - dx1, hc + dx1);
            let y2 = t + md(ss, a2, 100000);
            vec![(l, y2), (hc, t), (r, y2), (x2, y2), (x2, b), (x1, b), (x1, y2)]
        }
        "downArrow" => {
            let a1 = adj("adj1", 50000, 100000);
            let a2 = adj("adj2", 50000, md(100000, hi, ss));
            let dx1 = md(wi, a1, 200000);
            let (x1, x2) = (hc - dx1, hc + dx1);
            let y1 = b - md(ss, a2, 100000);
            vec![(x1, t), (x2, t), (x2, y1), (r, y1), (hc, b), (l, y1), (x1, y1)]
        }
        "leftRightArrow" => {
            let a1 = adj("adj1", 50000, 100000);
            let a2 = adj("adj2", 50000, md(50000, wi, ss));
            let dx2 = md(ss, a2, 100000);
            let (x2, x3) = (l + dx2, r - dx2);
            let dy = md(hi, a1, 200000);
            let (y1, y2) = (vc - dy, vc + dy);
            vec![(l, vc), (x2, t), (x2, y1), (x3, y1), (x3, t), (r, vc), (x3, b), (x3, y2), (x2, y2), (x2, b)]
        }
        "chevron" => {
            let a = adj("adj", 50000, md(100000, wi, ss));
            let x1 = md(ss, a, 100000);
            let x2 = r - x1;
            vec![(l, t), (x2, t), (r, vc), (x2, b), (l, b), (x1, vc)]
        }
        "homePlate" => {
            let a = adj("adj", 50000, md(100000, wi, ss));
            let x1 = r - md(ss, a, 100000);
            vec![(l, t), (x1, t), (r, vc), (x1, b), (l, b)]
        }
        "diamond" => vec![(l, vc), (hc, t), (r, vc), (hc, b)],
        "parallelogram" => {
            let a = adj("adj", 25000, md(100000, wi, ss));
            let x2 = md(ss, a, 100000);
            let x5 = r - x2;
            vec![(l, b), (x2, t), (r, t), (x5, b)]
        }
        "trapezoid" => {
            let a = adj("adj", 25000, md(50000, wi, ss));
            let x2 = md(ss, a, 100000);
            let x3 = r - x2;
            vec![(l, b), (x2, t), (x3, t), (r, b)]
        }
        "octagon" => {
            let a = adj("adj", 29289, 50000);
            let x1 = md(ss, a, 100000);
            let (x2, y2) = (r - x1, b - x1);
            vec![(l, x1), (x1, t), (x2, t), (r, x1), (r, y2), (x2, b), (x1, b), (l, y2)]
        }
        "plus" => {
            let a = adj("adj", 25000, 50000);
            let x1 = md(ss, a, 100000);
            let (x2, y2) = (r - x1, b - x1);
            vec![(l, x1), (x1, x1), (x1, t), (x2, t), (x2, x1), (r, x1), (r, y2), (x2, y2), (x2, b), (x1, b), (x1, y2), (l, y2)]
        }
        "triangle" => {
            let a = adj("adj", 50000, 100000);
            let x2 = md(wi, a, 100000);
            vec![(l, b), (x2, t), (r, b)]
        }
        "rtTriangle" => vec![(l, t), (r, b), (l, b)],
        "notchedRightArrow" => {
            let a1 = adj("adj1", 50000, 100000);
            let a2 = adj("adj2", 50000, md(100000, wi, ss));
            let dx2 = md(ss, a2, 100000);
            let x2 = r - dx2;
            let dy1 = md(hi, a1, 200000);
            let (y1, y2) = (vc - dy1, vc + dy1);
            // `vc` is zero for a one-unit-high box; `md` then yields 0.
            let x1 = md(dy1, dx2, vc);
            vec![(l, y1), (x2, y1), (x2, t), (r, vc), (x2, b), (x2, y2), (l, y2), (x1, vc)]
        }
        "hexagon" => {
            let a = adj("adj", 25000, md(50000, wi, ss));
            let x1 = md(ss, a, 100000);
            let x2 = r - x1;
            vec![(l, vc), (x1, t), (x2, t), (r, vc), (x2, b), (x1, b)]
        }
        "pentagon" | "star5" => {
            let hf = adjust(g, "hf", 105146);
            let vf = adjust(g, "vf", 110557);
            let swd2 = md(hc, hf, 100000);
            let shd2 = md(vc, vf, 100000);
            let svc = md(vc, vf, 100000);
            let angle1 = 18_f64.to_radians();
            let angle2 = 306_f64.to_radians();
            let dx1 = (swd2 as f64 * angle1.cos()) as i128;
            let dx2 = (swd2 as f64 * angle2.cos()) as i128;
            let dy1 = (shd2 as f64 * angle1.sin()) as i128;
            let dy2 = (shd2 as f64 * angle2.sin()) as i128;
            let (x1, x2, x3, x4) = (hc - dx1, hc - dx2, hc + dx2, hc + dx1);
            let (y1, y2) = (svc - dy1, svc - dy2);
            let points = if g.preset == "star5" {
                let a = adj("adj", 19098, 50000);
                let iwd2 = md(swd2, a, 50000);
                let ihd2 = md(shd2, a, 50000);
                let angle3 = 342_f64.to_radians();
                let angle4 = 54_f64.to_radians();
                let sdx1 = (iwd2 as f64 * angle3.cos()) as i128;
                let sdx2 = (iwd2 as f64 * angle4.cos()) as i128;
                let sdy1 = (ihd2 as f64 * angle4.sin()) as i128;
                let sdy2 = (ihd2 as f64 * angle3.sin()) as i128;
                let (sx1, sx2, sx3, sx4) = (hc - sdx1, hc - sdx2, hc + sdx2, hc + sdx1);
                let (sy1, sy2, sy3) = (svc - sdy1, svc - sdy2, svc + ihd2);
                vec![(x1, y1), (sx2, sy1), (hc, t), (sx3, sy1), (x4, y1), (sx4, sy2), (x3, y2), (hc, sy3), (x2, y2), (sx1, sy2)]
            } else {
                vec![(x1, y1), (hc, t), (x4, y1), (x3, y2), (x2, y2)]
            };
            // Scale guides can extend vertices beyond the box, even with the default rounding.
            points.into_iter().map(|(x, y)| (x.clamp(l, r), y.clamp(t, b))).collect()
        }
        "snip1Rect" => {
            let a = adj("adj", 16667, 50000);
            let dx1 = md(ss, a, 100000);
            let x1 = r - dx1;
            vec![(l, t), (x1, t), (r, dx1), (r, b), (l, b)]
        }
        "star4" => {
            let a = adj("adj", 12500, 50000);
            let iwd2 = md(hc, a, 50000);
            let ihd2 = md(vc, a, 50000);
            let angle = 45_f64.to_radians();
            let sdx = (iwd2 as f64 * angle.cos()) as i128;
            let sdy = (ihd2 as f64 * angle.sin()) as i128;
            let (sx1, sx2) = (hc - sdx, hc + sdx);
            let (sy1, sy2) = (vc - sdy, vc + sdy);
            vec![(l, vc), (sx1, sy1), (hc, t), (sx2, sy1), (r, vc), (sx2, sy2), (hc, b), (sx1, sy2)]
        }
        "star6" | "star7" | "star8" | "star10" | "star12" | "star16" | "star24" | "star32" => star_points(g, wi, hi)?,
        "irregularSeal1" | "irregularSeal2" => {
            let table: &[(i128, i128)] = if g.preset == "irregularSeal1" { &EXPLOSION1 } else { &EXPLOSION2 };
            table.iter().map(|&(x, y)| (md(wi, x, 21600), md(hi, y, 21600))).collect()
        }
        "mathPlus" | "mathMinus" | "mathMultiply" | "mathEqual" | "mathNotEqual" => return math_paths(g, w, h),
        preset => {
            let (pw, ph, table) = flowchart_polygon(preset)?;
            table.iter().map(|&(x, y)| (md(wi, x, pw), md(hi, y, ph))).collect()
        }
    };
    let mut commands = Vec::with_capacity(pts.len() + 1);
    for (i, &(x, y)) in pts.iter().enumerate() {
        let (x, y) = (to_i64(x), to_i64(y));
        commands.push(if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) });
    }
    commands.push(PathCommand::Close);
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// Curved flowchart presets (terminator, alternate process, delay, document): one filled and
/// stroked path of lines, arcs and a Bézier. `w` and `h` are positive.
fn flowchart_curved(preset: &str, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    use PathCommand::{ArcTo, Close, CubicBezTo, LineTo, MoveTo};
    let (wi, hi) = (i128::from(w), i128::from(h));
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { 0 } else { i64::MAX });
    // Path-box coordinates scaled to the shape box; a 21600 box for the fixed-path presets.
    let sx = |x: i128, pw: i128| to_i64(md(x, wi, pw));
    let sy = |y: i128, ph: i128| to_i64(md(y, hi, ph));
    let (cd4, cd2, cd34) = (5_400_000, 10_800_000, 16_200_000);
    let commands = match preset {
        "flowChartTerminator" => {
            let (x, y) = (|v| sx(v, 21600), |v| sy(v, 21600));
            vec![
                MoveTo(x(3475), 0),
                LineTo(x(18125), 0),
                ArcTo { w_r: x(3475), h_r: y(10800), st_ang: cd34, sw_ang: cd2 },
                LineTo(x(3475), y(21600)),
                ArcTo { w_r: x(3475), h_r: y(10800), st_ang: cd4, sw_ang: cd2 },
                Close,
            ]
        }
        "flowChartAlternateProcess" => {
            let ssd6 = md(wi.min(hi), 1, 6);
            let s = to_i64(ssd6);
            let arc = |st_ang| ArcTo { w_r: s, h_r: s, st_ang, sw_ang: cd4 };
            vec![MoveTo(0, s), arc(cd2), LineTo(to_i64(wi - ssd6), 0), arc(cd34), LineTo(w, to_i64(hi - ssd6)), arc(0), LineTo(s, h), arc(cd4), Close]
        }
        "flowChartDelay" => {
            let (wd2, hd2) = (to_i64(wi / 2), to_i64(hi / 2));
            vec![MoveTo(0, 0), LineTo(wd2, 0), ArcTo { w_r: wd2, h_r: hd2, st_ang: cd34, sw_ang: cd2 }, LineTo(0, h), Close]
        }
        "flowChartDocument" => {
            let (x, y) = (|v| sx(v, 21600), |v| sy(v, 21600));
            // The second control point lies below the box (23922 > 21600), as the preset defines.
            vec![
                MoveTo(0, 0),
                LineTo(x(21600), 0),
                LineTo(x(21600), y(17322)),
                CubicBezTo([(x(10800), y(17322)), (x(10800), y(23922)), (0, y(20172))]),
                Close,
            ]
        }
        _ => return None,
    };
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// A fixed polygon in its own path box: path box `w`, `h`, then the vertices.
type FixedPolygon = (i128, i128, &'static [(i128, i128)]);

/// Straight-edged flowchart presets: the path box and the fixed polygon in it.
fn flowchart_polygon(preset: &str) -> Option<FixedPolygon> {
    Some(match preset {
        "flowChartDecision" => (2, 2, &[(0, 1), (1, 0), (2, 1), (1, 2)]),
        "flowChartInputOutput" => (5, 5, &[(0, 5), (1, 0), (5, 0), (4, 5)]),
        "flowChartPreparation" => (10, 10, &[(0, 5), (2, 0), (8, 0), (10, 5), (8, 10), (2, 10)]),
        "flowChartManualInput" => (5, 5, &[(0, 1), (5, 0), (5, 5), (0, 5)]),
        "flowChartManualOperation" => (5, 5, &[(0, 0), (5, 0), (4, 5), (1, 5)]),
        "flowChartMerge" => (2, 2, &[(0, 0), (2, 0), (1, 2)]),
        "flowChartExtract" => (2, 2, &[(0, 2), (1, 0), (2, 2)]),
        "flowChartPunchedCard" => (5, 5, &[(0, 1), (1, 0), (5, 0), (5, 5), (0, 5)]),
        "flowChartOffpageConnector" => (10, 10, &[(0, 0), (10, 0), (10, 8), (5, 10), (0, 8)]),
        _ => return None,
    })
}

/// A guide factor: `*/ r n 100000` for `F(n)`, `cos r a` for `T(a)` with `a` in degrees.
#[derive(Clone, Copy)]
enum K {
    F(i128),
    T(f64),
}

/// The vertices of the 6- to 32-point star presets (`star6` … `star32`), pinned into the box, or
/// `None` for another preset.
fn star_points(g: &ShapeGeometry, wi: i128, hi: i128) -> Option<Vec<(i128, i128)>> {
    let (t, b, hc, vc) = (0, hi, wi / 2, hi / 2);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let cos = |r: i128, deg: f64| (r as f64 * deg.to_radians().cos()) as i128;
    let sin = |r: i128, deg: f64| (r as f64 * deg.to_radians().sin()) as i128;
    let k = |r: i128, f: K| match f {
        K::F(n) => md(r, n, 100000),
        K::T(deg) => cos(r, deg),
    };
    let a = |default: i128| adjust(g, "adj", default).clamp(0, 50000);
    // Upper-left quadrant from the left vertex up to (not including) the top one, as
    // (left x, right x, upper y, lower y); the other quadrants mirror it.
    let quad = |q: &[(i128, i128, i128, i128)]| -> Vec<(i128, i128)> {
        let mut pts: Vec<(i128, i128)> = q.iter().map(|p| (p.0, p.2)).collect();
        pts.push((hc, t));
        pts.extend(q.iter().rev().map(|p| (p.1, p.2)));
        pts.extend(q.iter().skip(1).map(|p| (p.1, p.3)));
        pts.push((hc, b));
        pts.extend(q.iter().skip(1).rev().map(|p| (p.0, p.3)));
        pts
    };
    // Outer offsets `outer` on `wd2`/`hd2` and inner ones `inner` on `iwd2`/`ihd2`, alternating; each
    // list is read forwards for x and backwards for y.
    let radial = |adj: i128, outer: &[K], inner: &[K]| -> Vec<(i128, i128)> {
        let (iwd2, ihd2) = (md(hc, adj, 50000), md(vc, adj, 50000));
        let mut q = vec![(0, wi, vc, vc)];
        let off = |rx: i128, ry: i128, list: &[K], i: usize| {
            let kx = list.get(i).map_or(0, |&f| k(rx, f));
            let ky = list.get(list.len().saturating_sub(i + 1)).map_or(0, |&f| k(ry, f));
            (hc - kx, hc + kx, vc - ky, vc + ky)
        };
        for i in 0..inner.len() {
            q.push(off(iwd2, ihd2, inner, i));
            if i < outer.len() {
                q.push(off(hc, vc, outer, i));
            }
        }
        quad(&q)
    };
    let pts = match g.preset.as_str() {
        "star6" => {
            let a = a(28868);
            let swd2 = md(hc, adjust(g, "hf", 115470), 100000);
            let dx1 = cos(swd2, 30.0);
            let (x1, x2) = (hc - dx1, hc + dx1);
            let (hd4, y2) = (hi / 4, vc + hi / 4);
            let iwd2 = md(swd2, a, 50000);
            let ihd2 = md(vc, a, 50000);
            let sdx2 = md(iwd2, 1, 2);
            let sdy1 = sin(ihd2, 60.0);
            quad(&[(hc - iwd2, hc + iwd2, vc, vc), (x1, x2, hd4, y2), (hc - sdx2, hc + sdx2, vc - sdy1, vc + sdy1)])
        }
        "star7" => {
            let a = a(34601);
            let swd2 = md(hc, adjust(g, "hf", 102572), 100000);
            let shd2 = md(vc, adjust(g, "vf", 105210), 100000);
            let svc = shd2;
            let (dx1, dx2, dx3) = (md(swd2, 97493, 100000), md(swd2, 78183, 100000), md(swd2, 43388, 100000));
            let (dy1, dy2, dy3) = (md(shd2, 62349, 100000), md(shd2, 22252, 100000), md(shd2, 90097, 100000));
            let (x1, x2, x3, x4, x5, x6) = (hc - dx1, hc - dx2, hc - dx3, hc + dx3, hc + dx2, hc + dx1);
            let (y1, y2, y3) = (svc - dy1, svc + dy2, svc + dy3);
            let iwd2 = md(swd2, a, 50000);
            let ihd2 = md(shd2, a, 50000);
            let (sdx1, sdx2, sdx3) = (md(iwd2, 97493, 100000), md(iwd2, 78183, 100000), md(iwd2, 43388, 100000));
            let (sx1, sx2, sx3, sx4, sx5, sx6) = (hc - sdx1, hc - sdx2, hc - sdx3, hc + sdx3, hc + sdx2, hc + sdx1);
            let (sdy1, sdy2, sdy3) = (md(ihd2, 90097, 100000), md(ihd2, 22252, 100000), md(ihd2, 62349, 100000));
            let (sy1, sy2, sy3, sy4) = (svc - sdy1, svc - sdy2, svc + sdy3, svc + ihd2);
            vec![
                (x1, y2),
                (sx1, sy2),
                (x2, y1),
                (sx3, sy1),
                (hc, t),
                (sx4, sy1),
                (x5, y1),
                (sx6, sy2),
                (x6, y2),
                (sx5, sy3),
                (x4, y3),
                (hc, sy4),
                (x3, y3),
                (sx2, sy3),
            ]
        }
        "star8" => radial(a(37500), &[K::T(45.0)], &[K::F(92388), K::F(38268)]),
        "star10" => {
            let a = a(42533);
            let swd2 = md(hc, adjust(g, "hf", 105146), 100000);
            let (dx1, dx2) = (md(swd2, 95106, 100000), md(swd2, 58779, 100000));
            let (dy1, dy2) = (md(vc, 80902, 100000), md(vc, 30902, 100000));
            let iwd2 = md(swd2, a, 50000);
            let ihd2 = md(vc, a, 50000);
            let (sdx1, sdx2) = (md(iwd2, 80902, 100000), md(iwd2, 30902, 100000));
            let (sdy1, sdy2) = (md(ihd2, 95106, 100000), md(ihd2, 58779, 100000));
            quad(&[
                (hc - iwd2, hc + iwd2, vc, vc),
                (hc - dx1, hc + dx1, vc - dy2, vc + dy2),
                (hc - sdx1, hc + sdx1, vc - sdy2, vc + sdy2),
                (hc - dx2, hc + dx2, vc - dy1, vc + dy1),
                (hc - sdx2, hc + sdx2, vc - sdy1, vc + sdy1),
            ])
        }
        "star12" => {
            let a = a(37500);
            let dx1 = cos(hc, 30.0);
            let dy1 = sin(vc, 60.0);
            let iwd2 = md(hc, a, 50000);
            let ihd2 = md(vc, a, 50000);
            let inner = |deg: f64| {
                let (sdx, sdy) = (cos(iwd2, deg), sin(ihd2, deg));
                (hc - sdx, hc + sdx, vc - sdy, vc + sdy)
            };
            quad(&[
                (0, wi, vc, vc),
                inner(15.0),
                (hc - dx1, hc + dx1, hi / 4, md(hi, 3, 4)),
                inner(45.0),
                (wi / 4, md(wi, 3, 4), vc - dy1, vc + dy1),
                inner(75.0),
            ])
        }
        "star16" => radial(a(37500), &[K::F(92388), K::F(70711), K::F(38268)], &[K::F(98079), K::F(83147), K::F(55557), K::F(19509)]),
        "star24" => radial(
            a(37500),
            &[K::T(15.0), K::T(30.0), K::T(45.0), K::F(50000), K::T(75.0)],
            &[K::F(99144), K::F(92388), K::F(79335), K::F(60876), K::F(38268), K::F(13053)],
        ),
        "star32" => radial(
            a(37500),
            &[K::F(98079), K::F(92388), K::F(83147), K::T(45.0), K::F(55557), K::F(38268), K::F(19509)],
            &[K::F(99518), K::F(95694), K::F(88192), K::F(77301), K::F(63439), K::F(47140), K::F(29028), K::F(9802)],
        ),
        _ => return None,
    };
    // Scale guides (`hf`, `vf`) can extend vertices beyond the box.
    Some(pts.into_iter().map(|(x, y)| (x.clamp(0, wi), y.clamp(t, b))).collect())
}

/// Explosion 1 (`irregularSeal1`): a fixed 24-point polygon in a 21600 × 21600 path box.
const EXPLOSION1: [(i128, i128); 24] = [
    (10800, 5800),
    (14522, 0),
    (14155, 5325),
    (18380, 4457),
    (16702, 7315),
    (21097, 8137),
    (17607, 10475),
    (21600, 13290),
    (16837, 12942),
    (18145, 18095),
    (14020, 14457),
    (13247, 19737),
    (10532, 14935),
    (8485, 21600),
    (7715, 15627),
    (4762, 17617),
    (5667, 13937),
    (135, 14587),
    (3722, 11775),
    (0, 8615),
    (4627, 7617),
    (370, 2295),
    (7312, 6320),
    (8352, 2295),
];

/// Explosion 2 (`irregularSeal2`): a fixed 28-point polygon in a 21600 × 21600 path box.
const EXPLOSION2: [(i128, i128); 28] = [
    (11462, 4342),
    (14790, 0),
    (14525, 5777),
    (18007, 3172),
    (16380, 6532),
    (21600, 6645),
    (16985, 9402),
    (18270, 11290),
    (16380, 12310),
    (18877, 15632),
    (14640, 14350),
    (14942, 17370),
    (12180, 15935),
    (11612, 18842),
    (9872, 17370),
    (8700, 19712),
    (7527, 18125),
    (4917, 21600),
    (4805, 18240),
    (1285, 17825),
    (3330, 15370),
    (0, 12877),
    (3935, 11592),
    (1172, 8270),
    (5372, 7817),
    (4502, 3625),
    (8550, 6382),
    (9722, 1887),
];

/// The adjust value `name` (`a:gd fmla="val N"`), or `default` when missing or not a plain value.
fn adjust(g: &ShapeGeometry, name: &str, default: i128) -> i128 {
    g.adjust
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, f)| f.trim().strip_prefix("val "))
        .and_then(|v| v.trim().parse::<i64>().ok())
        .map_or(default, i128::from)
}

/// Equation shapes (`mathPlus`, `mathMinus`, `mathMultiply`, `mathEqual`, `mathNotEqual`): one
/// filled path of closed polygons (`mathEqual` has two, top bar first).
///
/// The guides use trigonometry and square roots, so they are evaluated in `f64`; each vertex is
/// rounded and pinned into the box at the end.
fn math_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if w <= 0 || h <= 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (wf, hf) = (w as f64, h as f64);
    let polygons = match g.preset.as_str() {
        "mathPlus" => vec![math_plus(g, wf, hf)],
        "mathMinus" => vec![math_minus(g, wf, hf)],
        "mathMultiply" => vec![math_multiply(g, wf, hf)],
        "mathEqual" => math_equal(g, wf, hf),
        "mathNotEqual" => vec![math_not_equal(g, wf, hf)],
        _ => return None,
    };
    let mut commands = Vec::new();
    for polygon in polygons {
        for (i, &(x, y)) in polygon.iter().enumerate() {
            let (x, y) = (math_pin(x, w), math_pin(y, h));
            commands.push(if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) });
        }
        commands.push(PathCommand::Close);
    }
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// `v` rounded and pinned into `0..=max`; NaN is 0.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn math_pin(v: f64, max: i64) -> i64 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v >= max as f64 {
        max
    } else {
        (v.round() as i64).clamp(0, max)
    }
}

/// Adjust `name` (or `default`) pinned into `min..=max`, as `f64`.
#[allow(clippy::cast_precision_loss)]
fn math_adj(g: &ShapeGeometry, name: &str, default: i128, min: f64, max: f64) -> f64 {
    (adjust(g, name, default) as f64).clamp(min, max.max(min))
}

/// `mathPlus`: a cross of two bars.
fn math_plus(g: &ShapeGeometry, w: f64, h: f64) -> Vec<(f64, f64)> {
    let (hc, vc, ss) = (w / 2.0, h / 2.0, w.min(h));
    let a1 = math_adj(g, "adj1", 23520, 0.0, 73490.0);
    let dx1 = w * 73490.0 / 200000.0;
    let dy1 = h * 73490.0 / 200000.0;
    let dx2 = ss * a1 / 200000.0;
    let (x1, x2, x3, x4) = (hc - dx1, hc - dx2, hc + dx2, hc + dx1);
    let (y1, y2, y3, y4) = (vc - dy1, vc - dx2, vc + dx2, vc + dy1);
    vec![(x1, y2), (x2, y2), (x2, y1), (x3, y1), (x3, y2), (x4, y2), (x4, y3), (x3, y3), (x3, y4), (x2, y4), (x2, y3), (x1, y3)]
}

/// `mathMinus`: one horizontal bar.
fn math_minus(g: &ShapeGeometry, w: f64, h: f64) -> Vec<(f64, f64)> {
    let (hc, vc) = (w / 2.0, h / 2.0);
    let a1 = math_adj(g, "adj1", 23520, 0.0, 100000.0);
    let dy1 = h * a1 / 200000.0;
    let dx1 = w * 73490.0 / 200000.0;
    let (y1, y2, x1, x2) = (vc - dy1, vc + dy1, hc - dx1, hc + dx1);
    vec![(x1, y1), (x2, y1), (x2, y2), (x1, y2)]
}

/// `mathMultiply`: two bars crossing along the box diagonals.
fn math_multiply(g: &ShapeGeometry, w: f64, h: f64) -> Vec<(f64, f64)> {
    let (r, b, hc, vc, ss) = (w, h, w / 2.0, h / 2.0, w.min(h));
    let a1 = math_adj(g, "adj1", 23520, 0.0, 51965.0);
    let th = ss * a1 / 100000.0;
    let a = h.atan2(w);
    let (sa, ca, ta) = (a.sin(), a.cos(), a.tan());
    let dl = w.hypot(h);
    let rw = dl * 51965.0 / 100000.0;
    let lm = dl - rw;
    let (xm, ym) = (ca * lm / 2.0, sa * lm / 2.0);
    let (dxam, dyam) = (sa * th / 2.0, ca * th / 2.0);
    let (xa, ya, xb, yb) = (xm - dxam, ym + dyam, xm + dxam, ym - dyam);
    let ybc = (hc - xb) * ta;
    let yc = ybc + yb;
    let (xd, xe) = (r - xb, r - xa);
    let xfe = if ta > 0.0 { (vc - ya) / ta } else { 0.0 };
    let (xf, xl) = (xe - xfe, xa + xfe);
    let (yg, yh, yi) = (b - ya, b - yb, b - yc);
    vec![(xa, ya), (xb, yb), (hc, yc), (xd, yb), (xe, ya), (xf, vc), (xe, yg), (xd, yh), (hc, yi), (xb, yh), (xa, yg), (xl, vc)]
}

/// `mathEqual`: two horizontal bars, top first.
fn math_equal(g: &ShapeGeometry, w: f64, h: f64) -> Vec<Vec<(f64, f64)>> {
    let (hc, vc) = (w / 2.0, h / 2.0);
    let a1 = math_adj(g, "adj1", 23520, 0.0, 36745.0);
    let a2 = math_adj(g, "adj2", 11760, 0.0, 100000.0 - a1 * 2.0);
    let dy1 = h * a1 / 100000.0;
    let dy2 = h * a2 / 200000.0;
    let dx1 = w * 73490.0 / 200000.0;
    let (y2, y3) = (vc - dy2, vc + dy2);
    let (y1, y4) = (y2 - dy1, y3 + dy1);
    let (x1, x2) = (hc - dx1, hc + dx1);
    vec![vec![(x1, y1), (x2, y1), (x2, y2), (x1, y2)], vec![(x1, y3), (x2, y3), (x2, y4), (x1, y4)]]
}

/// `mathNotEqual`: two horizontal bars crossed by a slanted stroke.
fn math_not_equal(g: &ShapeGeometry, w: f64, h: f64) -> Vec<(f64, f64)> {
    let (hc, vc, hd2) = (w / 2.0, h / 2.0, h / 2.0);
    let a1 = math_adj(g, "adj1", 23520, 0.0, 50000.0);
    let cr_ang = math_adj(g, "adj2", 6600000, 4200000.0, 6600000.0);
    let a3 = math_adj(g, "adj3", 11760, 0.0, 100000.0 - a1 * 2.0);
    let dy1 = h * a1 / 100000.0;
    let dy2 = h * a3 / 200000.0;
    let dx1 = w * 73490.0 / 200000.0;
    let (x1, x8) = (hc - dx1, hc + dx1);
    let (y2, y3) = (vc - dy2, vc + dy2);
    let (y1, y4) = (y2 - dy1, y3 + dy1);
    let cadj2 = cr_ang - 5400000.0;
    let xadj2 = hd2 * (cadj2 / 60000.0).to_radians().tan();
    let len = xadj2.hypot(hd2);
    let div = |x: f64, z: f64| if z > 0.0 { x / z } else { 0.0 };
    let bhw = div(len * dy1, hd2);
    let x7 = hc + xadj2 - bhw / 2.0;
    let x6 = x7 - div(xadj2 * y1, hd2);
    let x5 = x7 - div(xadj2 * y2, hd2);
    let x4 = x7 - div(xadj2 * y3, hd2);
    let x3 = x7 - div(xadj2 * y4, hd2);
    let (rx7, rx6, rx5, rx4, rx3) = (x7 + bhw, x6 + bhw, x5 + bhw, x4 + bhw, x3 + bhw);
    let dx7 = div(dy1 * hd2, len);
    let (rxt, lxt) = (x7 + dx7, rx7 - dx7);
    let (rx, lx) = if cadj2 > 0.0 { (rxt, x7) } else { (rx7, lxt) };
    let dy3 = div(dy1 * xadj2, len);
    let (ry, ly) = if cadj2 > 0.0 { (dy3, 0.0) } else { (0.0, -dy3) };
    let (dlx, drx, dly, dry) = (w - rx, w - lx, h - ry, h - ly);
    vec![
        (x1, y1),
        (x6, y1),
        (lx, ly),
        (rx, ry),
        (rx6, y1),
        (x8, y1),
        (x8, y2),
        (rx5, y2),
        (rx4, y3),
        (x8, y3),
        (x8, y4),
        (rx3, y4),
        (drx, dry),
        (dlx, dly),
        (x3, y4),
        (x1, y4),
        (x1, y3),
        (x4, y3),
        (x5, y2),
        (x1, y2),
    ]
}
