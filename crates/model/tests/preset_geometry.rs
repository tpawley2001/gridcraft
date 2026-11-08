//! Preset shape outlines (ECMA-376 Part 1 presetShapeDefinitions) for the presets drawn like
//! freeforms. Polygons are compared up to their starting vertex and direction.
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
fn block_arrows_default_adjusts() {
    check("rightArrow", &[], 200, 100, &[(0, 25), (150, 25), (150, 0), (200, 50), (150, 100), (150, 75), (0, 75)]);
    check("leftArrow", &[], 200, 100, &[(0, 50), (50, 0), (50, 25), (200, 25), (200, 75), (50, 75), (50, 100)]);
    check("upArrow", &[], 200, 100, &[(0, 50), (100, 0), (200, 50), (150, 50), (150, 100), (50, 100), (50, 50)]);
    check("downArrow", &[], 200, 100, &[(50, 0), (150, 0), (150, 50), (200, 50), (100, 100), (0, 50), (50, 50)]);
    check("leftRightArrow", &[], 200, 100, &[(0, 50), (50, 0), (50, 25), (150, 25), (150, 0), (200, 50), (150, 100), (150, 75), (50, 75), (50, 100)]);
}

#[test]
fn chevron_pentagon_and_quadrilaterals_default_adjusts() {
    check("chevron", &[], 200, 100, &[(0, 0), (150, 0), (200, 50), (150, 100), (0, 100), (50, 50)]);
    check("homePlate", &[], 200, 100, &[(0, 0), (150, 0), (200, 50), (150, 100), (0, 100)]);
    check("diamond", &[], 200, 100, &[(0, 50), (100, 0), (200, 50), (100, 100)]);
    check("parallelogram", &[], 200, 100, &[(0, 100), (25, 0), (200, 0), (175, 100)]);
    check("trapezoid", &[], 200, 100, &[(0, 100), (25, 0), (175, 0), (200, 100)]);
}

#[test]
fn adjust_values_override_the_defaults() {
    check(
        "rightArrow",
        &[("adj1", "val 30000"), ("adj2", "val 80000")],
        200,
        100,
        &[(0, 35), (120, 35), (120, 0), (200, 50), (120, 100), (120, 65), (0, 65)],
    );
    check("chevron", &[("adj", "val 20000")], 200, 100, &[(0, 0), (180, 0), (200, 50), (180, 100), (0, 100), (20, 50)]);
    check("parallelogram", &[("adj", "val 50000")], 200, 100, &[(0, 100), (50, 0), (200, 0), (150, 100)]);
}

#[test]
fn adjusts_are_pinned_to_their_maximum() {
    // trapezoid: max = 50000 * w / ss = 100000 on 200 x 100, so 150000 acts as 100000: x2 = 100.
    check("trapezoid", &[("adj", "val 150000")], 200, 100, &[(0, 100), (100, 0), (100, 0), (200, 100)]);
    // rightArrow adj1 above 100000 acts as 100000: the shaft fills the height.
    check("rightArrow", &[("adj1", "val 250000")], 200, 100, &[(0, 0), (150, 0), (150, 0), (200, 50), (150, 100), (150, 100), (0, 100)]);
}

#[test]
fn tall_box_uses_the_short_side() {
    // ss = 100: the head is 50 deep, the shaft half the width.
    check("downArrow", &[], 100, 200, &[(25, 0), (75, 0), (75, 150), (100, 150), (50, 200), (0, 150), (25, 150)]);
    check("chevron", &[], 100, 200, &[(0, 0), (50, 0), (100, 100), (50, 200), (0, 200), (50, 100)]);
}

#[test]
fn unparsable_adjust_uses_the_default() {
    check("homePlate", &[("adj", "pin 0 adj 100000")], 200, 100, &[(0, 0), (150, 0), (200, 50), (150, 100), (0, 100)]);
}

#[test]
fn unknown_presets_custom_geometry_and_empty_boxes_have_no_outline() {
    assert!(preset_paths(&geom("star5", &[]), 200, 100).is_none());
    assert!(preset_paths(&geom("", &[]), 200, 100).is_none());
    assert!(preset_paths(&geom("rightArrow", &[]), 0, 100).is_none());
    let mut custom = geom("rightArrow", &[]);
    custom.paths.push(GeometryPath { w: 10, h: 10, fill: true, stroke: true, commands: vec![PathCommand::MoveTo(0, 0), PathCommand::Close] });
    assert!(preset_paths(&custom, 200, 100).is_none());
}
