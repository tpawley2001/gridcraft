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
    if let Some(paths) = ribbon_or_scroll_paths(g, w, h) {
        return Some(paths);
    }
    if let Some(paths) = tabs_funnel_gears(g, w, h) {
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
    if let Some(paths) = shaded_paths(g, wi, hi) {
        return Some(paths);
    }
    if let Some(commands) = rounded_rect_paths(g, wi, hi) {
        return Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }]);
    }
    if let Some(paths) = symbol_paths(g, w, h) {
        return Some(paths);
    }
    if let Some(paths) = wedge_callout(g, w, h) {
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
        "heart" | "wave" | "doubleWave" => return curved_paths(g, w, h),
        "leftArrowCallout"
        | "rightArrowCallout"
        | "upArrowCallout"
        | "downArrowCallout"
        | "leftRightArrowCallout"
        | "upDownArrowCallout"
        | "quadArrowCallout" => return arrow_callout(g, w, h),
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
        "bentArrow" | "uturnArrow" | "circularArrow" | "leftCircularArrow" | "leftRightCircularArrow" => return arc_arrow_paths(g, w, h),
        "smileyFace"
        | "actionButtonBlank"
        | "actionButtonBackPrevious"
        | "actionButtonForwardNext"
        | "actionButtonBeginning"
        | "actionButtonEnd"
        | "actionButtonHome" => return button_paths(g, w, h),
        "actionButtonHelp"
        | "actionButtonInformation"
        | "actionButtonReturn"
        | "actionButtonDocument"
        | "actionButtonSound"
        | "actionButtonMovie" => {
            return action_button_glyph_paths(&g.preset, w, h);
        }
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

/// Heart, wave and double wave: one filled and stroked path of cubic Béziers and lines, or `None`
/// for another preset. On-curve points are clamped into the box; control points are kept as the
/// definition computes them, even outside it (the heart's lobes).
fn curved_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    let (wi, hi) = (i128::from(w), i128::from(h));
    let (l, t, r, b, hc) = (0, 0, wi, hi, wi / 2);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let pin = |name: &str, default: i128, lo: i128, hi: i128| adjust(g, name, default).clamp(lo, hi);
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX });
    let on = |x: i128, y: i128| (to_i64(x.clamp(l, r)), to_i64(y.clamp(t, b)));
    let m = |x: i128, y: i128| {
        let (x, y) = on(x, y);
        PathCommand::MoveTo(x, y)
    };
    let ln = |x: i128, y: i128| {
        let (x, y) = on(x, y);
        PathCommand::LineTo(x, y)
    };
    let bez = |c1: (i128, i128), c2: (i128, i128), e: (i128, i128)| {
        PathCommand::CubicBezTo([(to_i64(c1.0), to_i64(c1.1)), (to_i64(c2.0), to_i64(c2.1)), on(e.0, e.1)])
    };
    let commands = match g.preset.as_str() {
        "heart" => {
            let (dx1, dx2) = (md(wi, 49, 48), md(wi, 10, 48));
            let (x1, x2, x3, x4) = (hc - dx1, hc - dx2, hc + dx2, hc + dx1);
            let hd4 = hi / 4;
            let y1 = t - hi / 3;
            vec![m(hc, hd4), bez((x3, y1), (x4, hd4), (hc, b)), bez((x1, hd4), (x2, y1), (hc, hd4)), PathCommand::Close]
        }
        "wave" | "doubleWave" => {
            let double = g.preset == "doubleWave";
            let a1 = if double { pin("adj1", 6250, 0, 12500) } else { pin("adj1", 12500, 0, 20000) };
            let a2 = pin("adj2", 0, -10000, 10000);
            let y1 = md(hi, a1, 100000);
            // `*/ y1 10 3` from `h` directly, so `y1`'s truncation is not tripled.
            let dy2 = md(hi, a1.saturating_mul(10), 300000);
            let (y2, y3) = (y1 - dy2, y1 + dy2);
            let y4 = b - y1;
            let (y5, y6) = (y4 - dy2, y4 + dy2);
            let of2 = md(wi, a2, 50000);
            let dx2 = if of2 > 0 { 0 } else { of2 };
            let dxr = if of2 > 0 { of2 } else { 0 };
            let x2 = l - dx2;
            let xr = r - dxr;
            let xl = l + dxr;
            let xr2 = r + dx2;
            let avg = |x: i128, y: i128, z: i128| x.saturating_add(y) / z;
            if double {
                let (x8, x9, x15) = (xr, xl, xr2);
                let (dx3, dx4) = (avg(dx2, x8, 6), avg(dx2, x8, 3));
                let (x3, x4, x5) = (x2 + dx3, x2 + dx4, avg(x2, x8, 2));
                let x6 = x5 + dx3;
                let x7 = avg(x6, x8, 2);
                let (x10, x11, x12) = (x9 + dx3, x9 + dx4, avg(x9, x15, 2));
                let x13 = x12 + dx3;
                let x14 = avg(x13, x15, 2);
                vec![
                    m(x2, y1),
                    bez((x3, y2), (x4, y3), (x5, y1)),
                    bez((x6, y2), (x7, y3), (x8, y1)),
                    ln(x15, y4),
                    bez((x14, y6), (x13, y5), (x12, y4)),
                    bez((x11, y6), (x10, y5), (x9, y4)),
                    PathCommand::Close,
                ]
            } else {
                let (x5, x6, x10) = (xr, xl, xr2);
                let dx3 = avg(dx2, x5, 3);
                let x3 = x2 + dx3;
                let x4 = avg(x3, x5, 2);
                let x7 = x6 + dx3;
                let x8 = avg(x7, x10, 2);
                vec![m(x2, y1), bez((x3, y2), (x4, y3), (x5, y1)), ln(x10, y4), bez((x8, y6), (x7, y5), (x6, y4)), PathCommand::Close]
            }
        }
        _ => return None,
    };
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// Can, folded corner, bevel, sun and cloud: several paths each, some filled with the shape's fill
/// shaded (`fill="lighten"` …), or `None` for another preset. Points and radii are clamped into the
/// box.
fn shaded_paths(g: &ShapeGeometry, wi: i128, hi: i128) -> Option<Vec<GeometryPath>> {
    let (w, h) = (i64::try_from(wi).ok()?, i64::try_from(hi).ok()?);
    let ss = wi.min(hi);
    let (l, t, r, b, hc, vc) = (0, 0, wi, hi, wi / 2, hi / 2);
    let cd2 = 10_800_000;
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { 0 } else { i64::MAX });
    let m = |x: i128, y: i128| PathCommand::MoveTo(to_i64(x.clamp(0, wi)), to_i64(y.clamp(0, hi)));
    let ln = |x: i128, y: i128| PathCommand::LineTo(to_i64(x.clamp(0, wi)), to_i64(y.clamp(0, hi)));
    let arc = |w_r: i128, h_r: i128, st_ang: i64, sw_ang: i64| PathCommand::ArcTo {
        w_r: to_i64(w_r.clamp(0, wi)),
        h_r: to_i64(h_r.clamp(0, hi)),
        st_ang,
        sw_ang,
    };
    let path = |fill: bool, stroke: bool, shade: PathShade, commands: Vec<PathCommand>| GeometryPath { w, h, fill, stroke, shade, commands };
    let close = || PathCommand::Close;
    let pin = |lo: i128, v: i128, up: i128| v.max(lo).min(up.max(lo));
    Some(match g.preset.as_str() {
        "can" => {
            let max_adj = md(50000, hi, ss);
            let a = pin(0, adjust(g, "adj", 25000), max_adj);
            let y1 = md(ss, a, 200000);
            let y3 = b - y1;
            let wd2 = hc;
            vec![
                path(true, false, PathShade::Norm, vec![m(l, y1), arc(wd2, y1, cd2, -cd2), ln(r, y3), arc(wd2, y1, 0, cd2), close()]),
                path(true, false, PathShade::Lighten, vec![m(l, y1), arc(wd2, y1, cd2, cd2), arc(wd2, y1, 0, cd2), close()]),
                path(
                    false,
                    true,
                    PathShade::Norm,
                    vec![m(r, y1), arc(wd2, y1, 0, cd2), arc(wd2, y1, cd2, cd2), ln(r, y3), arc(wd2, y1, 0, cd2), ln(l, y1)],
                ),
            ]
        }
        "foldedCorner" => {
            let a = pin(0, adjust(g, "adj", 16667), 50000);
            let dy2 = md(ss, a, 100000);
            let dy1 = md(dy2, 1, 5);
            let (x1, y2) = (r - dy2, b - dy2);
            let (x2, y1) = (x1 + dy1, y2 + dy1);
            vec![
                path(true, false, PathShade::Norm, vec![m(l, t), ln(r, t), ln(r, y2), ln(x1, b), ln(l, b), close()]),
                path(true, false, PathShade::DarkenLess, vec![m(x1, b), ln(x2, y1), ln(r, y2), close()]),
                path(false, true, PathShade::Norm, vec![m(x1, b), ln(x2, y1), ln(r, y2), ln(x1, b), ln(l, b), ln(l, t), ln(r, t), ln(r, y2)]),
            ]
        }
        "bevel" => {
            let a = pin(0, adjust(g, "adj", 12500), 50000);
            let x1 = md(ss, a, 100000);
            let (x2, y2) = (r - x1, b - x1);
            vec![
                path(true, false, PathShade::Norm, vec![m(x1, x1), ln(x2, x1), ln(x2, y2), ln(x1, y2), close()]),
                path(true, false, PathShade::LightenLess, vec![m(l, t), ln(r, t), ln(x2, x1), ln(x1, x1), close()]),
                path(true, false, PathShade::DarkenLess, vec![m(l, b), ln(x1, y2), ln(x2, y2), ln(r, b), close()]),
                path(true, false, PathShade::Lighten, vec![m(l, t), ln(x1, x1), ln(x1, y2), ln(l, b), close()]),
                path(true, false, PathShade::Darken, vec![m(r, t), ln(r, b), ln(x2, y2), ln(x2, x1), close()]),
                path(
                    false,
                    true,
                    PathShade::Norm,
                    vec![
                        m(l, t),
                        ln(r, t),
                        ln(r, b),
                        ln(l, b),
                        close(),
                        m(x1, x1),
                        ln(x2, x1),
                        ln(x2, y2),
                        ln(x1, y2),
                        close(),
                        m(l, t),
                        ln(x1, x1),
                        m(l, b),
                        ln(x1, y2),
                        m(r, t),
                        ln(x2, x1),
                        m(r, b),
                        ln(x2, y2),
                    ],
                ),
            ]
        }
        "sun" => {
            let a = pin(12500, adjust(g, "adj", 25000), 46875);
            let g0 = 50000 - a;
            let (g1, g2) = (md(g0, 30274, 32768), md(g0, 12540, 32768));
            let (g5, g6) = (50000 - g1, 50000 - g2);
            let (g10, g11) = (md(g5, 3, 4), md(g6, 3, 4));
            let (g12, g13, g14) = (g10 + 3662, g11 + 3662, g11 + 12500);
            let (g15, g16, g17, g18) = (100000 - g10, 100000 - g12, 100000 - g13, 100000 - g14);
            let (ox1, oy1, ox2, oy2) = (md(wi, 18436, 21600), md(hi, 3163, 21600), md(wi, 3163, 21600), md(hi, 18436, 21600));
            let x = |f: i128| md(wi, f, 100000);
            let y = |f: i128| md(hi, f, 100000);
            let (x10, x12, x13, x14, x15, x16, x17, x18, x19) = (x(g10), x(g12), x(g13), x(g14), x(g15), x(g16), x(g17), x(g18), x(a));
            let (y10, y12, y13, y14, y15, y16, y17, y18) = (y(g10), y(g12), y(g13), y(g14), y(g15), y(g16), y(g17), y(g18));
            let (w_r, h_r) = (x(g0), y(g0));
            let rays = [
                [(r, vc), (x15, y18), (x15, y14)],
                [(ox1, oy1), (x16, y13), (x17, y12)],
                [(hc, t), (x18, y10), (x14, y10)],
                [(ox2, oy1), (x13, y12), (x12, y13)],
                [(l, vc), (x10, y14), (x10, y18)],
                [(ox2, oy2), (x12, y17), (x13, y16)],
                [(hc, b), (x14, y15), (x18, y15)],
                [(ox1, oy2), (x17, y16), (x16, y17)],
            ];
            let mut commands = Vec::with_capacity(rays.len() * 4 + 3);
            for [(x0, y0), (x1, y1), (x2, y2)] in rays {
                commands.extend([m(x0, y0), ln(x1, y1), ln(x2, y2), close()]);
            }
            commands.extend([m(x19, vc), arc(w_r, h_r, cd2, 21_600_000), close()]);
            vec![path(true, true, PathShade::Norm, commands)]
        }
        "cloud" => {
            // Both paths have a 43200 × 43200 box of their own.
            let sx = |v: i128| md(wi, v, 43200);
            let sy = |v: i128| md(hi, v, 43200);
            let mut outline = vec![m(sx(3900), sy(14370))];
            outline.extend(CLOUD_OUTLINE.iter().map(|&(w_r, h_r, st, sw)| arc(sx(w_r), sy(h_r), st, sw)));
            outline.push(close());
            let mut creases = Vec::with_capacity(CLOUD_CREASES.len() * 2);
            for &(x0, y0, w_r, h_r, st, sw) in &CLOUD_CREASES {
                creases.extend([m(sx(x0), sy(y0)), arc(sx(w_r), sy(h_r), st, sw)]);
            }
            vec![path(true, true, PathShade::Norm, outline), path(false, true, PathShade::Norm, creases)]
        }
        _ => return None,
    })
}

/// The cloud's outline in its 43200 × 43200 path box: `wR`, `hR`, `stAng`, `swAng` per arc, from
/// (3900, 14370).
const CLOUD_OUTLINE: [(i128, i128, i64, i64); 11] = [
    (6753, 9190, -11429249, 7426832),
    (5333, 7267, -8646143, 5396714),
    (4365, 5945, -8748475, 5983381),
    (4857, 6595, -7859164, 7034504),
    (5333, 7273, -4722533, 6541615),
    (6775, 9220, -2776035, 7816140),
    (5785, 7867, 37501, 6842000),
    (6752, 9215, 1347096, 6910353),
    (7720, 10543, 3974558, 4542661),
    (4360, 5918, -16496525, 8804134),
    (4345, 5945, -14809710, 9151131),
];

/// The cloud's unfilled inner creases: start point, then `wR`, `hR`, `stAng`, `swAng` of one arc.
const CLOUD_CREASES: [(i128, i128, i128, i128, i64, i64); 11] = [
    (4693, 26177, 4345, 5945, 5204520, 1585770),
    (6928, 34899, 4360, 5918, 4416628, 686848),
    (16478, 39090, 6752, 9215, 8257449, 844866),
    (28827, 34751, 6752, 9215, 387196, 959901),
    (34129, 22954, 5785, 7867, -4217541, 4255042),
    (41798, 15354, 5333, 7273, 1819082, 1665090),
    (38324, 5426, 4857, 6595, -824660, 891534),
    (29078, 3952, 4857, 6595, -8950887, 1091722),
    (22141, 4720, 4365, 5945, -9809656, 1061181),
    (14000, 5192, 6753, 9190, -4002417, 739161),
    (4127, 15789, 6753, 9190, 9459261, 711490),
];

/// The rounded rectangles (`round1Rect`, `round2SameRect`, `round2DiagRect`) and the quarter-ellipse
/// pie wedge (`pieWedge`), or `None` for another preset.
///
/// Each corner is a quarter arc of radius `dx1`/`bx1`/`a`; a corner whose adjust is 0 still emits its
/// `arcTo`, with zero radii. Points and radii are pinned into the box.
fn rounded_rect_paths(g: &ShapeGeometry, wi: i128, hi: i128) -> Option<Vec<PathCommand>> {
    let (l, t, r, b) = (0, 0, wi, hi);
    let ss = wi.min(hi);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let adj = |name: &str, default: i128| adjust(g, name, default).clamp(0, 50000);
    let (cd2, cd4, cd34) = (10_800_000, 5_400_000, 16_200_000);
    let x = |v: i128| i64::try_from(v.clamp(l, r)).unwrap_or(0);
    let y = |v: i128| i64::try_from(v.clamp(t, b)).unwrap_or(0);
    let m = |px: i128, py: i128| PathCommand::MoveTo(x(px), y(py));
    let ln = |px: i128, py: i128| PathCommand::LineTo(x(px), y(py));
    let arc = |w_r: i128, h_r: i128, st_ang: i64, sw_ang: i64| PathCommand::ArcTo {
        w_r: i64::try_from(w_r.clamp(0, r)).unwrap_or(0),
        h_r: i64::try_from(h_r.clamp(0, b)).unwrap_or(0),
        st_ang,
        sw_ang,
    };
    let close = PathCommand::Close;
    Some(match g.preset.as_str() {
        "round1Rect" => {
            let dx1 = md(ss, adj("adj", 16667), 100000);
            let x1 = r - dx1;
            vec![m(l, t), ln(x1, t), arc(dx1, dx1, cd34, cd4), ln(r, b), ln(l, b), close]
        }
        "round2SameRect" => {
            let tx1 = md(ss, adj("adj1", 16667), 100000);
            let bx1 = md(ss, adj("adj2", 0), 100000);
            let (tx2, by1) = (r - tx1, b - bx1);
            vec![
                m(tx1, t),
                ln(tx2, t),
                arc(tx1, tx1, cd34, cd4),
                ln(r, by1),
                arc(bx1, bx1, 0, cd4),
                ln(bx1, b),
                arc(bx1, bx1, cd4, cd4),
                ln(l, tx1),
                arc(tx1, tx1, cd2, cd4),
                close,
            ]
        }
        "round2DiagRect" => {
            let x1 = md(ss, adj("adj1", 16667), 100000);
            let a = md(ss, adj("adj2", 0), 100000);
            let (x2, y1) = (r - a, b - x1);
            vec![
                m(x1, t),
                ln(x2, t),
                arc(a, a, cd34, cd4),
                ln(r, y1),
                arc(x1, x1, 0, cd4),
                ln(a, b),
                arc(a, a, cd4, cd4),
                ln(l, x1),
                arc(x1, x1, cd2, cd4),
                close,
            ]
        }
        "pieWedge" => vec![m(l, b), arc(wi, hi, cd2, cd4), ln(r, b), close],
        _ => return None,
    })
}

/// Plaque tabs, the funnel and the six- and nine-tooth gears: filled and stroked paths of lines and
/// arcs whose guides use trigonometry. `w` and `h` are positive. Guides are evaluated in `f64` and
/// rounded once, as points and radii pinned into the box; a huge box cannot overflow `f64`.
fn tabs_funnel_gears(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    use PathCommand::{ArcTo, Close, LineTo, MoveTo};
    let (wi, hi) = (w as f64, h as f64);
    let ss = wi.min(hi);
    let (r, b, hc, vc, wd2, hd2) = (wi, hi, wi / 2.0, hi / 2.0, wi / 2.0, hi / 2.0);
    let (cd4, cd2, cd34) = (5_400_000.0, 10_800_000.0, 16_200_000.0);
    // `as` saturates and maps NaN to 0.
    let pin = |v: f64, max: i64| (v.round() as i64).clamp(0, max);
    let m = |x: f64, y: f64| MoveTo(pin(x, w), pin(y, h));
    let ln = |x: f64, y: f64| LineTo(pin(x, w), pin(y, h));
    let angle = |a: f64| a.round() as i64;
    let arc = |w_r: f64, h_r: f64, st: f64, sw: f64| ArcTo { w_r: pin(w_r, w), h_r: pin(h_r, h), st_ang: angle(st), sw_ang: angle(sw) };
    let path = |commands: Vec<PathCommand>| GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands };
    match g.preset.as_str() {
        "plaqueTabs" => {
            let dx = guide_md(guide_mod(wi, hi, 0.0), 1.0, 20.0);
            let (y1, x1) = (b - dx, r - dx);
            Some(vec![
                path(vec![m(0.0, 0.0), ln(dx, 0.0), arc(dx, dx, 0.0, cd4), Close]),
                path(vec![m(0.0, y1), arc(dx, dx, cd34, cd4), ln(0.0, b), Close]),
                path(vec![m(r, 0.0), ln(r, dx), arc(dx, dx, cd4, cd4), Close]),
                path(vec![m(x1, b), arc(dx, dx, cd2, cd4), ln(r, b), Close]),
            ])
        }
        "funnel" => {
            let hd4 = hi / 4.0;
            let d = guide_md(ss, 1.0, 20.0);
            let (rw2, rh2) = (wd2 - d, hd4 - d);
            let da = guide_at2(guide_cos(wd2, 480_000.0), guide_sin(hd4, 480_000.0));
            let da2 = da * 2.0;
            let (st_ang1, sw_ang1, sw_ang3) = (cd2 - da, cd2 + da2, cd2 - da2);
            let (rw3, rh3) = (guide_md(wd2, 1.0, 4.0), guide_md(hd4, 1.0, 4.0));
            let (dx1, dy1) = ellipse_offset(wd2, hd4, st_ang1);
            let (x1, y1) = (hc + dx1, hd4 + dy1);
            let (dx3, dy3) = ellipse_offset(rw3, rh3, da);
            let (x3, y2) = (hc + dx3, b - rh3 + dy3);
            let x2 = wd2 - rw2;
            Some(vec![path(vec![
                m(x1, y1),
                arc(wd2, hd4, st_ang1, sw_ang1),
                ln(x3, y2),
                arc(rw3, rh3, da, sw_ang3),
                Close,
                m(x2, hd4),
                arc(rw2, rh2, cd2, -21_600_000.0),
                Close,
            ])])
        }
        "gear6" | "gear9" => {
            let six = g.preset == "gear6";
            let a1 = adjust(g, "adj1", if six { 15000 } else { 10000 }).clamp(0, 20000) as f64;
            let a2 = adjust(g, "adj2", if six { 3526 } else { 1763 }).clamp(0, if six { 5358 } else { 2679 }) as f64;
            let th = guide_md(ss, a1, 100000.0);
            let lfd = guide_md(ss, a2, 100000.0);
            let l3 = guide_md(th, 1.0, 2.0) + guide_md(lfd, 1.0, 2.0);
            let (rh, rw) = (hd2 - th, wd2 - th);
            let maxr = if rw - rh > 0.0 { rh } else { rw };
            let gear = Gear { rw, rh, hc, vc, ha: guide_at2(maxr, l3), th, lfd };
            let ellipse = |a: f64| ellipse_angle(rw, rh, a);
            let rim = |b_d: f64, sw: f64| arc(rw, rh, b_d, sw);
            // The tooth at the top: its corners are offset straight up from the rim.
            let (b_a9, b_d9) = (ellipse(cd34 - gear.ha), ellipse(cd34 + gear.ha));
            let (dxd9, dyd9) = ellipse_offset(rw, rh, b_d9);
            let (x_d9, y_d9) = (hc + dxd9, vc + dyd9);
            let x_a9 = hc - dxd9;
            let (x_f9, x_e9, y_c9) = (x_d9 - lfd, x_a9 + lfd, y_d9 - th);
            let t1 = gear.tooth(if six { 19_800_000.0 } else { 18_600_000.0 });
            if six {
                let b_a2 = ellipse(1_800_000.0 - gear.ha);
                let b_d2 = ellipse(1_800_000.0 + gear.ha);
                let b_d3 = ellipse(cd4 + gear.ha);
                let b_d4 = ellipse(9_000_000.0 + gear.ha);
                let b_d5 = ellipse(12_600_000.0 + gear.ha);
                let (sw1, sw2) = (t1.b_a - b_d9, b_a2 - t1.b_d);
                let (y_a2, y_d2, y_c2, y_b2) = (hi - t1.d.1, hi - t1.a.1, hi - t1.b.1, hi - t1.c.1);
                let (y_d3, y_b3) = (hi - y_d9, hi - y_c9);
                let (x_d4, x_c4, x_b4) = (wi - t1.d.0, wi - t1.c.0, wi - t1.b.0);
                let (x_d5, x_c5, x_b5) = (wi - t1.a.0, wi - t1.b.0, wi - t1.c.0);
                return Some(vec![path(vec![
                    m(t1.a.0, t1.a.1),
                    ln(t1.b.0, t1.b.1),
                    ln(t1.c.0, t1.c.1),
                    ln(t1.d.0, t1.d.1),
                    rim(t1.b_d, sw2),
                    ln(t1.c.0, y_b2),
                    ln(t1.b.0, y_c2),
                    ln(t1.a.0, y_d2),
                    rim(b_d2, sw1),
                    ln(x_f9, y_b3),
                    ln(x_e9, y_b3),
                    ln(x_a9, y_d3),
                    rim(b_d3, sw1),
                    ln(x_b4, y_c2),
                    ln(x_c4, y_b2),
                    ln(x_d4, y_a2),
                    rim(b_d4, sw2),
                    ln(x_b5, t1.c.1),
                    ln(x_c5, t1.b.1),
                    ln(x_d5, t1.a.1),
                    rim(b_d5, sw1),
                    ln(x_e9, y_c9),
                    ln(x_f9, y_c9),
                    ln(x_d9, y_d9),
                    rim(b_d9, sw1),
                    Close,
                ])]);
            }
            let t2 = gear.tooth(21_000_000.0);
            let t3 = gear.tooth(1_800_000.0);
            let t4 = gear.tooth(4_200_000.0);
            let b_a5 = ellipse(6_600_000.0 - gear.ha);
            let b_d5 = ellipse(6_600_000.0 + gear.ha);
            let b_d6 = ellipse(9_000_000.0 + gear.ha);
            let b_d7 = ellipse(11_400_000.0 + gear.ha);
            let b_d8 = ellipse(13_800_000.0 + gear.ha);
            let (sw1, sw2, sw3) = (t2.b_a - t1.b_d, t3.b_a - t2.b_d, t4.b_a - t3.b_d);
            let (sw4, sw5) = (b_a5 - t4.b_d, b_a9 - b_d8);
            // Teeth 5 to 8 mirror teeth 4 to 1 across the vertical centre line.
            let mirror = |t: &Tooth| (wi - t.c.0, wi - t.b.0, wi - t.a.0);
            let mut commands = vec![m(t1.a.0, t1.a.1)];
            for (t, b_d, sw) in [(&t1, t1.b_d, sw1), (&t2, t2.b_d, sw2), (&t3, t3.b_d, sw3), (&t4, t4.b_d, sw4)] {
                commands.extend([ln(t.b.0, t.b.1), ln(t.c.0, t.c.1), ln(t.d.0, t.d.1), rim(b_d, sw)]);
            }
            for (t, b_d, sw) in [(&t4, b_d5, sw3), (&t3, b_d6, sw2), (&t2, b_d7, sw1), (&t1, b_d8, sw5)] {
                let (x_b, x_c, x_d) = mirror(t);
                commands.extend([ln(x_b, t.c.1), ln(x_c, t.b.1), ln(x_d, t.a.1), rim(b_d, sw)]);
            }
            commands.extend([ln(x_e9, y_c9), ln(x_f9, y_c9), ln(x_d9, y_d9), rim(b_d9, sw5), Close]);
            Some(vec![path(commands)])
        }
        _ => None,
    }
}

/// The rim ellipse and tooth sizes of a gear preset.
struct Gear {
    rw: f64,
    rh: f64,
    hc: f64,
    vc: f64,
    /// Half the angle a tooth spans on the rim.
    ha: f64,
    th: f64,
    lfd: f64,
}

/// One gear tooth: the rim angles where it starts (`b_a`) and ends (`b_d`), and its corners
/// A (on the rim), B, C (the outer edge) and D (on the rim).
struct Tooth {
    b_a: f64,
    b_d: f64,
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
}

impl Gear {
    /// The tooth centred on the angle `mid`, from the `aA`…`xB` guides of the gear presets.
    fn tooth(&self, mid: f64) -> Tooth {
        let corner = |angle: f64| {
            let b_x = ellipse_angle(self.rw, self.rh, angle);
            let (dx, dy) = ellipse_offset(self.rw, self.rh, b_x);
            (b_x, ((self.hc + dx), (self.vc + dy)))
        };
        let (b_a, a) = corner(mid - self.ha);
        let (b_d, d) = corner(mid + self.ha);
        let dir = guide_at2(a.1 - d.1, a.0 - d.0);
        let (dx_f, dy_f) = (guide_sin(self.lfd, dir), guide_cos(self.lfd, dir));
        let (y_ct, x_ct) = (guide_sin(self.th, dir), guide_cos(self.th, dir));
        let f = ((d.0 + dx_f), (d.1 + dy_f));
        let e = ((a.0 - dx_f), (a.1 - dy_f));
        Tooth { b_a, b_d, a, b: ((e.0 - x_ct), (e.1 + y_ct)), c: ((f.0 - x_ct), (f.1 + y_ct)), d }
    }
}

/// The angle on the `rw` × `rh` ellipse for the guide angle `a` (`at2 (cos rw a) (sin rh a)`).
fn ellipse_angle(rw: f64, rh: f64, a: f64) -> f64 {
    guide_at2(guide_cos(rw, a), guide_sin(rh, a))
}

/// The offset from the centre of the point at angle `a` on the `rw` × `rh` ellipse.
fn ellipse_offset(rw: f64, rh: f64, a: f64) -> (f64, f64) {
    let n = guide_md(rw, rh, guide_mod(guide_cos(rh, a), guide_sin(rw, a), 0.0));
    (guide_cos(n, a), guide_sin(n, a))
}

/// `*/ x y z`: `x * y / z`; 0 when `z` is zero.
fn guide_md(x: f64, y: f64, z: f64) -> f64 {
    if z == 0.0 { 0.0 } else { x * y / z }
}

/// A guide angle (60000ths of a degree) in radians.
fn guide_radians(a: f64) -> f64 {
    (a / 60000.0).to_radians()
}

/// `cos r a`.
fn guide_cos(r: f64, a: f64) -> f64 {
    r * guide_radians(a).cos()
}

/// `sin r a`.
fn guide_sin(r: f64, a: f64) -> f64 {
    r * guide_radians(a).sin()
}

/// `at2 x y`: `atan2(y, x)` in 60000ths of a degree.
fn guide_at2(x: f64, y: f64) -> f64 {
    y.atan2(x).to_degrees() * 60000.0
}

/// `mod x y z`: `sqrt(x² + y² + z²)`.
fn guide_mod(x: f64, y: f64, z: f64) -> f64 {
    (x * x + y * y + z * z).sqrt()
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

/// Arrow callouts (`leftArrowCallout` … `quadArrowCallout`): one filled, stroked polygon in the
/// shape box. Their guides are ratios of the box and the adjusts, so they are evaluated in `f64`;
/// every vertex is rounded and pinned into the box, and the guide pins bound every adjust, so a
/// hostile box or adjust cannot overflow.
fn arrow_callout(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    let (wf, hf) = (w as f64, h as f64);
    let ss = wf.min(hf);
    let (l, t, r, b, hc, vc) = (0.0, 0.0, wf, hf, wf / 2.0, hf / 2.0);
    // `x * y / z`, 0 when `z` is zero.
    let md = |x: f64, y: f64, z: f64| if z == 0.0 { 0.0 } else { x * y / z };
    // `pin x y z`: `y` clamped to `x..=z`.
    let pin = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi.max(lo));
    let adj = |name: &str, default: i128| adjust(g, name, default) as f64;
    // `a2` is the arrow head's half-depth, `a1` the shaft's; `maxAdj1 = */ a2 2 1` keeps the
    // shaft from being deeper than the head.
    let head = |adj2: f64, adj1: f64, max: f64| {
        let a2 = pin(adj2, 0.0, max);
        (a2, pin(adj1, 0.0, md(a2, 2.0, 1.0)))
    };
    // The horizontal arrow's top and bottom edges: `y1`/`y4` at the head's depth, `y2`/`y3` at
    // the shaft's.
    let rows = |a2: f64, a1: f64| {
        let (dy1, dy2) = (md(ss, a2, 100000.0), md(ss, a1, 200000.0));
        (vc - dy1, vc - dy2, vc + dy2, vc + dy1)
    };
    // The vertical arrow's left and right edges: `x1`/`x4` at the head's width, `x2`/`x3` at the
    // shaft's.
    let cols = |a2: f64, a1: f64| {
        let (dx1, dx2) = (md(ss, a2, 100000.0), md(ss, a1, 200000.0));
        (hc - dx1, hc - dx2, hc + dx2, hc + dx1)
    };
    let points: Vec<(f64, f64)> = match g.preset.as_str() {
        "leftArrowCallout" => {
            let (a2, a1) = head(adj("adj2", 25000), adj("adj1", 25000), md(50000.0, hf, ss));
            let a3 = pin(adj("adj3", 25000), 0.0, md(100000.0, wf, ss));
            let a4 = pin(adj("adj4", 64977), 0.0, 100000.0 - md(a3, ss, wf));
            let (y1, y2, y3, y4) = rows(a2, a1);
            let x1 = md(ss, a3, 100000.0);
            let x2 = r - md(wf, a4, 100000.0);
            vec![(l, vc), (x1, y1), (x1, y2), (x2, y2), (x2, t), (r, t), (r, b), (x2, b), (x2, y3), (x1, y3), (x1, y4)]
        }
        "rightArrowCallout" => {
            let (a2, a1) = head(adj("adj2", 25000), adj("adj1", 25000), md(50000.0, hf, ss));
            let a3 = pin(adj("adj3", 25000), 0.0, md(100000.0, wf, ss));
            let a4 = pin(adj("adj4", 64977), 0.0, 100000.0 - md(a3, ss, wf));
            let (y1, y2, y3, y4) = rows(a2, a1);
            let x2 = md(wf, a4, 100000.0);
            let x3 = r - md(ss, a3, 100000.0);
            vec![(l, t), (x2, t), (x2, y2), (x3, y2), (x3, y1), (r, vc), (x3, y4), (x3, y3), (x2, y3), (x2, b), (l, b)]
        }
        "upArrowCallout" => {
            let (a2, a1) = head(adj("adj2", 25000), adj("adj1", 25000), md(50000.0, wf, ss));
            let a3 = pin(adj("adj3", 25000), 0.0, md(100000.0, hf, ss));
            let a4 = pin(adj("adj4", 64977), 0.0, 100000.0 - md(a3, ss, hf));
            let (x1, x2, x3, x4) = cols(a2, a1);
            let y1 = md(ss, a3, 100000.0);
            let y2 = b - md(hf, a4, 100000.0);
            vec![(l, y2), (x2, y2), (x2, y1), (x1, y1), (hc, t), (x4, y1), (x3, y1), (x3, y2), (r, y2), (r, b), (l, b)]
        }
        "downArrowCallout" => {
            let (a2, a1) = head(adj("adj2", 25000), adj("adj1", 25000), md(50000.0, wf, ss));
            let a3 = pin(adj("adj3", 25000), 0.0, md(100000.0, hf, ss));
            let a4 = pin(adj("adj4", 64977), 0.0, 100000.0 - md(a3, ss, hf));
            let (x1, x2, x3, x4) = cols(a2, a1);
            let y2 = md(hf, a4, 100000.0);
            let y3 = b - md(ss, a3, 100000.0);
            vec![(l, t), (r, t), (r, y2), (x3, y2), (x3, y3), (x4, y3), (hc, b), (x1, y3), (x2, y3), (x2, y2), (l, y2)]
        }
        "leftRightArrowCallout" => {
            let (a2, a1) = head(adj("adj2", 25000), adj("adj1", 25000), md(50000.0, hf, ss));
            let a3 = pin(adj("adj3", 25000), 0.0, md(50000.0, wf, ss));
            let a4 = pin(adj("adj4", 48123), 0.0, 100000.0 - md(a3, ss, hc));
            let (y1, y2, y3, y4) = rows(a2, a1);
            let x1 = md(ss, a3, 100000.0);
            let x4 = r - x1;
            let dx2 = md(wf, a4, 200000.0);
            let (x2, x3) = (hc - dx2, hc + dx2);
            vec![
                (l, vc),
                (x1, y1),
                (x1, y2),
                (x2, y2),
                (x2, t),
                (x3, t),
                (x3, y2),
                (x4, y2),
                (x4, y1),
                (r, vc),
                (x4, y4),
                (x4, y3),
                (x3, y3),
                (x3, b),
                (x2, b),
                (x2, y3),
                (x1, y3),
                (x1, y4),
            ]
        }
        "upDownArrowCallout" => {
            let (a2, a1) = head(adj("adj2", 25000), adj("adj1", 25000), md(50000.0, wf, ss));
            let a3 = pin(adj("adj3", 25000), 0.0, md(50000.0, hf, ss));
            let a4 = pin(adj("adj4", 48123), 0.0, 100000.0 - md(a3, ss, vc));
            let (x1, x2, x3, x4) = cols(a2, a1);
            let y1 = md(ss, a3, 100000.0);
            let y4 = b - y1;
            let dy2 = md(hf, a4, 200000.0);
            let (y2, y3) = (vc - dy2, vc + dy2);
            vec![
                (l, y2),
                (x2, y2),
                (x2, y1),
                (x1, y1),
                (hc, t),
                (x4, y1),
                (x3, y1),
                (x3, y2),
                (r, y2),
                (r, y3),
                (x3, y3),
                (x3, y4),
                (x4, y4),
                (hc, b),
                (x1, y4),
                (x2, y4),
                (x2, y3),
                (l, y3),
            ]
        }
        "quadArrowCallout" => {
            let a2 = pin(adj("adj2", 18515), 0.0, 50000.0);
            let a1 = pin(adj("adj1", 18515), 0.0, md(a2, 2.0, 1.0));
            let a3 = pin(adj("adj3", 18515), 0.0, 50000.0 - a2);
            let a4 = pin(adj("adj4", 48123), a1, 100000.0 - md(a3, 2.0, 1.0));
            let ah = md(ss, a3, 100000.0);
            let (dx2, dx3) = (md(ss, a2, 100000.0), md(ss, a1, 200000.0));
            let (x3, x6) = (hc - dx2, hc + dx2);
            let (x4, x5) = (hc - dx3, hc + dx3);
            let (x2, x7) = (hc - md(wf, a4, 200000.0), hc + md(wf, a4, 200000.0));
            let (y2, y7) = (vc - md(hf, a4, 200000.0), vc + md(hf, a4, 200000.0));
            let (y3, y6) = (vc - dx2, vc + dx2);
            let (y4, y5) = (vc - dx3, vc + dx3);
            let (x8, y8) = (r - ah, b - ah);
            vec![
                (l, vc),
                (ah, y3),
                (ah, y4),
                (x2, y4),
                (x2, y2),
                (x4, y2),
                (x4, ah),
                (x3, ah),
                (hc, t),
                (x6, ah),
                (x5, ah),
                (x5, y2),
                (x7, y2),
                (x7, y4),
                (x8, y4),
                (x8, y3),
                (r, vc),
                (x8, y6),
                (x8, y5),
                (x7, y5),
                (x7, y7),
                (x5, y7),
                (x5, y8),
                (x6, y8),
                (hc, b),
                (x3, y8),
                (x4, y8),
                (x4, y7),
                (x2, y7),
                (x2, y5),
                (ah, y5),
                (ah, y6),
            ]
        }
        _ => return None,
    };
    let mut commands = Vec::with_capacity(points.len() + 1);
    for (i, &(x, y)) in points.iter().enumerate() {
        let (x, y) = (x.round().clamp(0.0, wf) as i64, y.round().clamp(0.0, hf) as i64);
        commands.push(if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) });
    }
    commands.push(PathCommand::Close);
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// The "No" symbol (`noSmoking`), the division sign (`mathDivide`) and the teardrop: one filled and
/// stroked path each, built from arcs, quadratic Béziers and lines. `None` for another preset.
///
/// Guides are evaluated in `f64` (most are trigonometric or square roots, and the arc angles need
/// more than integer precision); `f64` cannot overflow and the final `as` casts saturate. Points
/// and radii are clamped into the box; Bézier control points are not (the teardrop's tip leaves the
/// box when `adj` > 100000).
fn symbol_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    let (wf, hf) = (w as f64, h as f64);
    let (l, t, r, b, hc, vc, wd2, hd2) = (0.0, 0.0, wf, hf, wf / 2.0, hf / 2.0, wf / 2.0, hf / 2.0);
    let ss = wf.min(hf);
    // `pin lo x hi`: `x` raised to `lo`, then lowered to `hi`.
    let pin = |lo: f64, x: f64, hi: f64| {
        if x < lo {
            lo
        } else if x > hi {
            hi
        } else {
            x
        }
    };
    let adj = |name: &str, default: i128| adjust(g, name, default) as f64;
    // `*/ x y z`; 0 when `z` is zero, like `md`.
    let md = |x: f64, y: f64, z: f64| if z == 0.0 { 0.0 } else { x * y / z };
    // Angles are in 60000ths of a degree.
    let rad = |a: f64| (a / 60000.0).to_radians();
    let at2 = |x: f64, y: f64| y.atan2(x).to_degrees() * 60000.0;
    let cos = |r: f64, a: f64| r * rad(a).cos();
    let sin = |r: f64, a: f64| r * rad(a).sin();
    let m = |x: f64, y: f64| PathCommand::MoveTo((x.round() as i64).clamp(0, w), (y.round() as i64).clamp(0, h));
    let ln = |x: f64, y: f64| PathCommand::LineTo((x.round() as i64).clamp(0, w), (y.round() as i64).clamp(0, h));
    let arc = |w_r: f64, h_r: f64, st_ang: f64, sw_ang: f64| PathCommand::ArcTo {
        w_r: (w_r.round() as i64).clamp(0, w),
        h_r: (h_r.round() as i64).clamp(0, h),
        st_ang: st_ang.round() as i64,
        sw_ang: sw_ang.round() as i64,
    };
    let quad =
        |x1: f64, y1: f64, x2: f64, y2: f64| PathCommand::QuadBezTo([(x1.round() as i64, y1.round() as i64), (x2.round() as i64, y2.round() as i64)]);
    let (cd4, cd2, cd34) = (5_400_000.0, 10_800_000.0, 16_200_000.0);
    let close = PathCommand::Close;
    let commands = match g.preset.as_str() {
        "noSmoking" => {
            let a = pin(0.0, adj("adj", 18750), 50000.0);
            let dr = md(ss, a, 100000.0);
            let (iwd2, ihd2) = (wd2 - dr, hd2 - dr);
            let ang = at2(wf, hf);
            let (ct, st) = (cos(ihd2, ang), sin(iwd2, ang));
            let n = md(iwd2, ihd2, ct.hypot(st));
            let dang = at2(n, dr / 2.0);
            let sw_ang = -cd2 + 2.0 * dang;
            let st_ang1 = ang - dang;
            let st_ang2 = st_ang1 - cd2;
            let (ct1, st1) = (cos(ihd2, st_ang1), sin(iwd2, st_ang1));
            let n1 = md(iwd2, ihd2, ct1.hypot(st1));
            let (dx1, dy1) = (cos(n1, st_ang1), sin(n1, st_ang1));
            vec![
                m(l, vc),
                arc(wd2, hd2, cd2, cd4),
                arc(wd2, hd2, cd34, cd4),
                arc(wd2, hd2, 0.0, cd4),
                arc(wd2, hd2, cd4, cd4),
                close.clone(),
                m(hc + dx1, vc + dy1),
                arc(iwd2, ihd2, st_ang1, sw_ang),
                close.clone(),
                m(hc - dx1, vc - dy1),
                arc(iwd2, ihd2, st_ang2, sw_ang),
                close,
            ]
        }
        "mathDivide" => {
            let a1 = pin(1000.0, adj("adj1", 23520), 36745.0);
            let ma3h = (73490.0 - a1) / 4.0;
            let ma3w = md(36745.0, wf, hf);
            let a3 = pin(1000.0, adj("adj3", 11760), ma3h.min(ma3w));
            let a2 = pin(0.0, adj("adj2", 5880), 73490.0 - 4.0 * a3 - a1);
            let dy1 = md(hf, a1, 200000.0);
            let yg = md(hf, a2, 100000.0);
            let rad = md(hf, a3, 100000.0);
            let dx1 = md(wf, 73490.0, 200000.0);
            let (y3, y4) = (vc - dy1, vc + dy1);
            let y1 = y3 - (yg + rad) - rad;
            let y5 = b - y1;
            let (x1, x3) = (hc - dx1, hc + dx1);
            vec![
                m(hc, y1),
                arc(rad, rad, cd34, 21_600_000.0),
                close.clone(),
                m(hc, y5),
                arc(rad, rad, cd4, 21_600_000.0),
                close.clone(),
                m(x1, y3),
                ln(x3, y3),
                ln(x3, y4),
                ln(x1, y4),
                close,
            ]
        }
        "teardrop" => {
            let a = pin(0.0, adj("adj", 100000), 200000.0);
            let r2 = std::f64::consts::SQRT_2;
            let (sw, sh) = (md(wd2 * r2, a, 100000.0), md(hd2 * r2, a, 100000.0));
            let (x1, y1) = (hc + cos(sw, 2_700_000.0), vc - sin(sh, 2_700_000.0));
            let (x2, y2) = ((hc + x1) / 2.0, (vc + y1) / 2.0);
            vec![m(l, vc), arc(wd2, hd2, cd2, cd4), quad(x2, t, x1, y1), quad(r, y2, r, vc), arc(wd2, hd2, 0.0, cd4), arc(wd2, hd2, cd4, cd4), close]
        }
        _ => return None,
    };
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

/// Wedge callouts (`wedgeRectCallout`, `wedgeRoundRectCallout`, `wedgeEllipseCallout`), or `None`
/// for another preset. The body stays in the box; the tail tip is where `adj1`/`adj2` put it,
/// relative to the centre, so it is usually outside the box. The tip is bounded to ten box sizes
/// from the centre.
fn wedge_callout(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if !matches!(g.preset.as_str(), "wedgeRectCallout" | "wedgeRoundRectCallout" | "wedgeEllipseCallout") {
        return None;
    }
    let (wi, hi) = (i128::from(w), i128::from(h));
    let (l, t, r, b, hc, vc) = (0, 0, wi, hi, wi / 2, hi / 2);
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let a1 = adjust(g, "adj1", -20833).clamp(-1_000_000, 1_000_000);
    let a2 = adjust(g, "adj2", 62500).clamp(-1_000_000, 1_000_000);
    // Saturating, not pinned into the box: the tail tip may lie outside it.
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX });
    let m = |x: i128, y: i128| PathCommand::MoveTo(to_i64(x), to_i64(y));
    let ln = |x: i128, y: i128| PathCommand::LineTo(to_i64(x), to_i64(y));
    let path = |commands: Vec<PathCommand>| vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }];
    if g.preset == "wedgeEllipseCallout" {
        // Angles in floating point, as the guides' `at2`/`cos`/`sin` are, rounded at the end.
        let (wf, hf) = (w as f64, h as f64);
        let (wd2, hd2) = (wf / 2.0, hf / 2.0);
        let (dx_pos, dy_pos) = (wf * a1 as f64 / 100000.0, hf * a2 as f64 / 100000.0);
        let pang = (dy_pos * wf).atan2(dx_pos * hf);
        let wing = 11f64.to_radians();
        let (st, en) = (pang + wing, pang - wing);
        let (dx1, dy1) = (wd2 * st.cos(), hd2 * st.sin());
        let (dx2, dy2) = (wd2 * en.cos(), hd2 * en.sin());
        let ang = |rad: f64| (rad.to_degrees() * 60000.0).round() as i64;
        let st_ang = ang(dy1.atan2(dx1));
        let sw1 = ang(dy2.atan2(dx2)).saturating_sub(st_ang);
        let sw_ang = if sw1 > 0 { sw1 } else { sw1.saturating_add(21_600_000) };
        let pt = |x: f64, y: f64| (x.round() as i64, y.round() as i64);
        let (tx, ty) = pt(wd2 + dx_pos, hd2 + dy_pos);
        let (x1, y1) = pt(wd2 + dx1, hd2 + dy1);
        let arc = PathCommand::ArcTo { w_r: to_i64(hc), h_r: to_i64(vc), st_ang, sw_ang };
        return Some(path(vec![PathCommand::MoveTo(tx, ty), PathCommand::LineTo(x1, y1), arc, PathCommand::Close]));
    }
    let dx_pos = md(wi, a1, 100000);
    let dy_pos = md(hi, a2, 100000);
    let (x_pos, y_pos) = (hc.saturating_add(dx_pos), vc.saturating_add(dy_pos));
    let dq = md(dx_pos, hi, wi);
    let dz = dy_pos.saturating_abs().saturating_sub(dq.saturating_abs());
    let pick = |c: i128, y: i128, z: i128| if c > 0 { y } else { z };
    let (x1, x2) = (md(wi, pick(dx_pos, 7, 2), 12), md(wi, pick(dx_pos, 10, 5), 12));
    let (y1, y2) = (md(hi, pick(dy_pos, 7, 2), 12), md(hi, pick(dy_pos, 10, 5), 12));
    let xl = pick(dz, l, pick(dx_pos, l, x_pos));
    let xt = pick(dz, pick(dy_pos, x1, x_pos), x1);
    let xr = pick(dz, r, pick(dx_pos, x_pos, r));
    let xb = pick(dz, pick(dy_pos, x_pos, x1), x1);
    let yl = pick(dz, y1, pick(dx_pos, y1, y_pos));
    let yt = pick(dz, pick(dy_pos, t, y_pos), t);
    let yr = pick(dz, y1, pick(dx_pos, y_pos, y1));
    let yb = pick(dz, pick(dy_pos, y_pos, b), b);
    if g.preset == "wedgeRectCallout" {
        return Some(path(vec![
            m(l, t),
            ln(x1, t),
            ln(xt, yt),
            ln(x2, t),
            ln(r, t),
            ln(r, y1),
            ln(xr, yr),
            ln(r, y2),
            ln(r, b),
            ln(x2, b),
            ln(xb, yb),
            ln(x1, b),
            ln(l, b),
            ln(l, y2),
            ln(xl, yl),
            ln(l, y1),
            PathCommand::Close,
        ]));
    }
    // Corner radius: `adj3` of the shorter side, at most half of it.
    let u1 = md(wi.min(hi), adjust(g, "adj3", 16667).clamp(0, 50000), 100000);
    let (u2, v2) = (r - u1, b - u1);
    let (cd4, cd2, cd34) = (5_400_000, 10_800_000, 16_200_000);
    let arc = |st_ang: i64| PathCommand::ArcTo { w_r: to_i64(u1), h_r: to_i64(u1), st_ang, sw_ang: cd4 };
    Some(path(vec![
        m(l, u1),
        arc(cd2),
        ln(x1, t),
        ln(xt, yt),
        ln(x2, t),
        ln(u2, t),
        arc(cd34),
        ln(r, y1),
        ln(xr, yr),
        ln(r, y2),
        ln(r, v2),
        arc(0),
        ln(x2, b),
        ln(xb, yb),
        ln(x1, b),
        ln(u1, b),
        arc(cd4),
        ln(l, y2),
        ln(xl, yl),
        ln(l, y1),
        PathCommand::Close,
    ]))
}

/// One command of an arc-arrow outline before rounding: `A` is `a:arcTo` (`wR`, `hR`, `stAng`, `swAng`).
enum ArcCmd {
    M(f64, f64),
    L(f64, f64),
    A(f64, f64, f64, f64),
    Z,
}

const CD2: f64 = 10_800_000.0;
const CD4: f64 = 5_400_000.0;
const CD34: f64 = 16_200_000.0;
const C360: f64 = 21_600_000.0;

/// Arrows built from lines and `a:arcTo` (bent, U-turn and the circular arrows): one filled and
/// stroked path. The guides chain trigonometry, so they are evaluated in `f64`; points and radii
/// are rounded and pinned into the box at the end, and NaN or infinite values become 0.
fn arc_arrow_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if !g.paths.is_empty() || w <= 0 || h <= 0 {
        return None;
    }
    let (wf, hf) = (w as f64, h as f64);
    let b = ArrowBox { w: wf, h: hf, ss: wf.min(hf), hc: wf / 2.0, vc: hf / 2.0 };
    let adj = |name: &str, default: f64| adjust(g, name, default as i128) as f64;
    let adjs = |d: [f64; 5]| [adj("adj1", d[0]), adj("adj2", d[1]), adj("adj3", d[2]), adj("adj4", d[3]), adj("adj5", d[4])];
    let cmds = match g.preset.as_str() {
        "bentArrow" => bent_arrow(&b, adjs([25000.0, 25000.0, 25000.0, 43750.0, 0.0])),
        "uturnArrow" => uturn_arrow(&b, adjs([25000.0, 25000.0, 25000.0, 43750.0, 75000.0])),
        "circularArrow" => circular_arrow(&b, adjs([12500.0, 1142319.0, 20457681.0, 10800000.0, 12500.0])),
        "leftCircularArrow" => left_circular_arrow(&b, adjs([12500.0, -1142319.0, 1142319.0, 10800000.0, 12500.0])),
        "leftRightCircularArrow" => left_right_circular_arrow(&b, adjs([12500.0, 1142319.0, 20457681.0, 11942319.0, 12500.0])),
        _ => return None,
    };
    let fix = |v: f64, max: i64| if v.is_finite() { (v.round() as i64).clamp(0, max) } else { 0 };
    let ang = |v: f64| if v.is_finite() { (v.round() as i64).clamp(-3 * 21_600_000, 3 * 21_600_000) } else { 0 };
    let commands = cmds
        .into_iter()
        .map(|c| match c {
            ArcCmd::M(x, y) => PathCommand::MoveTo(fix(x, w), fix(y, h)),
            ArcCmd::L(x, y) => PathCommand::LineTo(fix(x, w), fix(y, h)),
            ArcCmd::A(w_r, h_r, st, sw) => PathCommand::ArcTo { w_r: fix(w_r, w), h_r: fix(h_r, h), st_ang: ang(st), sw_ang: ang(sw) },
            ArcCmd::Z => PathCommand::Close,
        })
        .collect();
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, shade: PathShade::Norm, commands }])
}

fn bent_arrow(b: &ArrowBox, adj: [f64; 5]) -> Vec<ArcCmd> {
    use ArcCmd::{A, L, M, Z};
    let (r, bt, ss) = (b.w, b.h, b.ss);
    let a2 = pin(0.0, adj[1], 50000.0);
    let a1 = pin(0.0, adj[0], a2 * 2.0);
    let a3 = pin(0.0, adj[2], 50000.0);
    let th = ss * a1 / 100000.0;
    let aw2 = ss * a2 / 100000.0;
    let dh2 = aw2 - th / 2.0;
    let ah = ss * a3 / 100000.0;
    let bs = (r - ah).min(bt - dh2);
    let a4 = pin(0.0, adj[3], muldiv(100000.0, bs, ss));
    let bd = ss * a4 / 100000.0;
    let bd2 = (bd - th).max(0.0);
    let (x3, x4) = (th + bd2, r - ah);
    let y3 = dh2 + th;
    let (y4, y5) = (y3 + dh2, dh2 + bd);
    vec![
        M(0.0, bt),
        L(0.0, y5),
        A(bd, bd, CD2, CD4),
        L(x4, dh2),
        L(x4, 0.0),
        L(r, aw2),
        L(x4, y4),
        L(x4, y3),
        L(x3, y3),
        A(bd2, bd2, CD34, -CD4),
        L(th, bt),
        Z,
    ]
}

fn uturn_arrow(b: &ArrowBox, adj: [f64; 5]) -> Vec<ArcCmd> {
    use ArcCmd::{A, L, M, Z};
    let (r, bt, ss) = (b.w, b.h, b.ss);
    let a2 = pin(0.0, adj[1], 25000.0);
    let a1 = pin(0.0, adj[0], a2 * 2.0);
    let q2 = muldiv(a1, ss, b.h);
    let a3 = pin(0.0, adj[2], muldiv(100000.0 - q2, b.h, ss));
    let min_adj5 = muldiv(a3 + a1, ss, b.h);
    let a5 = pin(min_adj5, adj[4], 100000.0);
    let th = ss * a1 / 100000.0;
    let aw2 = ss * a2 / 100000.0;
    let dh2 = aw2 - th / 2.0;
    let y5 = b.h * a5 / 100000.0;
    let ah = ss * a3 / 100000.0;
    let y4 = y5 - ah;
    let x9 = r - dh2;
    let bs = (x9 / 2.0).min(y4);
    let a4 = pin(0.0, adj[3], muldiv(bs, 100000.0, ss));
    let bd = ss * a4 / 100000.0;
    let bd2 = (bd - th).max(0.0);
    let x3 = th + bd2;
    let x8 = r - aw2;
    let x6 = x8 - aw2;
    let x7 = x6 + dh2;
    let x4 = x9 - bd;
    vec![
        M(0.0, bt),
        L(0.0, bd),
        A(bd, bd, CD2, CD4),
        L(x4, 0.0),
        A(bd, bd, CD34, CD4),
        L(x9, y4),
        L(r, y4),
        L(x8, y5),
        L(x6, y4),
        L(x7, y4),
        L(x7, x3),
        A(bd2, bd2, 0.0, -CD4),
        L(x3, th),
        A(bd2, bd2, CD34, -CD4),
        L(th, bt),
        Z,
    ]
}

/// DrawingML angle (60000ths of a degree) in radians.
fn rad(a: f64) -> f64 {
    (a / 60000.0).to_radians()
}

/// `at2 x y`: atan2(y, x) in 60000ths of a degree.
fn at2(x: f64, y: f64) -> f64 {
    y.atan2(x).to_degrees() * 60000.0
}

/// `sqrt x`, 0 for a negative or NaN `x`.
fn sqrt0(x: f64) -> f64 {
    if x > 0.0 { x.sqrt() } else { 0.0 }
}

/// `?: x y z`.
fn ifpos(x: f64, y: f64, z: f64) -> f64 {
    if x > 0.0 { y } else { z }
}

/// The point at angle `a` on the ellipse of radii `rw` × `rh` about the box centre, as the
/// definitions compute it (`sin`/`cos`, then `cat2`/`sat2`).
fn ellipse_at(b: &ArrowBox, rw: f64, rh: f64, a: f64) -> (f64, f64) {
    let (wt, ht) = (rw * rad(a).sin(), rh * rad(a).cos());
    let t = wt.atan2(ht);
    (b.hc + rw * t.cos(), b.vc + rh * t.sin())
}

/// Where the line through (x1, y1)–(x2, y2) meets the circle of radius `r` about the origin:
/// of the two hits, the one nearer (`nx`, `ny`) (the `dxF`/`dxC` guide chains).
#[allow(clippy::too_many_arguments)]
fn circle_hit(x1: f64, y1: f64, x2: f64, y2: f64, r: f64, sdy: f64, nx: f64, ny: f64) -> (f64, f64) {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let d2 = dx * dx + dy * dy;
    let dd = x1 * y2 - x2 * y1;
    let sdel = sqrt0(r * r * d2 - dd * dd);
    let (ex, ey) = (sdy * dx * sdel, dy.abs() * sdel);
    let (x_1, x_2) = (muldiv(dd * dy + ex, 1.0, d2), muldiv(dd * dy - ex, 1.0, d2));
    let (y_1, y_2) = (muldiv(-dd * dx + ey, 1.0, d2), muldiv(-dd * dx - ey, 1.0, d2));
    let n1 = (nx - x_1).hypot(ny - y_1);
    let n2 = (nx - x_2).hypot(ny - y_2);
    if n2 - n1 > 0.0 { (x_1, y_1) } else { (x_2, y_2) }
}

/// The guides the three circular arrows share, up to the arrowhead at `enAng`.
struct CircArrow {
    st_ang: f64,
    a_ang: f64,
    thh: f64,
    rw1: f64,
    rh1: f64,
    rw2: f64,
    rh2: f64,
    rw3: f64,
    rh3: f64,
    a: (f64, f64),
    gp: (f64, f64),
    bp: (f64, f64),
    f: (f64, f64),
    c: (f64, f64),
    p5: f64,
    /// `en2`: angle of `F` on the outer ellipse.
    en2: f64,
    /// `istAng` before the per-preset adjustment: angle of `C` on the inner ellipse.
    ist: f64,
}

/// `left` picks leftCircularArrow's negative `aAng`.
fn circ_arrow(b: &ArrowBox, adj: [f64; 5], left: bool) -> CircArrow {
    let (hc, vc, ss) = (b.hc, b.vc, b.ss);
    let a5 = pin(0.0, adj[4], 25000.0);
    let a1 = pin(0.0, adj[0], a5 * 2.0);
    let en_ang = pin(1.0, adj[2], 21599999.0);
    let st_ang = pin(0.0, adj[3], 21599999.0);
    let th = ss * a1 / 100000.0;
    let thh = ss * a5 / 100000.0;
    let th2 = th / 2.0;
    let rw1 = b.w / 2.0 + th2 - thh;
    let rh1 = b.h / 2.0 + th2 - thh;
    let (rw2, rh2) = (rw1 - th, rh1 - th);
    let (rw3, rh3) = (rw2 + th2, rh2 + th2);
    let (x_h, y_h) = ellipse_at(b, rw3, rh3, en_ang);
    let (dx_h, dy_h) = (x_h - hc, y_h - vc);
    let r_i = rw2.min(rh2);
    let u1 = dx_h * dx_h;
    let u2 = dy_h * dy_h;
    let u3 = r_i * r_i;
    let (u4, u5) = (u1 - u3, u2 - u3);
    let u7 = muldiv(muldiv(u4, u5, u1), 1.0, u2);
    let u9 = sqrt0(1.0 - u7);
    let u11 = muldiv(muldiv(u4, 1.0, dx_h), 1.0, dy_h);
    let u12 = muldiv(1.0 + u9, 1.0, u11);
    let u13 = at2(1.0, u12);
    let u15 = ifpos(u13, u13, u13 + C360);
    let u16 = u15 - en_ang;
    let u18 = ifpos(u16, u16, u16 + C360);
    let u21 = ifpos(u18 - CD2, u18 - C360, u18);
    let a_ang = if left { pin(-u21.abs(), -adj[1].abs(), 0.0) } else { pin(0.0, adj[1], u21.abs()) };
    let pt_ang = en_ang + a_ang;
    let a = ellipse_at(b, rw3, rh3, pt_ang);
    let (dx_g, dy_g) = (thh * rad(pt_ang).cos(), thh * rad(pt_ang).sin());
    let (x_g, y_g) = (x_h + dx_g, y_h + dy_g);
    let (x_b, y_b) = (x_h - dx_g, y_h - dy_g);
    let (sx1, sy1, sx2, sy2) = (x_b - hc, y_b - vc, x_g - hc, y_g - vc);
    let r_o = rw1.min(rh1);
    let (x1o, y1o) = (muldiv(sx1, r_o, rw1), muldiv(sy1, r_o, rh1));
    let (x2o, y2o) = (muldiv(sx2, r_o, rw1), muldiv(sy2, r_o, rh1));
    let sdy = ifpos(-(y2o - y1o), -1.0, 1.0);
    let (dx_f, dy_f) = circle_hit(x1o, y1o, x2o, y2o, r_o, sdy, x2o, y2o);
    let (sdx_f, sdy_f) = (muldiv(dx_f, rw1, r_o), muldiv(dy_f, rh1, r_o));
    let f = (hc + sdx_f, vc + sdy_f);
    let (x1i, y1i) = (muldiv(sx1, r_i, rw2), muldiv(sy1, r_i, rh2));
    let (x2i, y2i) = (muldiv(sx2, r_i, rw2), muldiv(sy2, r_i, rh2));
    let (dx_c, dy_c) = circle_hit(x1i, y1i, x2i, y2i, r_i, sdy, x1i, y1i);
    let (sdx_c, sdy_c) = (muldiv(dx_c, rw2, r_i), muldiv(dy_c, rh2, r_i));
    let c = (hc + sdx_c, vc + sdy_c);
    let p5 = (f.0 - c.0).hypot(f.1 - c.1) / 2.0 - thh;
    let gp = if p5 > 0.0 { f } else { (x_g, y_g) };
    let bp = if p5 > 0.0 { c } else { (x_b, y_b) };
    let en0 = at2(sdx_f, sdy_f);
    let ist0 = at2(sdx_c, sdy_c);
    CircArrow {
        st_ang,
        a_ang,
        thh,
        rw1,
        rh1,
        rw2,
        rh2,
        rw3,
        rh3,
        a,
        gp,
        bp,
        f,
        c,
        p5,
        en2: ifpos(en0, en0, en0 + C360),
        ist: ifpos(ist0, ist0, ist0 + C360),
    }
}

fn circular_arrow(b: &ArrowBox, adj: [f64; 5]) -> Vec<ArcCmd> {
    use ArcCmd::{A, L, M, Z};
    let k = circ_arrow(b, adj, false);
    let e = ellipse_at(b, k.rw1, k.rh1, k.st_ang);
    let isw1 = k.st_ang - k.ist;
    let isw_ang = ifpos(isw1, isw1 - C360, isw1);
    let sw0 = k.en2 - k.st_ang;
    let sw_ang = ifpos(sw0, sw0, sw0 + C360);
    vec![
        M(e.0, e.1),
        A(k.rw1, k.rh1, k.st_ang, sw_ang),
        L(k.gp.0, k.gp.1),
        L(k.a.0, k.a.1),
        L(k.bp.0, k.bp.1),
        L(k.c.0, k.c.1),
        A(k.rw2, k.rh2, k.ist, isw_ang),
        Z,
    ]
}

fn left_circular_arrow(b: &ArrowBox, adj: [f64; 5]) -> Vec<ArcCmd> {
    use ArcCmd::{A, L, M, Z};
    let k = circ_arrow(b, adj, true);
    let e = ellipse_at(b, k.rw1, k.rh1, k.st_ang);
    let d = ellipse_at(b, k.rw2, k.rh2, k.st_ang);
    let isw1 = k.st_ang - k.ist;
    let isw_ang0 = ifpos(isw1, isw1, isw1 + C360);
    let sw0 = k.en2 - k.st_ang;
    let sw_ang = ifpos(sw0, sw0 - C360, sw0);
    vec![
        M(e.0, e.1),
        L(d.0, d.1),
        A(k.rw2, k.rh2, k.ist + isw_ang0, -isw_ang0),
        L(k.bp.0, k.bp.1),
        L(k.a.0, k.a.1),
        L(k.gp.0, k.gp.1),
        L(k.f.0, k.f.1),
        A(k.rw1, k.rh1, k.st_ang + sw_ang, -sw_ang),
        Z,
    ]
}

fn left_right_circular_arrow(b: &ArrowBox, adj: [f64; 5]) -> Vec<ArcCmd> {
    use ArcCmd::{A, L, M, Z};
    let k = circ_arrow(b, adj, false);
    let en_ang = pin(1.0, adj[2], 21599999.0);
    let i = ellipse_at(b, k.rw3, k.rh3, k.st_ang);
    let lpt_ang = k.st_ang - k.a_ang;
    let l = ellipse_at(b, k.rw3, k.rh3, lpt_ang);
    let (dx_k, dy_k) = (k.thh * rad(lpt_ang).cos(), k.thh * rad(lpt_ang).sin());
    let od0 = k.en2 - en_ang;
    let od2 = ifpos(od0, od0, od0 + C360);
    let st0 = k.st_ang - od2;
    let st2 = ifpos(st0, st0, st0 + C360);
    let sw0 = k.en2 - st2;
    let sw_ang = ifpos(sw0, sw0, sw0 + C360);
    let id0 = k.ist - en_ang;
    let id2 = ifpos(id0, id0 - C360, id0);
    let ien0 = k.st_ang - id2;
    let ien1 = ien0 - C360;
    let ien2 = ifpos(ien1, ien1, ien0);
    let isw1 = ien2 - k.ist;
    let isw_ang = ifpos(isw1, isw1 - C360, isw1);
    let e = ellipse_at(b, k.rw1, k.rh1, st2);
    let d = ellipse_at(b, k.rw2, k.rh2, ien2);
    let kp = if k.p5 > 0.0 { e } else { (i.0 + dx_k, i.1 + dy_k) };
    let jp = if k.p5 > 0.0 { d } else { (i.0 - dx_k, i.1 - dy_k) };
    vec![
        M(l.0, l.1),
        L(kp.0, kp.1),
        L(e.0, e.1),
        A(k.rw1, k.rh1, st2, sw_ang),
        L(k.gp.0, k.gp.1),
        L(k.a.0, k.a.1),
        L(k.bp.0, k.bp.1),
        L(k.c.0, k.c.1),
        A(k.rw2, k.rh2, k.ist, isw_ang),
        L(jp.0, jp.1),
        Z,
    ]
}

/// A path command of a ribbon or scroll preset, with its points and radii as `f64` guides.
#[derive(Clone)]
enum RibbonCmd {
    M(f64, f64),
    L(f64, f64),
    A(f64, f64, i64, i64),
    Z,
}

/// The ribbons (`ribbon`, `ribbon2`) and the scrolls (`verticalScroll`, `horizontalScroll`): a
/// filled body, shaded folds and an outline, each path with its own fill and stroke.
///
/// The guides are evaluated in `f64`; every point and radius is rounded and pinned into the box at
/// the end.
fn ribbon_or_scroll_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    use RibbonCmd::{A, L, M, Z};
    if !g.paths.is_empty() || w <= 0 || h <= 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (wf, hf) = (w as f64, h as f64);
    let (ss, hc) = (wf.min(hf), wf / 2.0);
    let (wd8, wd32) = (wf / 8.0, wf / 32.0);
    let (cd4, cd2, cd34) = (5_400_000, 10_800_000, 16_200_000);
    let adj = |name: &str, default: i128| adjust(g, name, default) as f64;
    let paths: Vec<(bool, bool, PathShade, Vec<RibbonCmd>)> = match g.preset.as_str() {
        "ribbon" | "ribbon2" => {
            let a1 = pin(0.0, adj("adj1", 16667), 33333.0);
            let a2 = pin(25000.0, adj("adj2", 50000), 75000.0);
            let x10 = wf - wd8;
            let dx2 = wf * a2 / 200000.0;
            let (x2, x9) = (hc - dx2, hc + dx2);
            let (x3, x8) = (x2 + wd32, x9 - wd32);
            let (x5, x6) = (x2 + wd8, x9 - wd8);
            let (x4, x7) = (x5 - wd32, x6 + wd32);
            let h_r = hf * a1 / 400000.0;
            let arc = |st_ang: i64, sw_ang: i64| A(wd32, h_r, st_ang, sw_ang);
            if g.preset == "ribbon" {
                let y1 = hf * a1 / 200000.0;
                let y2 = hf * a1 / 100000.0;
                let y4 = hf - y2;
                let y3 = y4 / 2.0;
                let y5 = hf - h_r;
                let y6 = y2 - h_r;
                let body = vec![
                    M(0.0, 0.0),
                    L(x4, 0.0),
                    arc(cd34, cd2),
                    L(x3, y1),
                    arc(cd34, -cd2),
                    L(x8, y2),
                    arc(cd4, -cd2),
                    L(x7, y1),
                    arc(cd4, cd2),
                    L(wf, 0.0),
                    L(x10, y3),
                    L(wf, y4),
                    L(x9, y4),
                    L(x9, y5),
                    arc(0, cd4),
                    L(x3, hf),
                    arc(cd4, cd4),
                    L(x2, y4),
                    L(0.0, y4),
                    L(wd8, y3),
                    Z,
                ];
                let folds = vec![
                    M(x5, h_r),
                    arc(0, cd4),
                    L(x3, y1),
                    arc(cd34, -cd2),
                    L(x5, y2),
                    Z,
                    M(x6, h_r),
                    arc(cd2, -cd4),
                    L(x8, y1),
                    arc(cd34, cd2),
                    L(x6, y2),
                    Z,
                ];
                let mut outline = body.clone();
                outline.extend([M(x5, h_r), L(x5, y2), M(x6, y2), L(x6, h_r), M(x2, y4), L(x2, y6), M(x9, y6), L(x9, y4)]);
                vec![(true, false, PathShade::Norm, body), (true, false, PathShade::DarkenLess, folds), (false, true, PathShade::Norm, outline)]
            } else {
                let dy1 = hf * a1 / 200000.0;
                let y1 = hf - dy1;
                let dy2 = hf * a1 / 100000.0;
                let y2 = hf - dy2;
                let y4 = dy2;
                let y3 = (y4 + hf) / 2.0;
                let y6 = hf - h_r;
                let y7 = y1 - h_r;
                let body = vec![
                    M(0.0, hf),
                    L(x4, hf),
                    arc(cd4, -cd2),
                    L(x3, y1),
                    arc(cd4, cd2),
                    L(x8, y2),
                    arc(cd34, cd2),
                    L(x7, y1),
                    arc(cd34, -cd2),
                    L(wf, hf),
                    L(x10, y3),
                    L(wf, y4),
                    L(x9, y4),
                    L(x9, h_r),
                    arc(0, -cd4),
                    L(x3, 0.0),
                    arc(cd34, -cd4),
                    L(x2, y4),
                    L(0.0, y4),
                    L(wd8, y3),
                    Z,
                ];
                let folds = vec![
                    M(x5, y6),
                    arc(0, -cd4),
                    L(x3, y1),
                    arc(cd4, cd2),
                    L(x5, y2),
                    Z,
                    M(x6, y6),
                    arc(cd2, cd4),
                    L(x8, y1),
                    arc(cd4, -cd2),
                    L(x6, y2),
                    Z,
                ];
                let mut outline = vec![
                    M(0.0, hf),
                    L(wd8, y3),
                    L(0.0, y4),
                    L(x2, y4),
                    L(x2, h_r),
                    arc(cd2, cd4),
                    L(x8, 0.0),
                    arc(cd34, cd4),
                    L(x9, y4),
                    L(x9, y4),
                    L(wf, y4),
                    L(x10, y3),
                    L(wf, hf),
                    L(x7, hf),
                    arc(cd4, cd2),
                    L(x8, y1),
                    arc(cd4, -cd2),
                    L(x3, y2),
                    arc(cd34, -cd2),
                    L(x4, y1),
                    arc(cd34, cd2),
                    Z,
                ];
                outline.extend([M(x5, y2), L(x5, y6), M(x6, y6), L(x6, y2), M(x2, y7), L(x2, y4), M(x9, y4), L(x9, y7)]);
                vec![(true, false, PathShade::Norm, body), (true, false, PathShade::DarkenLess, folds), (false, true, PathShade::Norm, outline)]
            }
        }
        "verticalScroll" | "horizontalScroll" => {
            let a = pin(0.0, adj("adj", 12500), 25000.0);
            let ch = ss * a / 100000.0;
            let (ch2, ch4) = (ch / 2.0, ch / 4.0);
            let arc = |w_r: f64, h_r: f64, st_ang: i64, sw_ang: i64| A(w_r, h_r, st_ang, sw_ang);
            if g.preset == "verticalScroll" {
                let x3 = ch + ch2;
                let x4 = ch + ch;
                let x6 = wf - ch;
                let x7 = wf - ch2;
                let y3 = hf - ch;
                let y4 = hf - ch2;
                let body = vec![
                    M(ch2, hf),
                    arc(ch2, ch2, cd4, -cd4),
                    L(ch2, y4),
                    arc(ch4, ch4, cd4, -cd2),
                    L(ch, y3),
                    L(ch, ch2),
                    arc(ch2, ch2, cd2, cd4),
                    L(x7, 0.0),
                    arc(ch2, ch2, cd34, cd2),
                    L(x6, ch),
                    L(x6, y4),
                    arc(ch2, ch2, 0, cd4),
                    Z,
                    M(x4, ch2),
                    arc(ch2, ch2, 0, cd4),
                    arc(ch4, ch4, cd4, cd2),
                    Z,
                ];
                let folds = vec![
                    M(x4, ch2),
                    arc(ch2, ch2, 0, cd4),
                    arc(ch4, ch4, cd4, cd2),
                    Z,
                    M(ch, y4),
                    arc(ch2, ch2, 0, cd34),
                    arc(ch4, ch4, cd34, cd2),
                    Z,
                ];
                let outline = vec![
                    M(ch, y3),
                    L(ch, ch2),
                    arc(ch2, ch2, cd2, cd4),
                    L(x7, 0.0),
                    arc(ch2, ch2, cd34, cd2),
                    L(x6, ch),
                    L(x6, y4),
                    arc(ch2, ch2, 0, cd4),
                    L(ch2, hf),
                    arc(ch2, ch2, cd4, cd2),
                    Z,
                    M(x3, 0.0),
                    arc(ch2, ch2, cd34, cd2),
                    arc(ch4, ch4, cd4, cd2),
                    L(x4, ch2),
                    M(x6, ch),
                    L(x3, ch),
                    M(ch2, y3),
                    arc(ch4, ch4, cd34, cd2),
                    L(ch, y4),
                    M(ch2, hf),
                    arc(ch2, ch2, cd4, -cd4),
                    L(ch, y3),
                ];
                vec![(true, false, PathShade::Norm, body), (true, false, PathShade::DarkenLess, folds), (false, true, PathShade::Norm, outline)]
            } else {
                let y3 = ch + ch2;
                let y4 = ch + ch;
                let y6 = hf - ch;
                let y7 = hf - ch2;
                let y5 = y6 - ch2;
                let x3 = wf - ch;
                let x4 = wf - ch2;
                let body = vec![
                    M(wf, ch2),
                    arc(ch2, ch2, 0, cd4),
                    L(x4, ch2),
                    arc(ch4, ch4, 0, cd2),
                    L(x3, ch),
                    L(ch2, ch),
                    arc(ch2, ch2, cd34, -cd4),
                    L(0.0, y7),
                    arc(ch2, ch2, cd2, -cd2),
                    L(ch, y6),
                    L(x4, y6),
                    arc(ch2, ch2, cd4, -cd4),
                    Z,
                    M(ch2, y4),
                    arc(ch2, ch2, cd4, -cd4),
                    arc(ch4, ch4, 0, -cd2),
                    Z,
                ];
                let folds = vec![
                    M(ch2, y4),
                    arc(ch2, ch2, cd4, -cd4),
                    arc(ch4, ch4, 0, -cd2),
                    Z,
                    M(x4, ch),
                    arc(ch2, ch2, cd4, -cd34),
                    arc(ch4, ch4, cd2, -cd2),
                    Z,
                ];
                let outline = vec![
                    M(0.0, y3),
                    arc(ch2, ch2, cd2, cd4),
                    L(x3, ch),
                    L(x3, ch2),
                    arc(ch2, ch2, cd2, cd2),
                    L(wf, y5),
                    arc(ch2, ch2, 0, cd4),
                    L(ch, y6),
                    L(ch, y7),
                    arc(ch2, ch2, 0, cd2),
                    Z,
                    M(x3, ch),
                    L(x4, ch),
                    arc(ch2, ch2, cd4, -cd4),
                    M(x4, ch),
                    L(x4, ch2),
                    arc(ch4, ch4, 0, cd2),
                    M(ch2, y4),
                    L(ch2, y3),
                    arc(ch4, ch4, cd2, cd2),
                    arc(ch2, ch2, 0, cd2),
                    M(ch, y3),
                    L(ch, y6),
                ];
                vec![(true, false, PathShade::Norm, body), (true, false, PathShade::DarkenLess, folds), (false, true, PathShade::Norm, outline)]
            }
        }
        _ => return None,
    };
    // Round, then pin into the box: float error can land a unit outside.
    let fix = |v: f64, max: i64| if v.is_finite() { (v.round() as i64).clamp(0, max) } else { 0 };
    let paths = paths
        .into_iter()
        .map(|(fill, stroke, shade, cmds)| {
            let commands = cmds
                .into_iter()
                .map(|c| match c {
                    M(x, y) => PathCommand::MoveTo(fix(x, w), fix(y, h)),
                    L(x, y) => PathCommand::LineTo(fix(x, w), fix(y, h)),
                    A(w_r, h_r, st_ang, sw_ang) => PathCommand::ArcTo { w_r: fix(w_r, w), h_r: fix(h_r, h), st_ang, sw_ang },
                    Z => PathCommand::Close,
                })
                .collect();
            GeometryPath { w, h, fill, stroke, shade, commands }
        })
        .collect();
    Some(paths)
}

/// Smiley face and the action buttons (blank, back, forward, beginning, end, home): one path per
/// `<path>` of the definition, in order. A body path is filled only, shaded faces (`darken`,
/// `darkenLess`) are filled only, and the outline paths are stroked only.
///
/// Guides are evaluated in `f64`; every point and radius is rounded and pinned into the box.
fn button_paths(g: &ShapeGeometry, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    if !g.paths.is_empty() || w <= 0 || h <= 0 {
        return None;
    }
    let b = ButtonBox { w, h, wf: w as f64, hf: h as f64 };
    let paths = match g.preset.as_str() {
        "smileyFace" => smiley_face(&b, adjust(g, "adj", 4653) as f64),
        "actionButtonBlank" => vec![b.path(true, true, PathShade::Norm, b.rect())],
        "actionButtonHome" => action_button_home(&b),
        preset => action_button_arrows(&b, preset)?,
    };
    Some(paths)
}

/// The shape box for [`button_paths`] (`l` = `t` = 0).
struct ButtonBox {
    w: i64,
    h: i64,
    wf: f64,
    hf: f64,
}

impl ButtonBox {
    fn fix(v: f64, max: i64) -> i64 {
        if v.is_finite() { (v.round() as i64).clamp(0, max) } else { 0 }
    }
    fn pt(&self, x: f64, y: f64) -> (i64, i64) {
        (Self::fix(x, self.w), Self::fix(y, self.h))
    }
    fn m(&self, x: f64, y: f64) -> PathCommand {
        let (x, y) = self.pt(x, y);
        PathCommand::MoveTo(x, y)
    }
    fn ln(&self, x: f64, y: f64) -> PathCommand {
        let (x, y) = self.pt(x, y);
        PathCommand::LineTo(x, y)
    }
    /// A whole ellipse from its leftmost point (`stAng` = cd2, `swAng` = 21600000).
    fn ellipse(&self, w_r: f64, h_r: f64) -> PathCommand {
        PathCommand::ArcTo { w_r: Self::fix(w_r, self.w), h_r: Self::fix(h_r, self.h), st_ang: 10_800_000, sw_ang: 21_600_000 }
    }
    /// A closed polygon through `pts`.
    fn poly(&self, pts: &[(f64, f64)]) -> Vec<PathCommand> {
        let mut commands: Vec<PathCommand> = pts.iter().enumerate().map(|(i, &(x, y))| if i == 0 { self.m(x, y) } else { self.ln(x, y) }).collect();
        commands.push(PathCommand::Close);
        commands
    }
    fn rect(&self) -> Vec<PathCommand> {
        self.poly(&[(0.0, 0.0), (self.wf, 0.0), (self.wf, self.hf), (0.0, self.hf)])
    }
    fn path(&self, fill: bool, stroke: bool, shade: PathShade, commands: Vec<PathCommand>) -> GeometryPath {
        GeometryPath { w: self.w, h: self.h, fill, stroke, shade, commands }
    }
    /// The guides every action button shares: `dx2`, `g9`..`g12`, `g13`.
    fn button_guides(&self) -> ButtonGuides {
        let (hc, vc) = (self.wf / 2.0, self.hf / 2.0);
        let ss = self.wf.min(self.hf);
        let dx2 = ss * 3.0 / 8.0;
        ButtonGuides { hc, vc, g9: vc - dx2, g10: vc + dx2, g11: hc - dx2, g12: hc + dx2, g13: ss * 3.0 / 4.0 }
    }
}

struct ButtonGuides {
    hc: f64,
    vc: f64,
    g9: f64,
    g10: f64,
    g11: f64,
    g12: f64,
    g13: f64,
}

fn smiley_face(b: &ButtonBox, adj: f64) -> Vec<GeometryPath> {
    let (w, h) = (b.wf, b.hf);
    let (hc, vc) = (w / 2.0, h / 2.0);
    let a = pin(-4653.0, adj, 4653.0);
    let (x1, x2, x3, x4) = (w * 4969.0 / 21699.0, w * 6215.0 / 21600.0, w * 13135.0 / 21600.0, w * 16640.0 / 21600.0);
    let (y1, y3) = (h * 7570.0 / 21600.0, h * 16515.0 / 21600.0);
    let dy2 = h * a / 100000.0;
    let (y2, y4) = (y3 - dy2, y3 + dy2);
    let y5 = y4 + h * a / 50000.0;
    let (w_r, h_r) = (w * 1125.0 / 21600.0, h * 1125.0 / 21600.0);
    let face = || vec![b.m(0.0, vc), b.ellipse(hc, vc), PathCommand::Close];
    let eyes = vec![b.m(x2, y1), b.ellipse(w_r, h_r), b.m(x3, y1), b.ellipse(w_r, h_r)];
    let mouth = vec![b.m(x1, y2), PathCommand::QuadBezTo([b.pt(hc, y5), b.pt(x4, y2)])];
    vec![
        b.path(true, false, PathShade::Norm, face()),
        b.path(true, true, PathShade::DarkenLess, eyes),
        b.path(false, true, PathShade::Norm, mouth),
        b.path(false, true, PathShade::Norm, face()),
    ]
}

/// Back/previous, forward/next, beginning and end: the button body with its symbol, the symbol
/// darkened, the symbol's outline and the button's outline.
fn action_button_arrows(b: &ButtonBox, preset: &str) -> Option<Vec<GeometryPath>> {
    let ButtonGuides { vc, g9, g10, g11, g12, g13, .. } = b.button_guides();
    let (faces, outline) = match preset {
        "actionButtonBackPrevious" => {
            let tri = b.poly(&[(g11, vc), (g12, g9), (g12, g10)]);
            (tri.clone(), tri)
        }
        "actionButtonForwardNext" => (b.poly(&[(g12, vc), (g11, g9), (g11, g10)]), b.poly(&[(g12, vc), (g11, g10), (g11, g9)])),
        "actionButtonBeginning" => {
            let (g16, g17) = (g11 + g13 / 8.0, g11 + g13 / 4.0);
            let tri = b.poly(&[(g17, vc), (g12, g9), (g12, g10)]);
            let bar = b.poly(&[(g16, g9), (g11, g9), (g11, g10), (g16, g10)]);
            let bar_outline = b.poly(&[(g16, g9), (g16, g10), (g11, g10), (g11, g9)]);
            ([tri.clone(), bar].concat(), [tri, bar_outline].concat())
        }
        "actionButtonEnd" => {
            let (g16, g17) = (g11 + g13 * 3.0 / 4.0, g11 + g13 * 7.0 / 8.0);
            let bar = b.poly(&[(g17, g9), (g12, g9), (g12, g10), (g17, g10)]);
            let tri = b.poly(&[(g16, vc), (g11, g9), (g11, g10)]);
            let tri_outline = b.poly(&[(g16, vc), (g11, g10), (g11, g9)]);
            ([tri, bar.clone()].concat(), [tri_outline, bar].concat())
        }
        _ => return None,
    };
    Some(vec![
        b.path(true, false, PathShade::Norm, [b.rect(), faces.clone()].concat()),
        b.path(true, false, PathShade::Darken, faces),
        b.path(false, true, PathShade::Norm, outline),
        b.path(false, true, PathShade::Norm, b.rect()),
    ])
}

fn action_button_home(b: &ButtonBox) -> Vec<GeometryPath> {
    let ButtonGuides { hc, vc, g9, g10, g11, g12, g13 } = b.button_guides();
    let s = |n: f64| g13 * n / 16.0;
    let (g24, g25, g26, g27) = (g9 + s(1.0), g9 + s(3.0), g9 + s(5.0), g9 + s(12.0));
    let (g28, g29, g30, g31, g32, g33) = (g11 + s(2.0), g11 + s(7.0), g11 + s(9.0), g11 + s(11.0), g11 + s(13.0), g11 + s(14.0));
    let house =
        b.poly(&[(hc, g9), (g11, vc), (g28, vc), (g28, g10), (g33, g10), (g33, vc), (g12, vc), (g32, g26), (g32, g24), (g31, g24), (g31, g25)]);
    let chimney = b.poly(&[(g32, g26), (g32, g24), (g31, g24), (g31, g25)]);
    let walls = b.poly(&[(g28, vc), (g28, g10), (g29, g10), (g29, g27), (g30, g27), (g30, g10), (g33, g10), (g33, vc)]);
    let roof = b.poly(&[(hc, g9), (g11, vc), (g12, vc)]);
    let door = b.poly(&[(g29, g27), (g30, g27), (g30, g10), (g29, g10)]);
    let mut outline =
        b.poly(&[(hc, g9), (g31, g25), (g31, g24), (g32, g24), (g32, g26), (g12, vc), (g33, vc), (g33, g10), (g28, g10), (g28, vc), (g11, vc)]);
    outline.extend([b.m(g31, g25), b.ln(g32, g26), b.m(g33, vc), b.ln(g28, vc), b.m(g29, g10), b.ln(g29, g27), b.ln(g30, g27), b.ln(g30, g10)]);
    vec![
        b.path(true, false, PathShade::Norm, [b.rect(), house].concat()),
        b.path(true, false, PathShade::DarkenLess, [chimney, walls].concat()),
        b.path(true, false, PathShade::Darken, [roof, door].concat()),
        b.path(false, true, PathShade::Norm, outline),
        b.path(false, true, PathShade::Norm, b.rect()),
    ]
}

/// A glyph path of an action button: its fill shade, or `None` for an unfilled outline.
type GlyphFace = (Option<PathShade>, Vec<PathCommand>);

/// The help, information, return, document, sound and movie action buttons: the filled body (box
/// plus glyph, no outline), the glyph's shaded faces, the glyph's outline and the box's outline.
/// Guides are evaluated in `f64` and every point and radius is rounded and pinned into the box.
#[allow(clippy::cast_precision_loss)]
fn action_button_glyph_paths(preset: &str, w: i64, h: i64) -> Option<Vec<GeometryPath>> {
    use PathCommand::{ArcTo, Close, LineTo, MoveTo};
    let (wf, hf) = (w as f64, h as f64);
    let (ss, hc, vc) = (wf.min(hf), wf / 2.0, hf / 2.0);
    let (cd4, cd2, cd34, c360) = (5_400_000, 10_800_000, 16_200_000, 21_600_000);
    let m = |x: f64, y: f64| MoveTo(math_pin(x, w), math_pin(y, h));
    let ln = |x: f64, y: f64| LineTo(math_pin(x, w), math_pin(y, h));
    let arc = |w_r: f64, h_r: f64, st_ang: i64, sw_ang: i64| ArcTo { w_r: math_pin(w_r, w), h_r: math_pin(h_r, h), st_ang, sw_ang };
    let path = |fill: bool, stroke: bool, shade: PathShade, commands: Vec<PathCommand>| GeometryPath { w, h, fill, stroke, shade, commands };
    let rect = vec![m(0.0, 0.0), ln(wf, 0.0), ln(wf, hf), ln(0.0, hf), Close];
    let dx2 = ss * 3.0 / 8.0;
    let (g9, g10, g11, g12) = (vc - dx2, vc + dx2, hc - dx2, hc + dx2);
    let g13 = ss * 3.0 / 4.0;
    let f = |n: f64, d: f64| g13 * n / d;
    // The glyph paths between the body and the frame.
    let (glyph, faces): (Vec<PathCommand>, Vec<GlyphFace>) = match preset {
        "actionButtonHelp" => {
            let (g14, g15, g16) = (f(1.0, 7.0), f(3.0, 14.0), f(2.0, 7.0));
            let (g41, g42) = (f(1.0, 14.0), f(3.0, 28.0));
            let (g27, g29, g30, g31) = (g9 + g16, g9 + f(17.0, 28.0), g9 + f(21.0, 28.0), g9 + f(11.0, 14.0));
            let (g33, g36, g37) = (g11 + g15, g11 + f(3.0, 7.0), g11 + f(4.0, 7.0));
            let glyph = vec![
                m(g33, g27),
                arc(g16, g16, cd2, cd2),
                arc(g14, g15, 0, cd4),
                arc(g41, g42, cd34, -cd4),
                ln(g37, g30),
                ln(g36, g30),
                ln(g36, g29),
                arc(g14, g15, cd2, cd4),
                arc(g41, g42, cd4, -cd4),
                arc(g14, g14, 0, -cd2),
                Close,
                m(hc, g31),
                arc(g42, g42, cd34, c360),
                Close,
            ];
            let faces = vec![(Some(PathShade::Darken), glyph.clone()), (None, glyph.clone())];
            (glyph, faces)
        }
        "actionButtonInformation" => {
            let (g38, g25) = (f(3.0, 32.0), g9 + f(1.0, 32.0));
            let (g28, g29, g30, g31) = (g9 + f(5.0, 16.0), g9 + f(3.0, 8.0), g9 + f(13.0, 16.0), g9 + f(7.0, 8.0));
            let (g32, g34, g35, g37) = (g11 + f(5.0, 16.0), g11 + f(13.0, 32.0), g11 + f(19.0, 32.0), g11 + f(11.0, 16.0));
            let circle = vec![m(hc, g9), arc(dx2, dx2, cd34, c360), Close];
            let dot = vec![m(hc, g25), arc(g38, g38, cd34, c360)];
            let stem_dark = [(g32, g29), (g34, g29), (g34, g30), (g32, g30), (g32, g31), (g37, g31), (g37, g30), (g35, g30), (g35, g28)];
            let stem_light = [(g35, g28), (g35, g30), (g37, g30), (g37, g31), (g32, g31), (g32, g30), (g34, g30), (g34, g29), (g32, g29)];
            let stem = |pts: &[(f64, f64)]| {
                let mut c = vec![m(g32, g28)];
                c.extend(pts.iter().map(|&(x, y)| ln(x, y)));
                c.push(Close);
                c
            };
            let dark = [circle.clone(), dot.clone(), stem(&stem_dark)].concat();
            let light = [dot.clone(), stem(&stem_light)].concat();
            let outline = [circle.clone(), dot, stem(&stem_light)].concat();
            (circle, vec![(Some(PathShade::Darken), dark), (Some(PathShade::Lighten), light), (None, outline)])
        }
        "actionButtonReturn" => {
            let (g17, g27) = (f(3.0, 8.0), f(1.0, 8.0));
            let (g19, g20, g21) = (g9 + f(3.0, 4.0), g9 + f(5.0, 8.0), g9 + f(1.0, 4.0));
            let (g22, g23, g24, g25, g26) = (g11 + f(7.0, 8.0), g11 + f(3.0, 4.0), g11 + f(5.0, 8.0), g11 + g17, g11 + f(1.0, 4.0));
            let glyph = vec![
                m(g12, g21),
                ln(g23, g9),
                ln(hc, g21),
                ln(g24, g21),
                ln(g24, g20),
                arc(g27, g27, 0, cd4),
                ln(g25, g19),
                arc(g27, g27, cd4, cd4),
                ln(g26, g21),
                ln(g11, g21),
                ln(g11, g20),
                arc(g17, g17, cd2, -cd4),
                ln(hc, g10),
                arc(g17, g17, cd4, -cd4),
                ln(g22, g21),
                Close,
            ];
            let outline = vec![
                m(g12, g21),
                ln(g22, g21),
                ln(g22, g20),
                arc(g17, g17, 0, cd4),
                ln(g25, g10),
                arc(g17, g17, cd4, cd4),
                ln(g11, g21),
                ln(g26, g21),
                ln(g26, g20),
                arc(g27, g27, cd2, -cd4),
                ln(hc, g19),
                arc(g27, g27, cd4, -cd4),
                ln(g24, g21),
                ln(hc, g21),
                ln(g23, g9),
                Close,
            ];
            (glyph.clone(), vec![(Some(PathShade::Darken), glyph), (None, outline)])
        }
        "actionButtonDocument" => {
            let dx1 = ss * 9.0 / 32.0;
            let (g11, g12, g13) = (hc - dx1, hc + dx1, ss * 3.0 / 16.0);
            let (g14, g15) = (g12 - g13, g9 + g13);
            let page = vec![m(g11, g9), ln(g14, g9), ln(g12, g15), ln(g12, g10), ln(g11, g10), Close];
            let sheet = vec![m(g11, g9), ln(g14, g9), ln(g14, g15), ln(g12, g15), ln(g12, g10), ln(g11, g10), Close];
            let corner = vec![m(g14, g9), ln(g14, g15), ln(g12, g15), Close];
            let outline = [page.clone(), vec![m(g12, g15), ln(g14, g15), ln(g14, g9)]].concat();
            (page, vec![(Some(PathShade::DarkenLess), sheet), (Some(PathShade::Darken), corner), (None, outline)])
        }
        "actionButtonSound" => {
            let (g20, g21, g22, g23) = (g9 + f(1.0, 8.0), g9 + f(5.0, 16.0), g9 + f(11.0, 16.0), g9 + f(7.0, 8.0));
            let (g24, g25, g26) = (g11 + f(5.0, 16.0), g11 + f(5.0, 8.0), g11 + f(3.0, 4.0));
            let speaker = vec![m(g11, g21), ln(g11, g22), ln(g24, g22), ln(g25, g10), ln(g25, g9), ln(g24, g21), Close];
            let outline = vec![
                m(g11, g21),
                ln(g24, g21),
                ln(g25, g9),
                ln(g25, g10),
                ln(g24, g22),
                ln(g11, g22),
                Close,
                m(g26, g21),
                ln(g12, g20),
                m(g26, vc),
                ln(g12, vc),
                m(g26, g22),
                ln(g12, g23),
            ];
            (speaker.clone(), vec![(Some(PathShade::Darken), speaker), (None, outline)])
        }
        "actionButtonMovie" => {
            let k = |n: f64| f(n, 21600.0);
            let (g31, g32, g33, g34) = (g11 + k(1455.0), g11 + k(1905.0), g11 + k(2325.0), g11 + k(16155.0));
            let (g35, g36, g37, g38) = (g11 + k(17010.0), g11 + k(19335.0), g11 + k(19725.0), g11 + k(20595.0));
            let (g39, g40, g41, g42, g43) = (g9 + k(5280.0), g9 + k(5730.0), g9 + k(6630.0), g9 + k(7492.0), g9 + k(9067.0));
            let (g44, g45, g46, g47) = (g9 + k(9555.0), g9 + k(13342.0), g9 + k(14580.0), g9 + k(15592.0));
            let camera = [
                (g11, g44),
                (g31, g44),
                (g32, g43),
                (g33, g43),
                (g33, g47),
                (g35, g47),
                (g35, g45),
                (g36, g45),
                (g38, g46),
                (g12, g46),
                (g12, g41),
                (g38, g41),
                (g37, g42),
                (g35, g42),
                (g35, g41),
                (g34, g40),
                (g32, g40),
                (g31, g39),
            ];
            let trace = |pts: &mut dyn Iterator<Item = &(f64, f64)>| {
                let mut c = vec![m(g11, g39)];
                c.extend(pts.map(|&(x, y)| ln(x, y)));
                c.push(Close);
                c
            };
            let glyph = trace(&mut camera.iter());
            // The outline runs the other way round, from the first point back to the last.
            let outline = trace(&mut camera.iter().rev());
            (glyph.clone(), vec![(Some(PathShade::Darken), glyph), (None, outline)])
        }
        _ => return None,
    };
    let mut paths = vec![path(true, false, PathShade::Norm, [rect.clone(), glyph].concat())];
    paths.extend(faces.into_iter().map(|(shade, commands)| match shade {
        Some(shade) => path(true, false, shade, commands),
        None => path(false, true, PathShade::Norm, commands),
    }));
    paths.push(path(false, true, PathShade::Norm, rect));
    Some(paths)
}
