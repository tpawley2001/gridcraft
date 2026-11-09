//! Hexagon, pentagon and 4-point star outlines (ECMA-376 Part 1 presetShapeDefinitions). Polygons are
//! compared up to their starting vertex and direction; each coordinate may differ by one unit from the
//! value computed from the guides (integer guides truncate, trig guides are rounded here).
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
fn hexagon_default_tall_and_adjusted() {
    check("hexagon", &[], 200, 100, &[(0, 50), (25, 0), (175, 0), (200, 50), (175, 100), (25, 100)]);
    check("hexagon", &[], 100, 200, &[(0, 100), (25, 0), (75, 0), (100, 100), (75, 200), (25, 200)]);
    // maxAdj = 50000 * w / ss = 100000 for a 200 x 100 box, so 60000 is kept.
    check("hexagon", &[("adj", "val 60000"), ("vf", "val 115470")], 200, 100, &[(0, 50), (60, 0), (140, 0), (200, 50), (140, 100), (60, 100)]);
}

#[test]
fn pentagon_square_and_wide() {
    check("pentagon", &[], 100, 100, &[(0, 38), (50, 0), (100, 38), (81, 100), (19, 100)]);
    check("pentagon", &[], 200, 100, &[(0, 38), (100, 0), (200, 38), (162, 100), (38, 100)]);
}

#[test]
fn star4_default_and_adjusted() {
    check("star4", &[], 200, 100, &[(0, 50), (82, 41), (100, 0), (118, 41), (200, 50), (118, 59), (100, 100), (82, 59)]);
    check("star4", &[("adj", "val 25000")], 100, 100, &[(0, 50), (32, 32), (50, 0), (68, 32), (100, 50), (68, 68), (50, 100), (32, 68)]);
}

#[test]
fn unknown_presets_still_have_no_outline() {
    assert!(preset_paths(&geom("cloud", &[]), 100, 100).is_none());
}
