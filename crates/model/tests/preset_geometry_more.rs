//! More preset shape outlines (ECMA-376 Part 1 presetShapeDefinitions): octagon, plus, triangle,
//! right triangle and notched right arrow. Polygons are compared up to their starting vertex and
//! direction; expected values follow the presets' integer guides.
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

fn same_cycle(actual: &[(i64, i64)], expected: &[(i64, i64)]) -> bool {
    let n = expected.len();
    if actual.len() != n {
        return false;
    }
    let rev: Vec<_> = expected.iter().rev().copied().collect();
    [expected.to_vec(), rev].iter().any(|e| (0..n).any(|k| (0..n).all(|i| actual[(i + k) % n] == e[i])))
}

fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[(i64, i64)]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke), (w, h, true, true), "{preset} path box and flags");
    let got = polygon(&paths);
    assert!(same_cycle(&got, expected), "{preset} {adjust:?} {w}x{h}: got {got:?}, expected {expected:?}");
}

#[test]
fn octagon_default_tall_and_adjusted() {
    check("octagon", &[], 200, 100, &[(0, 29), (29, 0), (171, 0), (200, 29), (200, 71), (171, 100), (29, 100), (0, 71)]);
    check("octagon", &[], 100, 200, &[(0, 29), (29, 0), (71, 0), (100, 29), (100, 171), (71, 200), (29, 200), (0, 171)]);
    check("octagon", &[("adj", "val 10000")], 200, 100, &[(0, 10), (10, 0), (190, 0), (200, 10), (200, 90), (190, 100), (10, 100), (0, 90)]);
}

#[test]
fn plus_default_and_pinned() {
    check(
        "plus",
        &[],
        200,
        100,
        &[(0, 25), (25, 25), (25, 0), (175, 0), (175, 25), (200, 25), (200, 75), (175, 75), (175, 100), (25, 100), (25, 75), (0, 75)],
    );
    // 60000 is pinned to 50000: the arms meet in the middle.
    check(
        "plus",
        &[("adj", "val 60000")],
        200,
        100,
        &[(0, 50), (50, 50), (50, 0), (150, 0), (150, 50), (200, 50), (200, 50), (150, 50), (150, 100), (50, 100), (50, 50), (0, 50)],
    );
}

#[test]
fn triangles() {
    check("triangle", &[], 200, 100, &[(0, 100), (100, 0), (200, 100)]);
    check("triangle", &[("adj", "val 25000")], 200, 100, &[(0, 100), (50, 0), (200, 100)]);
    check("triangle", &[("adj", "val 0")], 200, 100, &[(0, 100), (0, 0), (200, 100)]);
    check("rtTriangle", &[], 200, 100, &[(0, 0), (200, 100), (0, 100)]);
}

#[test]
fn notched_right_arrow_default_and_adjusted() {
    check("notchedRightArrow", &[], 200, 100, &[(0, 25), (150, 25), (150, 0), (200, 50), (150, 100), (150, 75), (0, 75), (25, 50)]);
    check(
        "notchedRightArrow",
        &[("adj1", "val 30000"), ("adj2", "val 80000")],
        200,
        100,
        &[(0, 35), (120, 35), (120, 0), (200, 50), (120, 100), (120, 65), (0, 65), (24, 50)],
    );
}

#[test]
fn one_unit_high_box_does_not_panic() {
    assert!(preset_paths(&geom("notchedRightArrow", &[]), 200, 1).is_some());
    assert!(preset_paths(&geom("plus", &[]), 1, 1).is_some());
}

#[test]
fn unsupported_curved_presets_still_have_no_outline() {
    assert!(preset_paths(&geom("notAPreset", &[]), 200, 100).is_none());
    assert!(preset_paths(&geom("ellipse", &[]), 200, 100).is_none());
}
