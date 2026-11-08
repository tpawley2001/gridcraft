//! DrawingML preset geometries (`a:prstGeom`, ECMA-376 Part 1 §20.1.9.18) as outline paths, so a
//! preset kept in [`ShapeGeometry`] can be drawn and printed like a freeform.
use super::{GeometryPath, PathCommand, ShapeGeometry};

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
    // `x * y / z`, truncating; `z` is never zero here (w, h, ss > 0 and constants).
    let md = |x: i128, y: i128, z: i128| x.saturating_mul(y).checked_div(z).unwrap_or(0);
    let adj = |name: &str, default: i128, max: i128| adjust(g, name, default).clamp(0, max.max(0));
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
        _ => return None,
    };
    // Every vertex lies in 0..=w × 0..=h because the adjusts are pinned, so this never clamps.
    let to_i64 = |v: i128| i64::try_from(v).unwrap_or(if v < 0 { 0 } else { i64::MAX });
    let mut commands = Vec::with_capacity(pts.len() + 1);
    for (i, &(x, y)) in pts.iter().enumerate() {
        let (x, y) = (to_i64(x), to_i64(y));
        commands.push(if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) });
    }
    commands.push(PathCommand::Close);
    Some(vec![GeometryPath { w, h, fill: true, stroke: true, commands }])
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
