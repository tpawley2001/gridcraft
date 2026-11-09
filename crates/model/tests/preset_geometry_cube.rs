//! Cube preset (ECMA-376 Part 1 presetShapeDefinitions `cube`): three faces, each its own path with the
//! fill shade the definition gives it (`a:path/@fill`, ST_PathFillMode §20.1.10.37), then an unfilled
//! outline path with the inner edges. All guides are integer, so coordinates are exact.
use PathCommand::{Close, LineTo as L, MoveTo as M};
use gridcraft_model::{GeometryPath, PathCommand, PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

/// The four cube paths for a `w` × `h` box whose depth guide is `y1` (`ss * adj / 100000`).
fn expected(w: i64, h: i64, y1: i64) -> Vec<(bool, bool, PathShade, Vec<PathCommand>)> {
    let (r, b, y4, x4) = (w, h, h - y1, w - y1);
    vec![
        (true, false, PathShade::Norm, vec![M(0, y1), L(x4, y1), L(x4, b), L(0, b), Close]),
        (true, false, PathShade::DarkenLess, vec![M(x4, y1), L(r, 0), L(r, y4), L(x4, b), Close]),
        (true, false, PathShade::LightenLess, vec![M(0, y1), L(y1, 0), L(r, 0), L(x4, y1), Close]),
        (
            false,
            true,
            PathShade::Norm,
            vec![M(0, y1), L(y1, 0), L(r, 0), L(r, y4), L(x4, b), L(0, b), Close, M(0, y1), L(x4, y1), L(r, 0), M(x4, y1), L(x4, b)],
        ),
    ]
}

fn check(adjust: &[(&str, &str)], w: i64, h: i64, y1: i64) {
    let paths: Vec<GeometryPath> = preset_paths(&geom("cube", adjust), w, h).expect("cube has an outline");
    let exp = expected(w, h, y1);
    assert_eq!(paths.len(), exp.len(), "cube {w}x{h} {adjust:?}: four paths");
    for (i, (p, (fill, stroke, shade, cmds))) in paths.iter().zip(exp).enumerate() {
        assert_eq!((p.w, p.h), (w, h), "path {i} in the shape's own box");
        assert_eq!((p.fill, p.stroke, p.shade), (fill, stroke, shade), "path {i} fill/stroke/shade");
        assert_eq!(p.commands, cmds, "path {i} commands ({w}x{h} {adjust:?})");
    }
}

#[test]
fn cube_default_depth_is_a_quarter_of_the_short_side() {
    // ss = 600, y1 = 600 * 25000 / 100000 = 150
    check(&[], 1000, 600, 150);
}

#[test]
fn cube_adjust_sets_the_depth() {
    // ss = 400, y1 = 400 * 50000 / 100000 = 200
    check(&[("adj", "val 50000")], 400, 800, 200);
    // 33333: y1 = 777 * 33333 / 100000 = 258 (truncated)
    check(&[("adj", "val 33333")], 1001, 777, 258);
}

#[test]
fn cube_adjust_is_pinned_to_0_and_100000() {
    check(&[("adj", "val 150000")], 400, 800, 400);
    check(&[("adj", "val -5000")], 400, 800, 0);
}

#[test]
fn other_presets_keep_the_normal_shade() {
    let paths = preset_paths(&geom("hexagon", &[]), 1000, 600).expect("hexagon has an outline");
    assert!(paths.iter().all(|p| p.shade == PathShade::Norm));
    assert_eq!(GeometryPath::default().shade, PathShade::Norm);
}
