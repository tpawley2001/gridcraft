//! Bracket and brace presets: left/right bracket, left/right brace, double bracket and double
//! brace, each a fill-only path and a stroke-only outline drawn from arcs. Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
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

const PRESETS: [&str; 6] = ["leftBracket", "rightBracket", "leftBrace", "rightBrace", "bracketPair", "bracePair"];

#[test]
fn bracket_points_stay_in_hostile_boxes() {
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
fn brackets_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn bracket_adjustments_are_pinned() {
    for preset in PRESETS {
        for name in ["adj", "adj1", "adj2"] {
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
fn left_bracket_paths() {
    check(
        "leftBracket",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 50),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 10800000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 50),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 10800000, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "leftBracket",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(333, 777),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 28),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 10800000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(333, 777),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 28),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 10800000, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "leftBracket",
        &[("adj", "val 4999")],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 30),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 10800000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 30),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 10800000, sw_ang: 5400000 },
                ],
            ),
        ],
    );
}

#[test]
fn right_bracket_paths() {
    check(
        "rightBracket",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 550),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 550),
                    ArcTo { w_r: 1000, h_r: 50, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "rightBracket",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(333, 749),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(333, 749),
                    ArcTo { w_r: 333, h_r: 28, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "rightBracket",
        &[("adj", "val 4999")],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 570),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 570),
                    ArcTo { w_r: 1000, h_r: 30, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
}

#[test]
fn left_brace_paths() {
    check(
        "leftBrace",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(500, 350),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 50, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(500, 50),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 10800000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(500, 350),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 50, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(500, 50),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 10800000, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "leftBrace",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(333, 777),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(166, 416),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 166, h_r: 28, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(166, 28),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 10800000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(333, 777),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(166, 416),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 166, h_r: 28, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(166, 28),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 10800000, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "leftBrace",
        &[("adj1", "val 4999"), ("adj2", "val 30000")],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(500, 210),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 30, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(500, 30),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 10800000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(1000, 600),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(500, 210),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 30, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(500, 30),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 10800000, sw_ang: 5400000 },
                ],
            ),
        ],
    );
}

#[test]
fn right_brace_paths() {
    check(
        "rightBrace",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(500, 250),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 50, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(500, 550),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(500, 250),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 50, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(500, 550),
                    ArcTo { w_r: 500, h_r: 50, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "rightBrace",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(166, 361),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 166, h_r: 28, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(166, 749),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(166, 361),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 166, h_r: 28, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(166, 749),
                    ArcTo { w_r: 166, h_r: 28, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "rightBrace",
        &[("adj1", "val 4999"), ("adj2", "val 30000")],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(500, 150),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 30, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(500, 570),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(0, 0),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(500, 150),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 500, h_r: 30, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(500, 570),
                    ArcTo { w_r: 500, h_r: 30, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
}

#[test]
fn bracket_pair_paths() {
    check(
        "bracketPair",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 100),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(900, 0),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 500),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 0, sw_ang: 5400000 },
                    LineTo(100, 600),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(100, 600),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 100),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 10800000, sw_ang: 5400000 },
                    MoveTo(900, 0),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 500),
                    ArcTo { w_r: 100, h_r: 100, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "bracketPair",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 56),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(277, 0),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(333, 721),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 0, sw_ang: 5400000 },
                    LineTo(56, 777),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(56, 777),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 56),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 10800000, sw_ang: 5400000 },
                    MoveTo(277, 0),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(333, 721),
                    ArcTo { w_r: 56, h_r: 56, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "bracketPair",
        &[("adj", "val 10000")],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 60),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(940, 0),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 540),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 0, sw_ang: 5400000 },
                    LineTo(60, 600),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(60, 600),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(0, 60),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 10800000, sw_ang: 5400000 },
                    MoveTo(940, 0),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(1000, 540),
                    ArcTo { w_r: 60, h_r: 60, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
}

#[test]
fn brace_pair_paths() {
    check(
        "bracePair",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(100, 600),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(50, 350),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 50, h_r: 50, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(50, 50),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(900, 0),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(950, 250),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 50, h_r: 50, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(950, 550),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(100, 600),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(50, 350),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 50, h_r: 50, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(50, 50),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 10800000, sw_ang: 5400000 },
                    MoveTo(900, 0),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(950, 250),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 50, h_r: 50, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(950, 550),
                    ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "bracePair",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(55, 777),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(28, 416),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 28, h_r: 28, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(28, 28),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(278, 0),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(305, 361),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 28, h_r: 28, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(305, 749),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(55, 777),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(28, 416),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 28, h_r: 28, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(28, 28),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 10800000, sw_ang: 5400000 },
                    MoveTo(278, 0),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(305, 361),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 28, h_r: 28, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(305, 749),
                    ArcTo { w_r: 28, h_r: 28, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
    check(
        "bracePair",
        &[("adj", "val 4999")],
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(60, 600),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(30, 330),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 30, h_r: 30, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(30, 30),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 10800000, sw_ang: 5400000 },
                    LineTo(940, 0),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(970, 270),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 30, h_r: 30, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(970, 570),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 0, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                &[
                    MoveTo(60, 600),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 5400000, sw_ang: 5400000 },
                    LineTo(30, 330),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 0, sw_ang: -5400000 },
                    ArcTo { w_r: 30, h_r: 30, st_ang: 5400000, sw_ang: -5400000 },
                    LineTo(30, 30),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 10800000, sw_ang: 5400000 },
                    MoveTo(940, 0),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 16200000, sw_ang: 5400000 },
                    LineTo(970, 270),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 10800000, sw_ang: -5400000 },
                    ArcTo { w_r: 30, h_r: 30, st_ang: 16200000, sw_ang: -5400000 },
                    LineTo(970, 570),
                    ArcTo { w_r: 30, h_r: 30, st_ang: 0, sw_ang: 5400000 },
                ],
            ),
        ],
    );
}
