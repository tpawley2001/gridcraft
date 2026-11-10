//! Straight-edged flowchart presets (ECMA-376 Part 1 presetShapeDefinitions `flowChart*`).
//! Each is a fixed path in its own small path box, scaled to the shape box (`w * x / path_w`, truncating).
//! Polygons are compared up to their starting vertex and direction; each coordinate may differ by one unit.
use gridcraft_model::{GeometryPath, PathCommand, ShapeGeometry, preset_paths};

fn geom(preset: &str) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] }
}

fn points(p: &GeometryPath) -> Vec<(i64, i64)> {
    p.commands
        .iter()
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

fn check(preset: &str, w: i64, h: i64, expected: &[(i64, i64)]) {
    let paths = preset_paths(&geom(preset), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), 1, "{preset}: one path");
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke), (w, h, true, true), "{preset} path box and flags");
    assert_eq!(p.commands.last(), Some(&PathCommand::Close), "{preset} ends with Close");
    assert!(matches!(p.commands.first(), Some(PathCommand::MoveTo(..))), "{preset} starts with MoveTo");
    let got = points(p);
    assert!(same_cycle(&got, expected), "{preset} {w}x{h}: got {got:?}, expected {expected:?}");
}

#[test]
fn single_polygon_flowchart_presets_scale_their_fixed_outline() {
    check("flowChartDecision", 1000, 600, &[(0, 300), (500, 0), (1000, 300), (500, 600)]);
    check("flowChartInputOutput", 1000, 600, &[(0, 600), (200, 0), (1000, 0), (800, 600)]);
    check("flowChartPreparation", 1000, 600, &[(0, 300), (200, 0), (800, 0), (1000, 300), (800, 600), (200, 600)]);
    check("flowChartManualInput", 1000, 600, &[(0, 120), (1000, 0), (1000, 600), (0, 600)]);
    check("flowChartManualOperation", 1000, 600, &[(0, 0), (1000, 0), (800, 600), (200, 600)]);
    check("flowChartMerge", 1000, 600, &[(0, 0), (1000, 0), (500, 600)]);
    check("flowChartExtract", 1000, 600, &[(0, 600), (500, 0), (1000, 600)]);
    check("flowChartPunchedCard", 1000, 600, &[(0, 120), (200, 0), (1000, 0), (1000, 600), (0, 600)]);
    check("flowChartOffpageConnector", 1000, 600, &[(0, 0), (1000, 0), (1000, 480), (500, 600), (0, 480)]);
    check("flowChartDecision", 777, 333, &[(0, 166), (388, 0), (777, 166), (388, 333)]);
    check("flowChartInputOutput", 777, 333, &[(0, 333), (155, 0), (777, 0), (621, 333)]);
    check("flowChartPreparation", 777, 333, &[(0, 166), (155, 0), (621, 0), (777, 166), (621, 333), (155, 333)]);
    check("flowChartManualInput", 777, 333, &[(0, 66), (777, 0), (777, 333), (0, 333)]);
    check("flowChartManualOperation", 777, 333, &[(0, 0), (777, 0), (621, 333), (155, 333)]);
    check("flowChartMerge", 777, 333, &[(0, 0), (777, 0), (388, 333)]);
    check("flowChartExtract", 777, 333, &[(0, 333), (388, 0), (777, 333)]);
    check("flowChartPunchedCard", 777, 333, &[(0, 66), (155, 0), (777, 0), (777, 333), (0, 333)]);
    check("flowChartOffpageConnector", 777, 333, &[(0, 0), (777, 0), (777, 266), (388, 333), (0, 266)]);
}

/// Fill rectangle, two open inner lines, outline rectangle.
fn check_three(preset: &str, w: i64, h: i64, lines: [(i64, i64); 4]) {
    let paths = preset_paths(&geom(preset), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), 3, "{preset}: fill, inner lines, outline");
    for p in &paths {
        assert_eq!((p.w, p.h), (w, h), "{preset} path box");
    }
    let rect = [(0, 0), (w, 0), (w, h), (0, h)];
    assert_eq!((paths[0].fill, paths[0].stroke), (true, false), "{preset} fill path flags");
    assert_eq!(paths[0].commands.last(), Some(&PathCommand::Close), "{preset} fill path closes");
    assert!(same_cycle(&points(&paths[0]), &rect), "{preset} fill path: {:?}", paths[0].commands);

    assert_eq!((paths[1].fill, paths[1].stroke), (false, true), "{preset} line path flags");
    let want = [
        PathCommand::MoveTo(lines[0].0, lines[0].1),
        PathCommand::LineTo(lines[1].0, lines[1].1),
        PathCommand::MoveTo(lines[2].0, lines[2].1),
        PathCommand::LineTo(lines[3].0, lines[3].1),
    ];
    assert_eq!(paths[1].commands, want, "{preset} inner lines are two open segments");

    assert_eq!((paths[2].fill, paths[2].stroke), (false, true), "{preset} outline path flags");
    assert_eq!(paths[2].commands.last(), Some(&PathCommand::Close), "{preset} outline closes");
    assert!(same_cycle(&points(&paths[2]), &rect), "{preset} outline: {:?}", paths[2].commands);
}

#[test]
fn predefined_process_has_two_inner_verticals() {
    check_three("flowChartPredefinedProcess", 1000, 600, [(125, 0), (125, 600), (875, 0), (875, 600)]);
    check_three("flowChartPredefinedProcess", 777, 333, [(97, 0), (97, 333), (679, 0), (679, 333)]);
}

#[test]
fn internal_storage_has_an_inner_vertical_and_horizontal() {
    check_three("flowChartInternalStorage", 1000, 600, [(125, 0), (125, 600), (0, 75), (1000, 75)]);
    check_three("flowChartInternalStorage", 777, 333, [(97, 0), (97, 333), (0, 41), (777, 41)]);
}

#[test]
fn curved_flowchart_presets_are_not_claimed() {
    // Not drawn yet: these still fall back to the rectangle. Terminator, alternate process, delay
    // and document are covered by `preset_geometry_flowchart_curved.rs`.
    for preset in ["flowChartMagneticDisk", "flowChartMultidocument", "flowChartOnlineStorage", "flowChartDisplay"] {
        assert!(preset_paths(&geom(preset), 1000, 600).is_none(), "{preset}");
    }
}
