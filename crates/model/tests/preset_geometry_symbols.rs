//! The "No" symbol (`noSmoking`), the division sign (`mathDivide`) and the teardrop. Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
//! and its preset shape definitions; `a:arcTo` (§20.1.9.3) is kept as `ArcTo` with its radii scaled
//! from the path's own box (`<path w h>`) to the shape box. Expected values are the definitions
//! evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo, QuadBezTo};
use gridcraft_model::{PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    let d = |x: i64, y: i64, tol: i64| (x - y).abs() <= tol;
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => d(*x1, *x2, 2) && d(*y1, *y2, 2),
        (ArcTo { w_r: w1, h_r: h1, st_ang: s1, sw_ang: e1 }, ArcTo { w_r: w2, h_r: h2, st_ang: s2, sw_ang: e2 }) => {
            // Angles are in 60000ths of a degree: 100 is under 0.002°.
            d(*w1, *w2, 2) && d(*h1, *h2, 2) && d(*s1, *s2, 100) && d(*e1, *e2, 100)
        }
        (QuadBezTo([(x1, y1), (x2, y2)]), QuadBezTo([(x3, y3), (x4, y4)])) => d(*x1, *x3, 2) && d(*y1, *y3, 2) && d(*x2, *x4, 2) && d(*y2, *y4, 2),
        (Close, Close) => true,
        _ => false,
    }
}

/// `expected` is one `(fill, stroke, commands)` per path, in the definition's order.
fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[(bool, bool, &[PathCommand])]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), expected.len(), "{preset} {adjust:?} in {w}x{h}: path count, actual {paths:?}");
    for (i, (p, (fill, stroke, cmds))) in paths.iter().zip(expected).enumerate() {
        assert_eq!((p.w, p.h), (w, h), "{preset} path {i}: path box is the shape box");
        assert_eq!((p.fill, p.stroke), (*fill, *stroke), "{preset} path {i}: fill/stroke flags");
        assert_eq!(p.shade, PathShade::Norm, "{preset} path {i}: plain fill");
        let ok = p.commands.len() == cmds.len() && p.commands.iter().zip(cmds.iter()).all(|(a, b)| near(a, b));
        assert!(ok, "{preset} {adjust:?} in {w}x{h} path {i}:\n actual   {:?}\n expected {cmds:?}", p.commands);
    }
}

const PRESETS: [&str; 3] = ["noSmoking", "mathDivide", "teardrop"];

#[test]
fn symbol_shape_points_stay_in_hostile_boxes() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            for c in paths.iter().flat_map(|p| &p.commands) {
                match *c {
                    MoveTo(x, y) | LineTo(x, y) => {
                        assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} in {w}x{h}: ({x}, {y}) outside the box")
                    }
                    ArcTo { w_r, h_r, .. } => assert!((0..=w).contains(&w_r) && (0..=h).contains(&h_r), "{preset} in {w}x{h}: radii {w_r}, {h_r}"),
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn symbol_shapes_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn symbol_shape_adjustments_are_pinned() {
    for preset in PRESETS {
        for name in ["adj", "adj1", "adj2", "adj3"] {
            for v in ["-9223372036854775808", "-5", "9223372036854775807", "21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for c in paths.iter().flat_map(|p| &p.commands) {
                    match *c {
                        MoveTo(x, y) | LineTo(x, y) => assert!((0..=1000).contains(&x) && (0..=700).contains(&y), "{preset} {name}={v}: ({x}, {y})"),
                        ArcTo { w_r, h_r, .. } => {
                            assert!((0..=1000).contains(&w_r) && (0..=700).contains(&h_r), "{preset} {name}={v}: radii {w_r}, {h_r}")
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[test]
fn no_smoking_paths() {
    check(
        "noSmoking",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                Close,
                MoveTo(811, 412),
                ArcTo { w_r: 388, h_r: 188, st_ang: 1184896, sw_ang: -9454140 },
                Close,
                MoveTo(189, 188),
                ArcTo { w_r: 388, h_r: 188, st_ang: -9615104, sw_ang: -9454140 },
                Close,
            ],
        )],
    );
    check(
        "noSmoking",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 388),
                ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                Close,
                MoveTo(259, 539),
                ArcTo { w_r: 104, h_r: 326, st_ang: 3505174, sw_ang: -9794178 },
                Close,
                MoveTo(74, 238),
                ArcTo { w_r: 104, h_r: 326, st_ang: -7294826, sw_ang: -9794178 },
                Close,
            ],
        )],
    );
    check(
        "noSmoking",
        &[("adj", "val 11250")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                Close,
                MoveTo(825, 453),
                ArcTo { w_r: 432, h_r: 232, st_ang: 1514249, sw_ang: -10112848 },
                Close,
                MoveTo(175, 147),
                ArcTo { w_r: 432, h_r: 232, st_ang: -9285751, sw_ang: -10112848 },
                Close,
            ],
        )],
    );
}

#[test]
fn math_divide_paths() {
    check(
        "mathDivide",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(500, 71),
                ArcTo { w_r: 71, h_r: 71, st_ang: 16200000, sw_ang: 21600000 },
                Close,
                MoveTo(500, 529),
                ArcTo { w_r: 71, h_r: 71, st_ang: 5400000, sw_ang: 21600000 },
                Close,
                MoveTo(133, 229),
                LineTo(867, 229),
                LineTo(867, 371),
                LineTo(133, 371),
                Close,
            ],
        )],
    );
    check(
        "mathDivide",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(166, 92),
                ArcTo { w_r: 91, h_r: 91, st_ang: 16200000, sw_ang: 21600000 },
                Close,
                MoveTo(166, 685),
                ArcTo { w_r: 91, h_r: 91, st_ang: 5400000, sw_ang: 21600000 },
                Close,
                MoveTo(44, 297),
                LineTo(289, 297),
                LineTo(289, 480),
                LineTo(44, 480),
                Close,
            ],
        )],
    );
    check(
        "mathDivide",
        &[("adj1", "val 14112"), ("adj2", "val 3528"), ("adj3", "val 7056")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(500, 152),
                ArcTo { w_r: 42, h_r: 42, st_ang: 16200000, sw_ang: 21600000 },
                Close,
                MoveTo(500, 448),
                ArcTo { w_r: 42, h_r: 42, st_ang: 5400000, sw_ang: 21600000 },
                Close,
                MoveTo(133, 258),
                LineTo(867, 258),
                LineTo(867, 342),
                LineTo(133, 342),
                Close,
            ],
        )],
    );
}

#[test]
fn teardrop_paths() {
    check(
        "teardrop",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                QuadBezTo([(750, 0), (1000, 0)]),
                QuadBezTo([(1000, 150), (1000, 300)]),
                ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "teardrop",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 388),
                ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                QuadBezTo([(250, 0), (333, 0)]),
                QuadBezTo([(333, 194), (333, 388)]),
                ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "teardrop",
        &[("adj", "val 60000")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                QuadBezTo([(650, 0), (800, 120)]),
                QuadBezTo([(1000, 210), (1000, 300)]),
                ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
}
