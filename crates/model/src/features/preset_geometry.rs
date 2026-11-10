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
    // Sort, collate, offline storage, chart markers and tabs: several straight paths, some only
    // filled and some only stroked.
    if let Some(paths) = straight_multipath(&g.preset, wi, hi) {
        let point = |x: i128, y: i128| (to_i64(x.clamp(l, r)), to_i64(y.clamp(t, b)));
        return Some(
            paths
                .into_iter()
                .map(|(fill, stroke, segs)| {
                    let commands = segs
                        .into_iter()
                        .map(|s| match s {
                            Seg::M(x, y) => {
                                let (x, y) = point(x, y);
                                PathCommand::MoveTo(x, y)
                            }
                            Seg::L(x, y) => {
                                let (x, y) = point(x, y);
                                PathCommand::LineTo(x, y)
                            }
                            Seg::Z => PathCommand::Close,
                        })
                        .collect();
                    GeometryPath { w, h, fill, stroke, shade: PathShade::Norm, commands }
                })
                .collect(),
        );
    }
    if let Some(paths) = arc_flowchart_paths(&g.preset, w, h) {
        return Some(paths);
    }
    if let Some(paths) = bracket_paths(g, w, h) {
        return Some(paths);
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
        "upDownArrow" | "quadArrow" | "leftRightUpArrow" | "leftUpArrow" | "bentUpArrow" | "stripedRightArrow" => return block_arrow_paths(g, w, h),
        "heptagon" => heptagon_points(wi, hi, g),
        "decagon" => decagon_points(wi, hi, g),
        "dodecagon" => dodecagon_points(wi, hi),
        "nonIsoscelesTrapezoid" => non_isosceles_trapezoid_points(wi, hi, g),
        "snip2SameRect" => snip_2_same_rect_points(wi, hi, g),
        "snip2DiagRect" => snip_2_diag_rect_points(wi, hi, g),
        "halfFrame" => half_frame_points(wi, hi, g),
        "corner" => corner_points(wi, hi, g),
        "diagStripe" => diag_stripe_points(wi, hi, g),
        "lightningBolt" => lightning_bolt_points(wi, hi),
        "pie" | "chord" | "arc" | "blockArc" | "plaque" => return arc_shape_paths(g, w, h),
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

/// One step of an arc-based preset path: `M`/`L` points and `A` radii (`wR`, `hR`) are in the
/// shape box; `A` angles are `stAng`, `swAng` in 60000ths of a degree.
enum ArcSeg {
    M(i128, i128),
    L(i128, i128),
    A(i128, i128, i64, i64),
    Z,
}

/// Flowchart presets built from `a:arcTo` (§20.1.9.3) and lines: one path per `<path>` of the
/// definition, or `None` for another preset. `w`, `h` are positive.
fn arc_flowchart_paths(preset: &str, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    use ArcSeg::{A, L, M, Z};
    let (wi, hi) = (i128::from(w), i128::from(h));
    let (hc, vc) = (wi / 2, hi / 2);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let (cd4, cd2, cd34) = (5_400_000, 10_800_000, 16_200_000);
    // A point or radius in a `pw` × `ph` path box, scaled to the shape box.
    let sx = |x: i128, pw: i128| md(wi, x, pw);
    let sy = |y: i128, ph: i128| md(hi, y, ph);
    // `cos wd2 2700000`, `sin hd2 2700000`: the 45° point of the inscribed ellipse.
    let idx = (hc as f64 * std::f64::consts::FRAC_PI_4.cos()) as i128;
    let idy = (vc as f64 * std::f64::consts::FRAC_PI_4.sin()) as i128;
    let ellipse = || vec![M(0, vc), A(hc, vc, cd2, cd4), A(hc, vc, cd34, cd4), A(hc, vc, 0, cd4), A(hc, vc, cd4, cd4), Z];
    // (fill, stroke, segments) per path.
    let paths: Vec<(bool, bool, Vec<ArcSeg>)> = match preset {
        "flowChartMagneticDisk" => {
            let (y1, y5, rx, ry) = (sy(1, 6), sy(5, 6), sx(3, 6), sy(1, 6));
            let outline = || vec![M(0, y1), A(rx, ry, cd2, cd2), L(wi, y5), A(rx, ry, 0, cd2), Z];
            vec![(true, false, outline()), (false, true, vec![M(wi, y1), A(rx, ry, 0, cd2)]), (false, true, outline())]
        }
        "flowChartMagneticDrum" => {
            let (x1, x5, rx, ry) = (sx(1, 6), sx(5, 6), sx(1, 6), sy(3, 6));
            let outline = || vec![M(x1, 0), L(x5, 0), A(rx, ry, cd34, cd2), L(x1, hi), A(rx, ry, cd4, cd2), Z];
            vec![(true, false, outline()), (false, true, vec![M(x5, hi), A(rx, ry, cd4, cd2)]), (false, true, outline())]
        }
        "flowChartMagneticTape" => {
            // `at2 w h`: the sweep from 0° back to the box's diagonal.
            let ang1 = ((h as f64).atan2(w as f64).to_degrees() * 60000.0) as i64;
            let ib = vc.saturating_add(idy);
            let segs = vec![M(hc, hi), A(hc, vc, cd4, cd4), A(hc, vc, cd2, cd4), A(hc, vc, cd34, cd4), A(hc, vc, 0, ang1), L(wi, ib), L(wi, hi), Z];
            vec![(true, true, segs)]
        }
        "flowChartDisplay" => {
            let segs = vec![M(0, sy(3, 6)), L(sx(1, 6), 0), L(sx(5, 6), 0), A(sx(1, 6), sy(3, 6), cd34, cd2), L(sx(1, 6), hi), Z];
            vec![(true, true, segs)]
        }
        "flowChartConnector" => vec![(true, true, ellipse())],
        "flowChartOr" => vec![(true, false, ellipse()), (false, true, vec![M(hc, 0), L(hc, hi), M(0, vc), L(wi, vc)]), (false, true, ellipse())],
        "flowChartSummingJunction" => {
            let (il, ir, it, ib) = (hc - idx, hc.saturating_add(idx), vc - idy, vc.saturating_add(idy));
            vec![(true, false, ellipse()), (false, true, vec![M(il, it), L(ir, ib), M(ir, it), L(il, ib)]), (false, true, ellipse())]
        }
        "flowChartOnlineStorage" => {
            let (rx, ry) = (sx(1, 6), sy(3, 6));
            let segs = vec![M(sx(1, 6), 0), L(wi, 0), A(rx, ry, cd34, -cd2), L(sx(1, 6), hi), A(rx, ry, cd4, cd2), Z];
            vec![(true, true, segs)]
        }
        "flowChartPunchedTape" => {
            let (rx, ry) = (sx(5, 20), sy(2, 20));
            let segs = vec![M(0, sy(2, 20)), A(rx, ry, cd2, -cd2), A(rx, ry, cd2, cd2), L(wi, sy(18, 20)), A(rx, ry, 0, -cd2), A(rx, ry, 0, cd2), Z];
            vec![(true, true, segs)]
        }
        _ => return None,
    };
    // Every point and radius is pinned into the box.
    let x = |v: i128| i64::try_from(v.clamp(0, wi)).unwrap_or(w);
    let y = |v: i128| i64::try_from(v.clamp(0, hi)).unwrap_or(h);
    Some(
        paths
            .into_iter()
            .map(|(fill, stroke, segs)| {
                let commands = segs
                    .into_iter()
                    .map(|s| match s {
                        M(px, py) => PathCommand::MoveTo(x(px), y(py)),
                        L(px, py) => PathCommand::LineTo(x(px), y(py)),
                        A(w_r, h_r, st_ang, sw_ang) => PathCommand::ArcTo { w_r: x(w_r), h_r: y(h_r), st_ang, sw_ang },
                        Z => PathCommand::Close,
                    })
                    .collect();
                GeometryPath { w, h, fill, stroke, shade: PathShade::Norm, commands }
            })
            .collect(),
    )
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

/// A straight path segment in a multi-path preset: move, line, or close.
#[derive(Clone, Copy)]
enum Seg {
    M(i128, i128),
    L(i128, i128),
    Z,
}

/// One path of a fixed multi-path preset: path box `w`, `h`, fill, stroke, then the segments.
type FixedPath = (i128, i128, bool, bool, &'static [Seg]);

/// Straight-edged presets drawn with several paths, as `(fill, stroke, segments)` per path in the
/// definition's order, in the `w` × `h` shape box. Vertices may need clamping into the box.
fn straight_multipath(preset: &str, w: i128, h: i128) -> Option<Vec<(bool, bool, Vec<Seg>)>> {
    use Seg::{L, M, Z};
    const DIAMOND: &[Seg] = &[M(0, 1), L(1, 0), L(2, 1), L(1, 2), Z];
    const FUNNEL: &[Seg] = &[M(0, 0), L(2, 0), L(1, 2), Z];
    const MARKER_BOX: FixedPath = (10, 10, true, false, &[M(0, 0), L(0, 10), L(10, 10), L(10, 0), Z]);
    let fixed: &[FixedPath] = match preset {
        "flowChartSort" => &[(2, 2, true, false, DIAMOND), (2, 2, false, true, &[M(0, 1), L(2, 1)]), (2, 2, false, true, DIAMOND)],
        "flowChartCollate" => &[(2, 2, true, true, &[M(0, 0), L(2, 0), L(1, 1), L(2, 2), L(0, 2), L(1, 1), Z])],
        "flowChartOfflineStorage" => &[(2, 2, true, false, FUNNEL), (5, 5, false, true, &[M(2, 4), L(3, 4)]), (2, 2, false, true, FUNNEL)],
        "chartPlus" => &[(10, 10, false, true, &[M(5, 0), L(5, 10), M(0, 5), L(10, 5)]), MARKER_BOX],
        "chartX" => &[(10, 10, false, true, &[M(0, 0), L(10, 10), M(0, 10), L(10, 0)]), MARKER_BOX],
        "chartStar" => &[(10, 10, false, true, &[M(0, 0), L(10, 10), M(0, 10), L(10, 0), M(5, 0), L(5, 10)]), MARKER_BOX],
        "squareTabs" | "cornerTabs" => {
            // `dx` is a twentieth of the box diagonal (`mod w h 0`), truncated.
            let dx = ((w as f64).hypot(h as f64) / 20.0) as i128;
            let (x1, y1) = (w - dx, h - dx);
            let tabs = if preset == "squareTabs" {
                [
                    vec![M(0, 0), L(dx, 0), L(dx, dx), L(0, dx), Z],
                    vec![M(0, y1), L(dx, y1), L(dx, h), L(0, h), Z],
                    vec![M(x1, 0), L(w, 0), L(w, dx), L(x1, dx), Z],
                    vec![M(x1, y1), L(w, y1), L(w, h), L(x1, h), Z],
                ]
            } else {
                [
                    vec![M(0, 0), L(dx, 0), L(0, dx), Z],
                    vec![M(0, y1), L(dx, h), L(0, h), Z],
                    vec![M(x1, 0), L(w, 0), L(w, dx), Z],
                    vec![M(w, y1), L(w, h), L(x1, h), Z],
                ]
            };
            return Some(tabs.into_iter().map(|segs| (true, true, segs)).collect());
        }
        _ => return None,
    };
    let scale = |v: i128, size: i128, path: i128| v.saturating_mul(size).checked_div(path).unwrap_or(0);
    Some(
        fixed
            .iter()
            .map(|&(pw, ph, fill, stroke, segs)| {
                let segs = segs
                    .iter()
                    .map(|&s| match s {
                        M(x, y) => M(scale(x, w, pw), scale(y, h, ph)),
                        L(x, y) => L(scale(x, w, pw), scale(y, h, ph)),
                        Z => Z,
                    })
                    .collect();
                (fill, stroke, segs)
            })
            .collect(),
    )
}

/// Brackets and braces (`leftBracket`, `rightBracket`, `leftBrace`, `rightBrace`, `bracketPair`,
/// `bracePair`): a fill-only path and an open stroke-only outline of arcs and lines, or `None` for
/// another preset. `w` and `h` are positive.
fn bracket_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    let (wi, hi) = (i128::from(w), i128::from(h));
    let ss = wi.min(hi);
    let (l, t, r, b, hc, vc, wd2) = (0, 0, wi, hi, wi / 2, hi / 2, wi / 2);
    let (cd4, cd2, cd34) = (5_400_000, 10_800_000, 16_200_000);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let pin = |lo: i128, x: i128, hi: i128| x.min(hi).max(lo);
    let px = |v: i128| i64::try_from(v.clamp(0, wi)).unwrap_or(w);
    let py = |v: i128| i64::try_from(v.clamp(0, hi)).unwrap_or(h);
    let m = |x: i128, y: i128| PathCommand::MoveTo(px(x), py(y));
    let ln = |x: i128, y: i128| PathCommand::LineTo(px(x), py(y));
    let arc = |w_r: i128, h_r: i128, st_ang: i64, sw_ang: i64| PathCommand::ArcTo { w_r: px(w_r), h_r: py(h_r), st_ang, sw_ang };
    // The outline is the filled path without its closing segment, except where the pair presets
    // split it into two open sub-paths.
    let paths = |fill: Vec<PathCommand>, outline: Vec<PathCommand>| {
        let path = |fill: bool, stroke: bool, commands: Vec<PathCommand>| GeometryPath { w, h, fill, stroke, shade: PathShade::Norm, commands };
        vec![path(true, false, fill), path(false, true, outline)]
    };
    let with_close = |mut c: Vec<PathCommand>| {
        c.push(PathCommand::Close);
        c
    };
    Some(match g.preset.as_str() {
        "leftBracket" | "rightBracket" => {
            let max_adj = md(50000, hi, ss);
            let a = pin(0, adjust(g, "adj", 8333), max_adj);
            let y1 = md(ss, a, 100000);
            let y2 = b - y1;
            let c = if g.preset == "leftBracket" {
                vec![m(r, b), arc(wi, y1, cd4, cd4), ln(l, y1), arc(wi, y1, cd2, cd4)]
            } else {
                vec![m(l, t), arc(wi, y1, cd34, cd4), ln(r, y2), arc(wi, y1, 0, cd4)]
            };
            paths(with_close(c.clone()), c)
        }
        "leftBrace" | "rightBrace" => {
            let a2 = pin(0, adjust(g, "adj2", 50000), 100000);
            let q1 = 100000 - a2;
            let q2 = q1.min(a2);
            let q3 = md(q2, 1, 2);
            let max_adj1 = md(q3, hi, ss);
            let a1 = pin(0, adjust(g, "adj1", 8333), max_adj1);
            let y1 = md(ss, a1, 100000);
            let y3 = md(hi, a2, 100000);
            let c = if g.preset == "leftBrace" {
                let y4 = y3 + y1;
                vec![m(r, b), arc(wd2, y1, cd4, cd4), ln(hc, y4), arc(wd2, y1, 0, -cd4), arc(wd2, y1, cd4, -cd4), ln(hc, y1), arc(wd2, y1, cd2, cd4)]
            } else {
                let (y2, y4) = (y3 - y1, b - y1);
                vec![
                    m(l, t),
                    arc(wd2, y1, cd34, cd4),
                    ln(hc, y2),
                    arc(wd2, y1, cd2, -cd4),
                    arc(wd2, y1, cd34, -cd4),
                    ln(hc, y4),
                    arc(wd2, y1, 0, cd4),
                ]
            };
            paths(with_close(c.clone()), c)
        }
        "bracketPair" => {
            let a = pin(0, adjust(g, "adj", 16667), 50000);
            let x1 = md(ss, a, 100000);
            let (x2, y2) = (r - x1, b - x1);
            let fill = vec![
                m(l, x1),
                arc(x1, x1, cd2, cd4),
                ln(x2, t),
                arc(x1, x1, cd34, cd4),
                ln(r, y2),
                arc(x1, x1, 0, cd4),
                ln(x1, b),
                arc(x1, x1, cd4, cd4),
                PathCommand::Close,
            ];
            let outline = vec![
                m(x1, b),
                arc(x1, x1, cd4, cd4),
                ln(l, x1),
                arc(x1, x1, cd2, cd4),
                m(x2, t),
                arc(x1, x1, cd34, cd4),
                ln(r, y2),
                arc(x1, x1, 0, cd4),
            ];
            paths(fill, outline)
        }
        "bracePair" => {
            let a = pin(0, adjust(g, "adj", 8333), 25000);
            let x1 = md(ss, a, 100000);
            let x2 = md(ss, a, 50000);
            let (x3, x4) = (r - x2, r - x1);
            let (y2, y3, y4) = (vc - x1, vc + x1, b - x1);
            let left =
                vec![m(x2, b), arc(x1, x1, cd4, cd4), ln(x1, y3), arc(x1, x1, 0, -cd4), arc(x1, x1, cd4, -cd4), ln(x1, x1), arc(x1, x1, cd2, cd4)];
            let right = [arc(x1, x1, cd34, cd4), ln(x4, y2), arc(x1, x1, cd2, -cd4), arc(x1, x1, cd34, -cd4), ln(x4, y4), arc(x1, x1, 0, cd4)];
            let mut fill = left.clone();
            fill.push(ln(x3, t));
            fill.extend(right.iter().cloned());
            fill.push(PathCommand::Close);
            let mut outline = left;
            outline.push(m(x3, t));
            outline.extend(right);
            paths(fill, outline)
        }
        _ => return None,
    })
}

/// The arc-based basic shapes (`pie`, `chord`, `arc`, `blockArc`, `plaque`): one path per `<path>`
/// of the definition, `a:arcTo` kept as [`PathCommand::ArcTo`], points and radii pinned into the
/// box. `None` for another preset or an empty box.
fn arc_shape_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let (wi, hi) = (i128::from(w), i128::from(h));
    let ss = wi.min(hi);
    let (l, t, r, b, hc, vc) = (0, 0, wi, hi, wi / 2, hi / 2);
    let (wd2, hd2) = (hc, vc);
    let (cd4, cd2, cd34, c360) = (5_400_000, 10_800_000, 16_200_000, 21_600_000);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    // `sin r a`, `cos r a` with `a` in 60000ths of a degree, and `cat2 x y z`, `sat2 x y z`.
    let rad = |a: i128| (a as f64 / 60000.0).to_radians();
    let sin = |r: i128, a: i128| (r as f64 * rad(a).sin()) as i128;
    let cos = |r: i128, a: i128| (r as f64 * rad(a).cos()) as i128;
    let cat2 = |x: i128, y: i128, z: i128| (x as f64 * (z as f64).atan2(y as f64).cos()) as i128;
    let sat2 = |x: i128, y: i128, z: i128| (x as f64 * (z as f64).atan2(y as f64).sin()) as i128;
    // The point at angle `ang` on the ellipse of radii `rx`, `ry` round the centre.
    let on_ellipse = |rx: i128, ry: i128, ang: i128| {
        let (wt, ht) = (sin(rx, ang), cos(ry, ang));
        (hc.saturating_add(cat2(rx, ht, wt)), vc.saturating_add(sat2(ry, ht, wt)))
    };
    // `swAng = ?: sw1 sw1 sw2`: the sweep from `st` to `en`, positive.
    let sweep = |st: i128, en: i128| {
        let sw1 = en - st;
        if sw1 > 0 { sw1 } else { sw1 + c360 }
    };
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { 0 } else { i64::MAX });
    let m = |x: i128, y: i128| PathCommand::MoveTo(to_i64(x.clamp(0, wi)), to_i64(y.clamp(0, hi)));
    let ln = |x: i128, y: i128| PathCommand::LineTo(to_i64(x.clamp(0, wi)), to_i64(y.clamp(0, hi)));
    let arc = |w_r: i128, h_r: i128, st_ang: i128, sw_ang: i128| PathCommand::ArcTo {
        w_r: to_i64(w_r.clamp(0, wi)),
        h_r: to_i64(h_r.clamp(0, hi)),
        st_ang: i64::try_from(st_ang).unwrap_or(0),
        sw_ang: i64::try_from(sw_ang).unwrap_or(0),
    };
    let path = |fill: bool, stroke: bool, commands: Vec<PathCommand>| GeometryPath { w, h, fill, stroke, shade: PathShade::Norm, commands };
    let close = PathCommand::Close;
    Some(match g.preset.as_str() {
        "pie" | "chord" | "arc" => {
            let (d1, d2) = match g.preset.as_str() {
                "pie" => (0, cd34),
                "chord" => (2_700_000, cd34),
                _ => (cd34, 0),
            };
            let st_ang = adjust(g, "adj1", d1).clamp(0, 21_599_999);
            let en_ang = adjust(g, "adj2", d2).clamp(0, 21_599_999);
            let sw_ang = sweep(st_ang, en_ang);
            let (x1, y1) = on_ellipse(wd2, hd2, st_ang);
            let outline = vec![m(x1, y1), arc(wd2, hd2, st_ang, sw_ang)];
            match g.preset.as_str() {
                "pie" => vec![path(true, true, [outline, vec![ln(hc, vc), close]].concat())],
                "chord" => vec![path(true, true, [outline, vec![close]].concat())],
                _ => vec![path(true, false, [outline.clone(), vec![ln(hc, vc), close]].concat()), path(false, true, outline)],
            }
        }
        "blockArc" => {
            let st_ang = adjust(g, "adj1", cd2).clamp(0, 21_599_999);
            let ist_ang = adjust(g, "adj2", 0).clamp(0, 21_599_999);
            let a3 = adjust(g, "adj3", 25000).clamp(0, 50000);
            let sw_ang = sweep(st_ang, ist_ang);
            let dr = md(ss, a3, 100000);
            let (iwd2, ihd2) = ((wd2 - dr).max(0), (hd2 - dr).max(0));
            let (x1, y1) = on_ellipse(wd2, hd2, st_ang);
            let (x2, y2) = on_ellipse(iwd2, ihd2, ist_ang);
            vec![path(true, true, vec![m(x1, y1), arc(wd2, hd2, st_ang, sw_ang), ln(x2, y2), arc(iwd2, ihd2, ist_ang, -sw_ang), close])]
        }
        "plaque" => {
            let a = adjust(g, "adj", 16667).clamp(0, 50000);
            let x1 = md(ss, a, 100000);
            let (x2, y2) = (r - x1, b - x1);
            vec![path(
                true,
                true,
                vec![
                    m(l, x1),
                    arc(x1, x1, cd4, -cd4),
                    ln(x2, t),
                    arc(x1, x1, cd2, -cd4),
                    ln(r, y2),
                    arc(x1, x1, cd34, -cd4),
                    ln(x1, b),
                    arc(x1, x1, 0, -cd4),
                    close,
                ],
            )]
        }
        _ => return None,
    })
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

/// `heptagon`: a seven-sided polygon, wider and taller than the box by its scale factors `hf`/`vf`.
fn heptagon_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let (hc, vc) = (wf / 2.0, hf / 2.0);
    let hf = adjust(g, "hf", 102572) as f64;
    let vf = adjust(g, "vf", 105210) as f64;
    let swd2 = hc * hf / 100000.0;
    let shd2 = vc * vf / 100000.0;
    let svc = vc * vf / 100000.0;
    let (dx1, dx2, dx3) = (swd2 * 97493.0 / 100000.0, swd2 * 78183.0 / 100000.0, swd2 * 43388.0 / 100000.0);
    let (dy1, dy2, dy3) = (shd2 * 62349.0 / 100000.0, shd2 * 22252.0 / 100000.0, shd2 * 90097.0 / 100000.0);
    let (x1, x2, x3) = (hc - dx1, hc - dx2, hc - dx3);
    let (x4, x5, x6) = (hc + dx3, hc + dx2, hc + dx1);
    let (y1, y2, y3) = (svc - dy1, svc + dy2, svc + dy3);
    round_points(&[(x1, y2), (x2, y1), (hc, 0.0), (x5, y1), (x6, y2), (x4, y3), (x3, y3)], w, h)
}

/// `decagon`: a ten-sided polygon, taller than the box by its scale factor `vf`.
fn decagon_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let (hc, vc) = (wf / 2.0, hf / 2.0);
    let vf = adjust(g, "vf", 105146) as f64;
    let shd2 = vc * vf / 100000.0;
    let (dx1, dx2) = (hc * 36_f64.to_radians().cos(), hc * 72_f64.to_radians().cos());
    let (dy1, dy2) = (shd2 * 72_f64.to_radians().sin(), shd2 * 36_f64.to_radians().sin());
    let (x1, x2, x3, x4) = (hc - dx1, hc - dx2, hc + dx2, hc + dx1);
    let (y1, y2, y3, y4) = (vc - dy1, vc - dy2, vc + dy2, vc + dy1);
    round_points(&[(0.0, vc), (x1, y2), (x2, y1), (x3, y1), (x4, y2), (wf, vc), (x4, y3), (x3, y4), (x2, y4), (x1, y3)], w, h)
}

/// `dodecagon`: a twelve-sided polygon with fixed proportions (21600ths of the box).
fn dodecagon_points(w: i128, h: i128) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let [x1, x2, x3, x4] = [2894.0, 7906.0, 13694.0, 18706.0].map(|n| wf * n / 21600.0);
    let [y1, y2, y3, y4] = [2894.0, 7906.0, 13694.0, 18706.0].map(|n| hf * n / 21600.0);
    round_points(&[(0.0, y2), (x1, y1), (x2, 0.0), (x3, 0.0), (x4, y1), (wf, y2), (wf, y3), (x4, y4), (x3, hf), (x2, hf), (x1, y4), (0.0, y3)], w, h)
}

/// `nonIsoscelesTrapezoid`: a trapezoid whose top edge is shifted by `adj1` and narrowed by `adj2`.
fn non_isosceles_trapezoid_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let ss = wf.min(hf);
    let max_adj = 50000.0 * wf / ss;
    let a1 = (adjust(g, "adj1", 25000) as f64).clamp(0.0, max_adj);
    let a2 = (adjust(g, "adj2", 25000) as f64).clamp(0.0, max_adj);
    let x2 = ss * a1 / 100000.0;
    let x3 = wf - ss * a2 / 100000.0;
    round_points(&[(0.0, hf), (x2, 0.0), (x3, 0.0), (wf, hf)], w, h)
}

/// `snip2SameRect`: a rectangle with two snipped corners, `adj1` on the top and `adj2` on the bottom.
fn snip_2_same_rect_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let ss = wf.min(hf);
    let a1 = (adjust(g, "adj1", 16667) as f64).clamp(0.0, 50000.0);
    let a2 = (adjust(g, "adj2", 0) as f64).clamp(0.0, 50000.0);
    let tx1 = ss * a1 / 100000.0;
    let bx1 = ss * a2 / 100000.0;
    round_points(&[(tx1, 0.0), (wf - tx1, 0.0), (wf, tx1), (wf, hf - bx1), (wf - bx1, hf), (bx1, hf), (0.0, hf - bx1), (0.0, tx1)], w, h)
}

/// `snip2DiagRect`: a rectangle with two snipped corners, `adj1` on the left and `adj2` on the right.
fn snip_2_diag_rect_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let ss = wf.min(hf);
    let a1 = (adjust(g, "adj1", 0) as f64).clamp(0.0, 50000.0);
    let a2 = (adjust(g, "adj2", 16667) as f64).clamp(0.0, 50000.0);
    let lx1 = ss * a1 / 100000.0;
    let rx1 = ss * a2 / 100000.0;
    round_points(&[(lx1, 0.0), (wf - rx1, 0.0), (wf, rx1), (wf, hf - lx1), (wf - lx1, hf), (rx1, hf), (0.0, hf - rx1), (0.0, lx1)], w, h)
}

/// `halfFrame`: a rectangle with its lower-left corner cut back to a point by `adj1` and `adj2`.
fn half_frame_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let ss = wf.min(hf);
    let max_adj2 = 100000.0 * wf / ss;
    let a2 = (adjust(g, "adj2", 33333) as f64).clamp(0.0, max_adj2);
    let x1 = ss * a2 / 100000.0;
    let g1 = hf * x1 / wf;
    let max_adj1 = 100000.0 * (hf - g1).max(0.0) / ss;
    let a1 = (adjust(g, "adj1", 33333) as f64).clamp(0.0, max_adj1);
    let y1 = ss * a1 / 100000.0;
    let x2 = wf - y1 * wf / hf;
    let y2 = hf - x1 * hf / wf;
    round_points(&[(0.0, 0.0), (wf, 0.0), (x2, y1), (x1, y1), (x1, y2), (0.0, hf)], w, h)
}

/// `corner`: a rectangle with its upper-left corner folded in by `adj1` and `adj2`.
fn corner_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let ss = wf.min(hf);
    let a1 = (adjust(g, "adj1", 50000) as f64).clamp(0.0, 100000.0 * hf / ss);
    let a2 = (adjust(g, "adj2", 50000) as f64).clamp(0.0, 100000.0 * wf / ss);
    let x1 = ss * a2 / 100000.0;
    let y1 = hf - ss * a1 / 100000.0;
    round_points(&[(0.0, 0.0), (x1, 0.0), (x1, y1), (wf, y1), (wf, hf), (0.0, hf)], w, h)
}

/// `diagStripe`: a diagonal band from the left edge to the top edge, widened by `adj`.
fn diag_stripe_points(w: i128, h: i128, g: &ShapeGeometry) -> Vec<(i128, i128)> {
    let (wf, hf) = (w as f64, h as f64);
    let a = (adjust(g, "adj", 50000) as f64).clamp(0.0, 100000.0);
    let x2 = wf * a / 100000.0;
    let y2 = hf * a / 100000.0;
    round_points(&[(0.0, y2), (x2, 0.0), (wf, 0.0), (0.0, hf)], w, h)
}

/// `lightningBolt`: a fixed eleven-point bolt in a 21600 × 21600 path box, scaled to the shape box.
fn lightning_bolt_points(w: i128, h: i128) -> Vec<(i128, i128)> {
    const BOLT: [(i128, i128); 11] = [
        (8472, 0),
        (12860, 6080),
        (11050, 6797),
        (16577, 12007),
        (14767, 12877),
        (21600, 21600),
        (10012, 14915),
        (12222, 13987),
        (5022, 9705),
        (7602, 8382),
        (0, 3890),
    ];
    BOLT.iter().map(|&(x, y)| (w.saturating_mul(x) / 21600, h.saturating_mul(y) / 21600)).collect()
}

/// Rounds the vertices of a guide-computed outline and pins them into the shape box: scale factors
/// can push a vertex outside it, and the conversion of a hostile float is clamped.
fn round_points(pts: &[(f64, f64)], w: i128, h: i128) -> Vec<(i128, i128)> {
    let to_i = |v: f64, max: i128| {
        let n = if v.is_nan() { 0.0 } else { v.round() };
        if n <= 0.0 {
            0
        } else if n >= max as f64 {
            max
        } else {
            n as i128
        }
    };
    pts.iter().map(|&(x, y)| (to_i(x, w), to_i(y, h))).collect()
}

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

/// Block arrows whose guides are evaluated in `f64`: one path of closed sub-paths, every vertex
/// rounded and pinned into the `w` × `h` box.
fn block_arrow_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if !g.paths.is_empty() || w <= 0 || h <= 0 {
        return None;
    }
    let (wf, hf) = (w as f64, h as f64);
    let b = ArrowBox { w: wf, h: hf, ss: wf.min(hf), hc: wf / 2.0, vc: hf / 2.0 };
    let adj = |name: &str, default: f64| adjust(g, name, default as i128) as f64;
    let polygons = match g.preset.as_str() {
        "upDownArrow" => up_down_arrow(&b, adj("adj1", 50000.0), adj("adj2", 50000.0)),
        "quadArrow" => quad_arrow(&b, adj("adj1", 22500.0), adj("adj2", 22500.0), adj("adj3", 22500.0)),
        "leftRightUpArrow" => left_right_up_arrow(&b, adj("adj1", 25000.0), adj("adj2", 25000.0), adj("adj3", 25000.0)),
        "leftUpArrow" => left_up_arrow(&b, adj("adj1", 25000.0), adj("adj2", 25000.0), adj("adj3", 25000.0)),
        "bentUpArrow" => bent_up_arrow(&b, adj("adj1", 25000.0), adj("adj2", 25000.0), adj("adj3", 25000.0)),
        "stripedRightArrow" => striped_right_arrow(&b, adj("adj1", 50000.0), adj("adj2", 50000.0)),
        _ => return None,
    };
    // Round, then pin into the box: scale factors and float error can land a unit outside.
    let fix = |v: f64, max: i64| if v.is_finite() { (v.round() as i64).clamp(0, max) } else { 0 };
    let mut commands = Vec::new();
    for polygon in &polygons {
        for (i, &(x, y)) in polygon.iter().enumerate() {
            let (x, y) = (fix(x, w), fix(y, h));
            commands.push(if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) });
        }
        commands.push(PathCommand::Close);
    }
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// The shape box for the `f64` block-arrow guides (`l` = `t` = 0).
struct ArrowBox {
    w: f64,
    h: f64,
    ss: f64,
    hc: f64,
    vc: f64,
}

/// DrawingML `pin lo v hi`: `v` held in `lo..=hi` (`lo` wins when the range is empty).
fn pin(lo: f64, v: f64, hi: f64) -> f64 {
    v.min(hi).max(lo)
}

/// `x * y / z`, or 0 when `z` is zero.
fn muldiv(x: f64, y: f64, z: f64) -> f64 {
    if z == 0.0 { 0.0 } else { x * y / z }
}

fn up_down_arrow(b: &ArrowBox, adj1: f64, adj2: f64) -> Vec<Vec<(f64, f64)>> {
    let (r, bt, hc) = (b.w, b.h, b.hc);
    let max_adj2 = muldiv(50000.0, b.h, b.ss);
    let a1 = pin(0.0, adj1, 100000.0);
    let a2 = pin(0.0, adj2, max_adj2);
    let y2 = b.ss * a2 / 100000.0;
    let y3 = bt - y2;
    let dx1 = b.w * a1 / 200000.0;
    let (x1, x2) = (hc - dx1, hc + dx1);
    vec![vec![(0.0, y2), (hc, 0.0), (r, y2), (x2, y2), (x2, y3), (r, y3), (hc, bt), (0.0, y3), (x1, y3), (x1, y2)]]
}

fn quad_arrow(b: &ArrowBox, adj1: f64, adj2: f64, adj3: f64) -> Vec<Vec<(f64, f64)>> {
    let (r, bt, hc, vc, ss) = (b.w, b.h, b.hc, b.vc, b.ss);
    let a2 = pin(0.0, adj2, 50000.0);
    let max_adj1 = a2 * 2.0;
    let a1 = pin(0.0, adj1, max_adj1);
    let max_adj3 = (100000.0 - max_adj1) / 2.0;
    let a3 = pin(0.0, adj3, max_adj3);
    let x1 = ss * a3 / 100000.0;
    let dx2 = ss * a2 / 100000.0;
    let (x2, x5) = (hc - dx2, hc + dx2);
    let dx3 = ss * a1 / 200000.0;
    let (x3, x4) = (hc - dx3, hc + dx3);
    let x6 = r - x1;
    let (y2, y5, y3, y4) = (vc - dx2, vc + dx2, vc - dx3, vc + dx3);
    let y6 = bt - x1;
    vec![vec![
        (0.0, vc),
        (x1, y2),
        (x1, y3),
        (x3, y3),
        (x3, x1),
        (x2, x1),
        (hc, 0.0),
        (x5, x1),
        (x4, x1),
        (x4, y3),
        (x6, y3),
        (x6, y2),
        (r, vc),
        (x6, y5),
        (x6, y4),
        (x4, y4),
        (x4, y6),
        (x5, y6),
        (hc, bt),
        (x2, y6),
        (x3, y6),
        (x3, y4),
        (x1, y4),
        (x1, y5),
    ]]
}

fn left_right_up_arrow(b: &ArrowBox, adj1: f64, adj2: f64, adj3: f64) -> Vec<Vec<(f64, f64)>> {
    let (r, bt, hc, ss) = (b.w, b.h, b.hc, b.ss);
    let a2 = pin(0.0, adj2, 50000.0);
    let max_adj1 = a2 * 2.0;
    let a1 = pin(0.0, adj1, max_adj1);
    let max_adj3 = (100000.0 - max_adj1) / 2.0;
    let a3 = pin(0.0, adj3, max_adj3);
    let x1 = ss * a3 / 100000.0;
    let dx2 = ss * a2 / 100000.0;
    let (x2, x5) = (hc - dx2, hc + dx2);
    let dx3 = ss * a1 / 200000.0;
    let (x3, x4) = (hc - dx3, hc + dx3);
    let x6 = r - x1;
    let y2 = bt - ss * a2 / 50000.0;
    let y4 = bt - dx2;
    let (y3, y5) = (y4 - dx3, y4 + dx3);
    vec![vec![
        (0.0, y4),
        (x1, y2),
        (x1, y3),
        (x3, y3),
        (x3, x1),
        (x2, x1),
        (hc, 0.0),
        (x5, x1),
        (x4, x1),
        (x4, y3),
        (x6, y3),
        (x6, y2),
        (r, y4),
        (x6, bt),
        (x6, y5),
        (x1, y5),
        (x1, bt),
    ]]
}

fn left_up_arrow(b: &ArrowBox, adj1: f64, adj2: f64, adj3: f64) -> Vec<Vec<(f64, f64)>> {
    let (r, bt, ss) = (b.w, b.h, b.ss);
    let a2 = pin(0.0, adj2, 50000.0);
    let max_adj1 = a2 * 2.0;
    let a1 = pin(0.0, adj1, max_adj1);
    let a3 = pin(0.0, adj3, 100000.0 - max_adj1);
    let x1 = ss * a3 / 100000.0;
    let dx2 = ss * a2 / 50000.0;
    let (x2, y2) = (r - dx2, bt - dx2);
    let dx4 = ss * a2 / 100000.0;
    let (x4, y4) = (r - dx4, bt - dx4);
    let dx3 = ss * a1 / 200000.0;
    let (x3, x5) = (x4 - dx3, x4 + dx3);
    let (y3, y5) = (y4 - dx3, y4 + dx3);
    vec![vec![(0.0, y4), (x1, y2), (x1, y3), (x3, y3), (x3, x1), (x2, x1), (x4, 0.0), (r, x1), (x5, x1), (x5, y5), (x1, y5), (x1, bt)]]
}

fn bent_up_arrow(b: &ArrowBox, adj1: f64, adj2: f64, adj3: f64) -> Vec<Vec<(f64, f64)>> {
    let (r, bt, ss) = (b.w, b.h, b.ss);
    let a1 = pin(0.0, adj1, 50000.0);
    let a2 = pin(0.0, adj2, 50000.0);
    let a3 = pin(0.0, adj3, 50000.0);
    let y1 = ss * a3 / 100000.0;
    let x1 = r - ss * a2 / 50000.0;
    let x3 = r - ss * a2 / 100000.0;
    let dx2 = ss * a1 / 200000.0;
    let (x2, x4) = (x3 - dx2, x3 + dx2);
    let y2 = bt - ss * a1 / 100000.0;
    vec![vec![(0.0, y2), (x2, y2), (x2, y1), (x1, y1), (x3, 0.0), (r, y1), (x4, y1), (x4, bt), (0.0, bt)]]
}

fn striped_right_arrow(b: &ArrowBox, adj1: f64, adj2: f64) -> Vec<Vec<(f64, f64)>> {
    let (r, bt, vc, ss) = (b.w, b.h, b.vc, b.ss);
    let max_adj2 = muldiv(84375.0, b.w, ss);
    let a1 = pin(0.0, adj1, 100000.0);
    let a2 = pin(0.0, adj2, max_adj2);
    let x4 = ss * 5.0 / 32.0;
    let x5 = r - ss * a2 / 100000.0;
    let dy1 = b.h * a1 / 200000.0;
    let (y1, y2) = (vc - dy1, vc + dy1);
    let (ssd32, ssd16, ssd8) = (ss / 32.0, ss / 16.0, ss / 8.0);
    vec![
        vec![(0.0, y1), (ssd32, y1), (ssd32, y2), (0.0, y2)],
        vec![(ssd16, y1), (ssd8, y1), (ssd8, y2), (ssd16, y2)],
        vec![(x4, y1), (x5, y1), (x5, 0.0), (r, vc), (x5, bt), (x5, y2), (x4, y2)],
    ]
}
