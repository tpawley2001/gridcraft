//! The bent arrow, the U-turn arrow and the circular arrows (clockwise, counter-clockwise and two-headed). Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
//! and its preset shape definitions; `a:arcTo` (§20.1.9.3) is kept as `ArcTo` with its radii scaled
//! from the path's own box (`<path w h>`) to the shape box. Expected values are the definitions
//! evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo};
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

const PRESETS: [&str; 5] = ["bentArrow", "uturnArrow", "circularArrow", "leftCircularArrow", "leftRightCircularArrow"];

#[test]
fn arc_arrow_points_stay_in_hostile_boxes() {
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
fn arc_arrows_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn arc_arrow_adjustments_are_pinned() {
    for preset in PRESETS {
        for name in ["adj1", "adj2", "adj3", "adj4", "adj5"] {
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
fn bent_arrow_paths() {
    check(
        "bentArrow",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 600),
                LineTo(0, 338),
                ArcTo { w_r: 262, h_r: 262, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(850, 75),
                LineTo(850, 0),
                LineTo(1000, 150),
                LineTo(850, 300),
                LineTo(850, 225),
                LineTo(262, 225),
                ArcTo { w_r: 112, h_r: 112, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(150, 600),
                Close,
            ],
        )],
    );
    check(
        "bentArrow",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 777),
                LineTo(0, 187),
                ArcTo { w_r: 146, h_r: 146, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(250, 42),
                LineTo(250, 0),
                LineTo(333, 83),
                LineTo(250, 166),
                LineTo(250, 125),
                LineTo(146, 125),
                ArcTo { w_r: 62, h_r: 62, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(83, 777),
                Close,
            ],
        )],
    );
    check(
        "bentArrow",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 26250")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 600),
                LineTo(0, 202),
                ArcTo { w_r: 158, h_r: 158, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(910, 45),
                LineTo(910, 0),
                LineTo(1000, 90),
                LineTo(910, 180),
                LineTo(910, 135),
                LineTo(158, 135),
                ArcTo { w_r: 68, h_r: 68, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(90, 600),
                Close,
            ],
        )],
    );
}

#[test]
fn uturn_arrow_paths() {
    check(
        "uturnArrow",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 600),
                LineTo(0, 262),
                ArcTo { w_r: 262, h_r: 262, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(662, 0),
                ArcTo { w_r: 262, h_r: 262, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(925, 300),
                LineTo(1000, 300),
                LineTo(850, 450),
                LineTo(700, 300),
                LineTo(775, 300),
                LineTo(775, 262),
                ArcTo { w_r: 112, h_r: 112, st_ang: 0, sw_ang: -5400000 },
                LineTo(262, 150),
                ArcTo { w_r: 112, h_r: 112, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(150, 600),
                Close,
            ],
        )],
    );
    check(
        "uturnArrow",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 777),
                LineTo(0, 146),
                ArcTo { w_r: 146, h_r: 146, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(146, 0),
                ArcTo { w_r: 146, h_r: 146, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(291, 500),
                LineTo(333, 500),
                LineTo(250, 583),
                LineTo(166, 500),
                LineTo(208, 500),
                LineTo(208, 146),
                ArcTo { w_r: 62, h_r: 62, st_ang: 0, sw_ang: -5400000 },
                LineTo(146, 83),
                ArcTo { w_r: 62, h_r: 62, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(83, 777),
                Close,
            ],
        )],
    );
    check(
        "uturnArrow",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 26250"), ("adj5", "val 45000")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 600),
                LineTo(0, 158),
                ArcTo { w_r: 158, h_r: 158, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(798, 0),
                ArcTo { w_r: 158, h_r: 158, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(955, 180),
                LineTo(1000, 180),
                LineTo(910, 270),
                LineTo(820, 180),
                LineTo(865, 180),
                LineTo(865, 158),
                ArcTo { w_r: 68, h_r: 68, st_ang: 0, sw_ang: -5400000 },
                LineTo(158, 90),
                ArcTo { w_r: 68, h_r: 68, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(90, 600),
                Close,
            ],
        )],
    );
}

#[test]
fn circular_arrow_paths() {
    check(
        "circularArrow",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(38, 300),
                ArcTo { w_r: 462, h_r: 262, st_ang: 10800000, sw_ang: 9796115 },
                LineTo(931, 177),
                LineTo(925, 300),
                LineTo(781, 177),
                LineTo(793, 177),
                ArcTo { w_r: 388, h_r: 188, st_ang: 20233817, sw_ang: -9433817 },
                Close,
            ],
        )],
    );
    check(
        "circularArrow",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(21, 389),
                ArcTo { w_r: 146, h_r: 368, st_ang: 10800000, sw_ang: 9812148 },
                LineTo(332, 346),
                LineTo(291, 388),
                LineTo(249, 346),
                LineTo(270, 346),
                ArcTo { w_r: 104, h_r: 326, st_ang: 20249100, sw_ang: -9449100 },
                Close,
            ],
        )],
    );
    check(
        "circularArrow",
        &[("adj1", "val 7500"), ("adj2", "val 685391"), ("adj3", "val 12274608"), ("adj4", "val 6480000"), ("adj5", "val 7500")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(411, 573),
                ArcTo { w_r: 478, h_r: 278, st_ang: 6480000, sw_ang: 5832605 },
                LineTo(111, 112),
                LineTo(222, 98),
                LineTo(184, 165),
                LineTo(166, 152),
                ArcTo { w_r: 432, h_r: 232, st_ang: 12231832, sw_ang: -5751832 },
                Close,
            ],
        )],
    );
}

#[test]
fn left_circular_arrow_paths() {
    check(
        "leftCircularArrow",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(38, 300),
                LineTo(112, 300),
                ArcTo { w_r: 388, h_r: 188, st_ang: 10800000, sw_ang: -9433817 },
                LineTo(781, 423),
                LineTo(925, 300),
                LineTo(931, 423),
                LineTo(909, 423),
                ArcTo { w_r: 462, h_r: 262, st_ang: 1003885, sw_ang: 9796115 },
                Close,
            ],
        )],
    );
    check(
        "leftCircularArrow",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(21, 389),
                LineTo(62, 389),
                ArcTo { w_r: 104, h_r: 326, st_ang: 10800000, sw_ang: -9449100 },
                LineTo(249, 431),
                LineTo(291, 388),
                LineTo(332, 431),
                LineTo(311, 431),
                ArcTo { w_r: 146, h_r: 368, st_ang: 987852, sw_ang: 9812148 },
                Close,
            ],
        )],
    );
    check(
        "leftCircularArrow",
        &[("adj1", "val 7500"), ("adj2", "val -685391"), ("adj3", "val 685391"), ("adj4", "val 6480000"), ("adj5", "val 7500")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(411, 573),
                LineTo(426, 529),
                ArcTo { w_r: 432, h_r: 232, st_ang: 6480000, sw_ang: -5750561 },
                LineTo(883, 386),
                LineTo(955, 300),
                LineTo(973, 386),
                LineTo(954, 386),
                ArcTo { w_r: 478, h_r: 278, st_ang: 647527, sw_ang: 5832473 },
                Close,
            ],
        )],
    );
}

#[test]
fn left_right_circular_arrow_paths() {
    check(
        "leftRightCircularArrow",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(75, 300),
                LineTo(69, 177),
                LineTo(91, 177),
                ArcTo { w_r: 462, h_r: 262, st_ang: 11803885, sw_ang: 8792229 },
                LineTo(931, 177),
                LineTo(925, 300),
                LineTo(781, 177),
                LineTo(793, 177),
                ArcTo { w_r: 388, h_r: 188, st_ang: 20233817, sw_ang: -8067635 },
                LineTo(219, 177),
                Close,
            ],
        )],
    );
    check(
        "leftRightCircularArrow",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(42, 389),
                LineTo(1, 346),
                LineTo(22, 346),
                ArcTo { w_r: 146, h_r: 368, st_ang: 11787852, sw_ang: 8824295 },
                LineTo(332, 346),
                LineTo(291, 388),
                LineTo(249, 346),
                LineTo(270, 346),
                ArcTo { w_r: 104, h_r: 326, st_ang: 20249100, sw_ang: -8098200 },
                LineTo(84, 346),
                Close,
            ],
        )],
    );
    check(
        "leftRightCircularArrow",
        &[("adj1", "val 7500"), ("adj2", "val 685391"), ("adj3", "val 12274608"), ("adj4", "val 7165391"), ("adj5", "val 7500")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(418, 551),
                LineTo(349, 586),
                LineTo(355, 564),
                ArcTo { w_r: 478, h_r: 278, st_ang: 7127394, sw_ang: 5185210 },
                LineTo(111, 112),
                LineTo(222, 98),
                LineTo(184, 165),
                LineTo(166, 152),
                ArcTo { w_r: 432, h_r: 232, st_ang: 12231832, sw_ang: -5023664 },
                LineTo(377, 500),
                Close,
            ],
        )],
    );
}
