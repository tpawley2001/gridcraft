//! 5-point star and single-snipped-corner rectangle outlines (ECMA-376 Part 1 presetShapeDefinitions). Polygons are
//! compared up to their starting vertex and direction; each coordinate may differ by one unit from the
//! value computed from the guides (integer guides truncate, trig guides truncate toward zero).
use gridcraft_model::{GeometryPath, PathCommand, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

/// The one closed path's vertices.
fn polygon(paths: &[GeometryPath]) -> Vec<(i64, i64)> {
    assert_eq!(paths.len(), 1, "one path");
    let cmds = &paths[0].commands;
    assert_eq!(cmds.last(), Some(&PathCommand::Close), "path ends with Close");
    assert!(matches!(cmds.first(), Some(PathCommand::MoveTo(..))), "path starts with MoveTo");
    cmds.iter()
        .filter_map(|c| match *c {
            PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((x, y)),
            PathCommand::Close => None,
            ref other => panic!("unexpected command {other:?}"),
        })
        .collect()
}

fn near(a: (i64, i64), b: (i64, i64)) -> bool {
    (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1
}

fn same_cycle(actual: &[(i64, i64)], expected: &[(i64, i64)]) -> bool {
    let n = expected.len();
    if actual.len() != n {
        return false;
    }
    let rev: Vec<_> = expected.iter().rev().copied().collect();
    [expected.to_vec(), rev].iter().any(|e| (0..n).any(|k| (0..n).all(|i| near(actual[(i + k) % n], e[i]))))
}

fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[(i64, i64)]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke), (w, h, true, true), "{preset} path box and flags");
    let got = polygon(&paths);
    assert!(got.iter().all(|&(x, y)| (0..=w).contains(&x) && (0..=h).contains(&y)), "{preset} stays in its box: {got:?}");
    assert!(same_cycle(&got, expected), "{preset} {adjust:?} {w}x{h}: got {got:?}, expected {expected:?}");
}


#[test]
fn star5_default_wide_and_adjusted() {
    check("star5", &[], 100, 100, &[(1, 39), (39, 39), (50, 0), (61, 39), (99, 39), (68, 61), (80, 99), (50, 76), (20, 99), (32, 61)]);
    check("star5", &[], 200, 100, &[(1, 39), (77, 39), (100, 0), (123, 39), (199, 39), (138, 61), (161, 99), (100, 76), (39, 99), (62, 61)]);
    check("star5", &[("adj", "val 40000")], 100, 100, &[(1, 39), (26, 20), (50, 0), (74, 20), (99, 39), (88, 68), (80, 99), (50, 99), (20, 99), (12, 68)]);
}

#[test]
fn snip1rect_default_tall_and_pinned() {
    check("snip1Rect", &[], 200, 100, &[(0, 0), (184, 0), (200, 16), (200, 100), (0, 100)]);
    check("snip1Rect", &[("adj", "val 30000")], 100, 200, &[(0, 0), (70, 0), (100, 30), (100, 200), (0, 200)]);
    // adj is pinned to 50000: the snip never passes the middle of the short side.
    check("snip1Rect", &[("adj", "val 90000")], 100, 100, &[(0, 0), (50, 0), (100, 50), (100, 100), (0, 100)]);
}

#[test]
fn unknown_presets_still_have_no_outline() {
    assert!(preset_paths(&geom("cloud", &[]), 100, 100).is_none());
}
