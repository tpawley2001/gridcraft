//! The curved right, left, up and down arrows: a filled arrow, a shaded (darkenLess) back face and an outline path.
//! A path's `fill="lighten" | "lightenLess" | "darken" | "darkenLess"` (ECMA-376 Part 1 §20.1.9.15,
//! ST_PathFillMode) fills that face with the shape's fill shaded, `fill="none"` draws no fill and
//! `stroke="false"` no outline. Paths and guides follow §20.1.9.18 (`prstGeom`) and its preset shape
//! definitions; `a:arcTo` (§20.1.9.3) is kept as `ArcTo` with its radii scaled from the path's own box
//! (`<path w h>`) to the shape box. Expected values are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo};
use gridcraft_model::PathShade::{self, DarkenLess, Norm};
use gridcraft_model::{ShapeGeometry, preset_paths};

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

/// `expected` is one `(fill, stroke, shade, commands)` per path, in the definition's order.
fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[(bool, bool, PathShade, &[PathCommand])]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), expected.len(), "{preset} {adjust:?} in {w}x{h}: path count, actual {paths:?}");
    for (i, (p, (fill, stroke, shade, cmds))) in paths.iter().zip(expected).enumerate() {
        assert_eq!((p.w, p.h), (w, h), "{preset} path {i}: path box is the shape box");
        assert_eq!((p.fill, p.stroke), (*fill, *stroke), "{preset} path {i}: fill/stroke flags");
        assert_eq!(p.shade, *shade, "{preset} path {i}: fill shading");
        let ok = p.commands.len() == cmds.len() && p.commands.iter().zip(cmds.iter()).all(|(a, b)| near(a, b));
        assert!(ok, "{preset} {adjust:?} in {w}x{h} path {i}:\n actual   {:?}\n expected {cmds:?}", p.commands);
    }
}

const PRESETS: [&str; 4] = ["curvedRightArrow", "curvedLeftArrow", "curvedUpArrow", "curvedDownArrow"];

#[test]
fn curved_arrow_points_stay_in_hostile_boxes() {
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
fn curved_arrows_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn curved_arrow_adjustments_are_pinned() {
    for preset in PRESETS {
        for name in ["adj1", "adj2", "adj3"] {
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
fn curved_right_arrow_paths() {
    check(
        "curvedRightArrow",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 188),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 10800000, sw_ang: -3061307 },
                    LineTo(850, 298),
                    LineTo(1000, 450),
                    LineTo(850, 598),
                    LineTo(850, 523),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 7738693, sw_ang: 3061307 },
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(1000, 150),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 16200000, sw_ang: -5119309 },
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 10519309, sw_ang: 5680691 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 188),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 10800000, sw_ang: -3061307 },
                    LineTo(850, 298),
                    LineTo(1000, 450),
                    LineTo(850, 598),
                    LineTo(850, 523),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 7738693, sw_ang: 3061307 },
                    LineTo(0, 188),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(1000, 150),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 16200000, sw_ang: -5119309 },
                ],
            ),
        ],
    );
    check(
        "curvedRightArrow",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 326),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 10800000, sw_ang: -4513668 },
                    LineTo(250, 600),
                    LineTo(333, 694),
                    LineTo(250, 767),
                    LineTo(250, 725),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 6286332, sw_ang: 4513668 },
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(333, 83),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 16200000, sw_ang: -4969009 },
                    ArcTo { w_r: 333, h_r: 326, st_ang: 10369009, sw_ang: 5830991 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 326),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 10800000, sw_ang: -4513668 },
                    LineTo(250, 600),
                    LineTo(333, 694),
                    LineTo(250, 767),
                    LineTo(250, 725),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 6286332, sw_ang: 4513668 },
                    LineTo(0, 326),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(333, 83),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 16200000, sw_ang: -4969009 },
                ],
            ),
        ],
    );
    check(
        "curvedRightArrow",
        &[("adj1", "val 15000"), ("adj2", "val 30000"), ("adj3", "val 15000")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 232),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 10800000, sw_ang: -4125611 },
                    LineTo(910, 419),
                    LineTo(1000, 510),
                    LineTo(910, 599),
                    LineTo(910, 554),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 6674389, sw_ang: 4125611 },
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(1000, 90),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 16200000, sw_ang: -5242430 },
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 10642430, sw_ang: 5557570 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 232),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 10800000, sw_ang: -4125611 },
                    LineTo(910, 419),
                    LineTo(1000, 510),
                    LineTo(910, 599),
                    LineTo(910, 554),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 6674389, sw_ang: 4125611 },
                    LineTo(0, 232),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(1000, 90),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 16200000, sw_ang: -5242430 },
                ],
            ),
        ],
    );
}

#[test]
fn curved_left_arrow_paths() {
    check(
        "curvedLeftArrow",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 450),
                    LineTo(150, 298),
                    LineTo(150, 373),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 3061307, sw_ang: -2780616 },
                    ArcTo { w_r: 1000, h_r: 188, st_ang: -280691, sw_ang: 3341998 },
                    LineTo(150, 598),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(1000, 338),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 0, sw_ang: -5400000 },
                    LineTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 16200000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(1000, 338),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 0, sw_ang: -5400000 },
                    LineTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 338),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 0, sw_ang: 3061307 },
                    LineTo(150, 598),
                    LineTo(0, 450),
                    LineTo(150, 298),
                    LineTo(150, 373),
                    ArcTo { w_r: 1000, h_r: 188, st_ang: 3061307, sw_ang: -2780616 },
                ],
            ),
        ],
    );
    check(
        "curvedLeftArrow",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 694),
                    LineTo(83, 600),
                    LineTo(83, 642),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 4513668, sw_ang: -4082677 },
                    ArcTo { w_r: 333, h_r: 326, st_ang: -430991, sw_ang: 4944659 },
                    LineTo(83, 767),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(333, 409),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 0, sw_ang: -5400000 },
                    LineTo(0, 0),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 16200000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(333, 409),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 0, sw_ang: -5400000 },
                    LineTo(0, 0),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(333, 409),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 0, sw_ang: 4513668 },
                    LineTo(83, 767),
                    LineTo(0, 694),
                    LineTo(83, 600),
                    LineTo(83, 642),
                    ArcTo { w_r: 333, h_r: 326, st_ang: 4513668, sw_ang: -4082677 },
                ],
            ),
        ],
    );
    check(
        "curvedLeftArrow",
        &[("adj1", "val 15000"), ("adj2", "val 30000"), ("adj3", "val 15000")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 510),
                    LineTo(90, 419),
                    LineTo(90, 464),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 4125611, sw_ang: -3968041 },
                    ArcTo { w_r: 1000, h_r: 232, st_ang: -157570, sw_ang: 4283181 },
                    LineTo(90, 599),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(1000, 322),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 0, sw_ang: -5400000 },
                    LineTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 16200000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(1000, 322),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 0, sw_ang: -5400000 },
                    LineTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 322),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 0, sw_ang: 4125611 },
                    LineTo(90, 599),
                    LineTo(0, 510),
                    LineTo(90, 419),
                    LineTo(90, 464),
                    ArcTo { w_r: 1000, h_r: 232, st_ang: 4125611, sw_ang: -3968041 },
                ],
            ),
        ],
    );
}

#[test]
fn curved_up_arrow_paths() {
    check(
        "curvedUpArrow",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(850, 0),
                    LineTo(988, 150),
                    LineTo(913, 150),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 1307468, sw_ang: 4528186 },
                    ArcTo { w_r: 388, h_r: 600, st_ang: 4964347, sw_ang: -3656879 },
                    LineTo(688, 150),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(388, 600),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(150, 0),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 10800000, sw_ang: -5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(462, 589),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 4964347, sw_ang: -3656879 },
                    LineTo(688, 150),
                    LineTo(850, 0),
                    LineTo(988, 150),
                    LineTo(913, 150),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 1307468, sw_ang: 4092532 },
                    LineTo(388, 600),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(150, 0),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 10800000, sw_ang: -5400000 },
                ],
            ),
        ],
    );
    check(
        "curvedUpArrow",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(250, 0),
                    LineTo(332, 83),
                    LineTo(291, 83),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 2329276, sw_ang: 3271437 },
                    ArcTo { w_r: 104, h_r: 777, st_ang: 5199288, sw_ang: -2870012 },
                    LineTo(166, 83),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(104, 777),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(83, 0),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 10800000, sw_ang: -5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(146, 712),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 5199288, sw_ang: -2870012 },
                    LineTo(166, 83),
                    LineTo(250, 0),
                    LineTo(332, 83),
                    LineTo(291, 83),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 2329276, sw_ang: 3070724 },
                    LineTo(104, 777),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(83, 0),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 10800000, sw_ang: -5400000 },
                ],
            ),
        ],
    );
    check(
        "curvedUpArrow",
        &[("adj1", "val 15000"), ("adj2", "val 30000"), ("adj3", "val 15000")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(910, 0),
                    LineTo(995, 90),
                    LineTo(950, 90),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 713147, sw_ang: 4945602 },
                    ArcTo { w_r: 432, h_r: 600, st_ang: 5141252, sw_ang: -4428105 },
                    LineTo(815, 90),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(432, 600),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(90, 0),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 10800000, sw_ang: -5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(478, 597),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 5141252, sw_ang: -4428105 },
                    LineTo(815, 90),
                    LineTo(910, 0),
                    LineTo(995, 90),
                    LineTo(950, 90),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 713147, sw_ang: 4686853 },
                    LineTo(432, 600),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(90, 0),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 10800000, sw_ang: -5400000 },
                ],
            ),
        ],
    );
}

#[test]
fn curved_down_arrow_paths() {
    check(
        "curvedDownArrow",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(850, 600),
                    LineTo(688, 450),
                    LineTo(763, 450),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 20292532, sw_ang: -4092532 },
                    LineTo(538, 0),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 16200000, sw_ang: 4092532 },
                    LineTo(988, 450),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(462, 11),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 15764347, sw_ang: -4964347 },
                    LineTo(0, 600),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 10800000, sw_ang: 5835653 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(462, 11),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 15764347, sw_ang: -4964347 },
                    LineTo(0, 600),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(538, 0),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 16200000, sw_ang: 4092532 },
                    LineTo(988, 450),
                    LineTo(850, 600),
                    LineTo(688, 450),
                    LineTo(763, 450),
                    ArcTo { w_r: 388, h_r: 600, st_ang: 20292532, sw_ang: -4092532 },
                ],
            ),
        ],
    );
    check(
        "curvedDownArrow",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(250, 777),
                    LineTo(166, 694),
                    LineTo(208, 694),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 19270724, sw_ang: -3070724 },
                    LineTo(187, 0),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 16200000, sw_ang: 3070724 },
                    LineTo(332, 694),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(146, 65),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 15999288, sw_ang: -5199288 },
                    LineTo(0, 777),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 10800000, sw_ang: 5600712 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(146, 65),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 15999288, sw_ang: -5199288 },
                    LineTo(0, 777),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(187, 0),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 16200000, sw_ang: 3070724 },
                    LineTo(332, 694),
                    LineTo(250, 777),
                    LineTo(166, 694),
                    LineTo(208, 694),
                    ArcTo { w_r: 104, h_r: 777, st_ang: 19270724, sw_ang: -3070724 },
                ],
            ),
        ],
    );
    check(
        "curvedDownArrow",
        &[("adj1", "val 15000"), ("adj2", "val 30000"), ("adj3", "val 15000")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(910, 600),
                    LineTo(815, 510),
                    LineTo(860, 510),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 20886853, sw_ang: -4686853 },
                    LineTo(522, 0),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 16200000, sw_ang: 4686853 },
                    LineTo(995, 510),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(478, 3),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 15941252, sw_ang: -5141252 },
                    LineTo(0, 600),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 10800000, sw_ang: 5658748 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(478, 3),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 15941252, sw_ang: -5141252 },
                    LineTo(0, 600),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(522, 0),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 16200000, sw_ang: 4686853 },
                    LineTo(995, 510),
                    LineTo(910, 600),
                    LineTo(815, 510),
                    LineTo(860, 510),
                    ArcTo { w_r: 432, h_r: 600, st_ang: 20886853, sw_ang: -4686853 },
                ],
            ),
        ],
    );
}
