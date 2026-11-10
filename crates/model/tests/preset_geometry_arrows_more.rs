//! Arrow presets (ECMA-376 Part 1 presetShapeDefinitions `upDownArrow`, `quadArrow`, `leftRightUpArrow`, `leftUpArrow`,
//! `bentUpArrow`, `stripedRightArrow`). Expected vertices were computed from the preset guides in floating point, rounded,
//! then pinned into the shape box. Each closed sub-path (MoveTo .. Close) is compared up to its starting vertex and direction;
//! each coordinate may differ by two units (integer guides truncate). Sub-paths are compared in the order the preset lists them.
use gridcraft_model::{GeometryPath, PathCommand, PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

/// The one path's closed sub-paths, as vertex lists.
fn polygons(paths: &[GeometryPath]) -> Vec<Vec<(i64, i64)>> {
    assert_eq!(paths.len(), 1, "one path");
    let mut out: Vec<Vec<(i64, i64)>> = Vec::new();
    let mut open = false;
    for c in &paths[0].commands {
        match *c {
            PathCommand::MoveTo(x, y) => {
                assert!(!open, "sub-path closed before the next MoveTo");
                out.push(vec![(x, y)]);
                open = true;
            }
            PathCommand::LineTo(x, y) => {
                assert!(open, "LineTo inside a sub-path");
                out.last_mut().expect("sub-path").push((x, y));
            }
            PathCommand::Close => {
                assert!(open, "Close ends a sub-path");
                open = false;
            }
            ref other => panic!("unexpected command {other:?}"),
        }
    }
    assert!(!open, "last sub-path is closed");
    out
}

fn near(a: (i64, i64), b: (i64, i64)) -> bool {
    (a.0 - b.0).abs() <= 2 && (a.1 - b.1).abs() <= 2
}

fn same_cycle(actual: &[(i64, i64)], expected: &[(i64, i64)]) -> bool {
    let n = expected.len();
    if actual.len() != n {
        return false;
    }
    let rev: Vec<_> = expected.iter().rev().copied().collect();
    [expected.to_vec(), rev].iter().any(|e| (0..n).any(|k| (0..n).all(|i| near(actual[(i + k) % n], e[i]))))
}

fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[&[(i64, i64)]]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    let p = &paths[0];
    assert!(p.fill && p.stroke, "{preset}: filled and stroked");
    assert_eq!(p.shade, PathShade::Norm, "{preset}: plain fill");
    assert_eq!((p.w, p.h), (w, h), "{preset}: path box is the shape box");
    let actual = polygons(&paths);
    assert_eq!(actual.len(), expected.len(), "{preset} {adjust:?} in {w}x{h}: sub-path count, actual {actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(same_cycle(a, e), "{preset} {adjust:?} in {w}x{h}:\n actual   {a:?}\n expected {e:?}");
    }
}

const ARROWS: [&str; 6] = ["upDownArrow", "quadArrow", "leftRightUpArrow", "leftUpArrow", "bentUpArrow", "stripedRightArrow"];

#[test]
fn up_down_arrow_outline() {
    check(
        "upDownArrow",
        &[],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (500000, 0),
            (1000000, 300000),
            (750000, 300000),
            (750000, 300000),
            (1000000, 300000),
            (500000, 600000),
            (0, 300000),
            (250000, 300000),
            (250000, 300000),
        ]],
    );
    check(
        "upDownArrow",
        &[],
        300000,
        900000,
        &[&[
            (0, 150000),
            (150000, 0),
            (300000, 150000),
            (225000, 150000),
            (225000, 750000),
            (300000, 750000),
            (150000, 900000),
            (0, 750000),
            (75000, 750000),
            (75000, 150000),
        ]],
    );
    check(
        "upDownArrow",
        &[("adj1", "val 30000"), ("adj2", "val 30000")],
        1000000,
        600000,
        &[&[
            (0, 180000),
            (500000, 0),
            (1000000, 180000),
            (650000, 180000),
            (650000, 420000),
            (1000000, 420000),
            (500000, 600000),
            (0, 420000),
            (350000, 420000),
            (350000, 180000),
        ]],
    );
}

#[test]
fn quad_arrow_outline() {
    check(
        "quadArrow",
        &[],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (135000, 165000),
            (135000, 232500),
            (432500, 232500),
            (432500, 135000),
            (365000, 135000),
            (500000, 0),
            (635000, 135000),
            (567500, 135000),
            (567500, 232500),
            (865000, 232500),
            (865000, 165000),
            (1000000, 300000),
            (865000, 435000),
            (865000, 367500),
            (567500, 367500),
            (567500, 465000),
            (635000, 465000),
            (500000, 600000),
            (365000, 465000),
            (432500, 465000),
            (432500, 367500),
            (135000, 367500),
            (135000, 435000),
        ]],
    );
    check(
        "quadArrow",
        &[],
        300000,
        900000,
        &[&[
            (0, 450000),
            (67500, 382500),
            (67500, 416250),
            (116250, 416250),
            (116250, 67500),
            (82500, 67500),
            (150000, 0),
            (217500, 67500),
            (183750, 67500),
            (183750, 416250),
            (232500, 416250),
            (232500, 382500),
            (300000, 450000),
            (232500, 517500),
            (232500, 483750),
            (183750, 483750),
            (183750, 832500),
            (217500, 832500),
            (150000, 900000),
            (82500, 832500),
            (116250, 832500),
            (116250, 483750),
            (67500, 483750),
            (67500, 517500),
        ]],
    );
    check(
        "quadArrow",
        &[("adj1", "val 13500"), ("adj2", "val 13500"), ("adj3", "val 13500")],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (81000, 219000),
            (81000, 259500),
            (459500, 259500),
            (459500, 81000),
            (419000, 81000),
            (500000, 0),
            (581000, 81000),
            (540500, 81000),
            (540500, 259500),
            (919000, 259500),
            (919000, 219000),
            (1000000, 300000),
            (919000, 381000),
            (919000, 340500),
            (540500, 340500),
            (540500, 519000),
            (581000, 519000),
            (500000, 600000),
            (419000, 519000),
            (459500, 519000),
            (459500, 340500),
            (81000, 340500),
            (81000, 381000),
        ]],
    );
}

#[test]
fn left_right_up_arrow_outline() {
    check(
        "leftRightUpArrow",
        &[],
        1000000,
        600000,
        &[&[
            (0, 450000),
            (150000, 300000),
            (150000, 375000),
            (425000, 375000),
            (425000, 150000),
            (350000, 150000),
            (500000, 0),
            (650000, 150000),
            (575000, 150000),
            (575000, 375000),
            (850000, 375000),
            (850000, 300000),
            (1000000, 450000),
            (850000, 600000),
            (850000, 525000),
            (150000, 525000),
            (150000, 600000),
        ]],
    );
    check(
        "leftRightUpArrow",
        &[],
        300000,
        900000,
        &[&[
            (0, 825000),
            (75000, 750000),
            (75000, 787500),
            (112500, 787500),
            (112500, 75000),
            (75000, 75000),
            (150000, 0),
            (225000, 75000),
            (187500, 75000),
            (187500, 787500),
            (225000, 787500),
            (225000, 750000),
            (300000, 825000),
            (225000, 900000),
            (225000, 862500),
            (75000, 862500),
            (75000, 900000),
        ]],
    );
    check(
        "leftRightUpArrow",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000")],
        1000000,
        600000,
        &[&[
            (0, 510000),
            (90000, 420000),
            (90000, 465000),
            (455000, 465000),
            (455000, 90000),
            (410000, 90000),
            (500000, 0),
            (590000, 90000),
            (545000, 90000),
            (545000, 465000),
            (910000, 465000),
            (910000, 420000),
            (1000000, 510000),
            (910000, 600000),
            (910000, 555000),
            (90000, 555000),
            (90000, 600000),
        ]],
    );
}

#[test]
fn left_up_arrow_outline() {
    check(
        "leftUpArrow",
        &[],
        1000000,
        600000,
        &[&[
            (0, 450000),
            (150000, 300000),
            (150000, 375000),
            (775000, 375000),
            (775000, 150000),
            (700000, 150000),
            (850000, 0),
            (1000000, 150000),
            (925000, 150000),
            (925000, 525000),
            (150000, 525000),
            (150000, 600000),
        ]],
    );
    check(
        "leftUpArrow",
        &[],
        300000,
        900000,
        &[&[
            (0, 825000),
            (75000, 750000),
            (75000, 787500),
            (187500, 787500),
            (187500, 75000),
            (150000, 75000),
            (225000, 0),
            (300000, 75000),
            (262500, 75000),
            (262500, 862500),
            (75000, 862500),
            (75000, 900000),
        ]],
    );
    check(
        "leftUpArrow",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000")],
        1000000,
        600000,
        &[&[
            (0, 510000),
            (90000, 420000),
            (90000, 465000),
            (865000, 465000),
            (865000, 90000),
            (820000, 90000),
            (910000, 0),
            (1000000, 90000),
            (955000, 90000),
            (955000, 555000),
            (90000, 555000),
            (90000, 600000),
        ]],
    );
}

#[test]
fn bent_up_arrow_outline() {
    check(
        "bentUpArrow",
        &[],
        1000000,
        600000,
        &[&[
            (0, 450000),
            (775000, 450000),
            (775000, 150000),
            (700000, 150000),
            (850000, 0),
            (1000000, 150000),
            (925000, 150000),
            (925000, 600000),
            (0, 600000),
        ]],
    );
    check(
        "bentUpArrow",
        &[],
        300000,
        900000,
        &[&[
            (0, 825000),
            (187500, 825000),
            (187500, 75000),
            (150000, 75000),
            (225000, 0),
            (300000, 75000),
            (262500, 75000),
            (262500, 900000),
            (0, 900000),
        ]],
    );
    check(
        "bentUpArrow",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000")],
        1000000,
        600000,
        &[&[
            (0, 510000),
            (865000, 510000),
            (865000, 90000),
            (820000, 90000),
            (910000, 0),
            (1000000, 90000),
            (955000, 90000),
            (955000, 600000),
            (0, 600000),
        ]],
    );
}

#[test]
fn striped_right_arrow_outline() {
    check(
        "stripedRightArrow",
        &[],
        1000000,
        600000,
        &[
            &[(0, 150000), (18750, 150000), (18750, 450000), (0, 450000)],
            &[(37500, 150000), (75000, 150000), (75000, 450000), (37500, 450000)],
            &[(93750, 150000), (700000, 150000), (700000, 0), (1000000, 300000), (700000, 600000), (700000, 450000), (93750, 450000)],
        ],
    );
    check(
        "stripedRightArrow",
        &[],
        300000,
        900000,
        &[
            &[(0, 225000), (9375, 225000), (9375, 675000), (0, 675000)],
            &[(18750, 225000), (37500, 225000), (37500, 675000), (18750, 675000)],
            &[(46875, 225000), (150000, 225000), (150000, 0), (300000, 450000), (150000, 900000), (150000, 675000), (46875, 675000)],
        ],
    );
    check(
        "stripedRightArrow",
        &[("adj1", "val 30000"), ("adj2", "val 30000")],
        1000000,
        600000,
        &[
            &[(0, 210000), (18750, 210000), (18750, 390000), (0, 390000)],
            &[(37500, 210000), (75000, 210000), (75000, 390000), (37500, 390000)],
            &[(93750, 210000), (820000, 210000), (820000, 0), (1000000, 300000), (820000, 600000), (820000, 390000), (93750, 390000)],
        ],
    );
}

#[test]
fn arrows_vertices_stay_in_hostile_boxes() {
    for preset in ARROWS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            for (x, y) in polygons(&paths).into_iter().flatten() {
                assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} in {w}x{h}: ({x}, {y}) outside the box");
            }
        }
    }
}

#[test]
fn arrows_hostile_adjustments_do_not_panic() {
    for preset in ARROWS {
        for name in ["adj", "adj1", "adj2", "adj3", "adj4"] {
            for v in ["-9223372036854775808", "-5", "9223372036854775807", "21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for (x, y) in polygons(&paths).into_iter().flatten() {
                    assert!((0..=1000).contains(&x) && (0..=700).contains(&y), "{preset} {name}={v}: ({x}, {y}) outside the box");
                }
            }
        }
    }
}

#[test]
fn arrows_with_empty_box_has_no_outline() {
    for preset in ARROWS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}
