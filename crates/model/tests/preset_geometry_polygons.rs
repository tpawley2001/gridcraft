//! Straight-edged polygon presets (ECMA-376 Part 1 presetShapeDefinitions `heptagon`, `decagon`, `dodecagon`,
//! `nonIsoscelesTrapezoid`, `snip2SameRect`, `snip2DiagRect`, `halfFrame`, `corner`, `diagStripe`, `lightningBolt`).
//! Expected vertices were computed from the preset guides in floating point, rounded, then pinned into the shape box. Each
//! closed sub-path (MoveTo .. Close) is compared up to its starting vertex and direction; each coordinate may differ by two
//! units (integer guides truncate). Sub-paths are compared in the order the preset lists them.
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

const POLYGONS: [&str; 10] = [
    "heptagon",
    "decagon",
    "dodecagon",
    "nonIsoscelesTrapezoid",
    "snip2SameRect",
    "snip2DiagRect",
    "halfFrame",
    "corner",
    "diagStripe",
    "lightningBolt",
];

#[test]
fn heptagon_outline() {
    check(
        "heptagon",
        &[],
        1000000,
        600000,
        &[&[(0, 385864), (99031, 118838), (500000, 0), (900969, 118838), (1000000, 385864), (722520, 600000), (277480, 600000)]],
    );
    check(
        "heptagon",
        &[],
        300000,
        900000,
        &[&[(0, 578796), (29709, 178257), (150000, 0), (270291, 178257), (300000, 578796), (216756, 900000), (83244, 900000)]],
    );
    check(
        "heptagon",
        &[("hf", "val 61543"), ("vf", "val 63126")],
        1000000,
        600000,
        &[&[(199999, 231518), (259419, 71303), (500000, 0), (740581, 71303), (800001, 231518), (633511, 360002), (366489, 360002)]],
    );
}

#[test]
fn decagon_outline() {
    check(
        "decagon",
        &[],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (95492, 114590),
            (345492, 1),
            (654508, 1),
            (904508, 114590),
            (1000000, 300000),
            (904508, 485410),
            (654508, 599999),
            (345492, 599999),
            (95492, 485410),
        ]],
    );
    check(
        "decagon",
        &[],
        300000,
        900000,
        &[&[
            (0, 450000),
            (28647, 171885),
            (103647, 1),
            (196353, 1),
            (271353, 171885),
            (300000, 450000),
            (271353, 728115),
            (196353, 899999),
            (103647, 899999),
            (28647, 728115),
        ]],
    );
    check(
        "decagon",
        &[("vf", "val 63087")],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (95492, 188755),
            (345492, 120002),
            (654508, 120002),
            (904508, 188755),
            (1000000, 300000),
            (904508, 411245),
            (654508, 479998),
            (345492, 479998),
            (95492, 411245),
        ]],
    );
}

#[test]
fn dodecagon_outline() {
    check(
        "dodecagon",
        &[],
        1000000,
        600000,
        &[&[
            (0, 219611),
            (133981, 80389),
            (366019, 0),
            (633981, 0),
            (866019, 80389),
            (1000000, 219611),
            (1000000, 380389),
            (866019, 519611),
            (633981, 600000),
            (366019, 600000),
            (133981, 519611),
            (0, 380389),
        ]],
    );
    check(
        "dodecagon",
        &[],
        300000,
        900000,
        &[&[
            (0, 329417),
            (40194, 120583),
            (109806, 0),
            (190194, 0),
            (259806, 120583),
            (300000, 329417),
            (300000, 570583),
            (259806, 779417),
            (190194, 900000),
            (109806, 900000),
            (40194, 779417),
            (0, 570583),
        ]],
    );
}

#[test]
fn non_isosceles_trapezoid_outline() {
    check("nonIsoscelesTrapezoid", &[], 1000000, 600000, &[&[(0, 600000), (150000, 0), (850000, 0), (1000000, 600000)]]);
    check("nonIsoscelesTrapezoid", &[], 300000, 900000, &[&[(0, 900000), (75000, 0), (225000, 0), (300000, 900000)]]);
    check(
        "nonIsoscelesTrapezoid",
        &[("adj1", "val 15000"), ("adj2", "val 15000")],
        1000000,
        600000,
        &[&[(0, 600000), (90000, 0), (910000, 0), (1000000, 600000)]],
    );
}

#[test]
fn snip_2_same_rect_outline() {
    check(
        "snip2SameRect",
        &[],
        1000000,
        600000,
        &[&[(100002, 0), (899998, 0), (1000000, 100002), (1000000, 600000), (1000000, 600000), (0, 600000), (0, 600000), (0, 100002)]],
    );
    check(
        "snip2SameRect",
        &[],
        300000,
        900000,
        &[&[(50001, 0), (249999, 0), (300000, 50001), (300000, 900000), (300000, 900000), (0, 900000), (0, 900000), (0, 50001)]],
    );
    check(
        "snip2SameRect",
        &[("adj1", "val 10000"), ("adj2", "val 0")],
        1000000,
        600000,
        &[&[(60000, 0), (940000, 0), (1000000, 60000), (1000000, 600000), (1000000, 600000), (0, 600000), (0, 600000), (0, 60000)]],
    );
}

#[test]
fn snip_2_diag_rect_outline() {
    check(
        "snip2DiagRect",
        &[],
        1000000,
        600000,
        &[&[(0, 0), (899998, 0), (1000000, 100002), (1000000, 600000), (1000000, 600000), (100002, 600000), (0, 499998), (0, 0)]],
    );
    check(
        "snip2DiagRect",
        &[],
        300000,
        900000,
        &[&[(0, 0), (249999, 0), (300000, 50001), (300000, 900000), (300000, 900000), (50001, 900000), (0, 849999), (0, 0)]],
    );
    check(
        "snip2DiagRect",
        &[("adj1", "val 0"), ("adj2", "val 10000")],
        1000000,
        600000,
        &[&[(0, 0), (940000, 0), (1000000, 60000), (1000000, 600000), (1000000, 600000), (60000, 600000), (0, 540000), (0, 0)]],
    );
}

#[test]
fn half_frame_outline() {
    check("halfFrame", &[], 1000000, 600000, &[&[(0, 0), (1000000, 0), (666670, 199998), (199998, 199998), (199998, 480001), (0, 600000)]]);
    check("halfFrame", &[], 300000, 900000, &[&[(0, 0), (300000, 0), (266667, 99999), (99999, 99999), (99999, 600003), (0, 900000)]]);
    check(
        "halfFrame",
        &[("adj1", "val 19999"), ("adj2", "val 19999")],
        1000000,
        600000,
        &[&[(0, 0), (1000000, 0), (800010, 119994), (119994, 119994), (119994, 528004), (0, 600000)]],
    );
}

#[test]
fn corner_outline() {
    check("corner", &[], 1000000, 600000, &[&[(0, 0), (300000, 0), (300000, 300000), (1000000, 300000), (1000000, 600000), (0, 600000)]]);
    check("corner", &[], 300000, 900000, &[&[(0, 0), (150000, 0), (150000, 750000), (300000, 750000), (300000, 900000), (0, 900000)]]);
    check(
        "corner",
        &[("adj1", "val 30000"), ("adj2", "val 30000")],
        1000000,
        600000,
        &[&[(0, 0), (180000, 0), (180000, 420000), (1000000, 420000), (1000000, 600000), (0, 600000)]],
    );
}

#[test]
fn diag_stripe_outline() {
    check("diagStripe", &[], 1000000, 600000, &[&[(0, 300000), (500000, 0), (1000000, 0), (0, 600000)]]);
    check("diagStripe", &[], 300000, 900000, &[&[(0, 450000), (150000, 0), (300000, 0), (0, 900000)]]);
    check("diagStripe", &[("adj", "val 30000")], 1000000, 600000, &[&[(0, 180000), (300000, 0), (1000000, 0), (0, 600000)]]);
}

#[test]
fn lightning_bolt_outline() {
    check(
        "lightningBolt",
        &[],
        1000000,
        600000,
        &[&[
            (392222, 0),
            (595370, 168888),
            (511574, 188805),
            (767453, 333527),
            (683657, 357694),
            (1000000, 600000),
            (463518, 414305),
            (565833, 388527),
            (232500, 269583),
            (351944, 232833),
            (0, 108055),
        ]],
    );
    check(
        "lightningBolt",
        &[],
        300000,
        900000,
        &[&[
            (117666, 0),
            (178611, 253333),
            (153472, 283208),
            (230236, 500291),
            (205097, 536541),
            (300000, 900000),
            (139055, 621458),
            (169750, 582791),
            (69750, 404375),
            (105583, 349250),
            (0, 162083),
        ]],
    );
}

#[test]
fn polygons_vertices_stay_in_hostile_boxes() {
    for preset in POLYGONS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            for (x, y) in polygons(&paths).into_iter().flatten() {
                assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} in {w}x{h}: ({x}, {y}) outside the box");
            }
        }
    }
}

#[test]
fn polygons_hostile_adjustments_do_not_panic() {
    for preset in POLYGONS {
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
fn polygons_with_empty_box_has_no_outline() {
    for preset in POLYGONS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}
