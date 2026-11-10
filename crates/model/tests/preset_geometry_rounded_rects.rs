//! Rounded rectangles with one, two same-side or two diagonal rounded corners (`round1Rect`,
//! `round2SameRect`, `round2DiagRect`) and the quarter-ellipse pie wedge (`pieWedge`). Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
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

const PRESETS: [&str; 4] = ["round1Rect", "round2SameRect", "round2DiagRect", "pieWedge"];

#[test]
fn rounded_shape_points_stay_in_hostile_boxes() {
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
fn rounded_shapes_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn rounded_shape_adjustments_are_pinned() {
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
fn round1_rect_paths() {
    check(
        "round1Rect",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 0),
                LineTo(900, 0),
                ArcTo { w_r: 100, h_r: 100, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 600),
                LineTo(0, 600),
                Close,
            ],
        )],
    );
    check(
        "round1Rect",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[MoveTo(0, 0), LineTo(277, 0), ArcTo { w_r: 56, h_r: 56, st_ang: 16200000, sw_ang: 5400000 }, LineTo(333, 777), LineTo(0, 777), Close],
        )],
    );
    check(
        "round1Rect",
        &[("adj", "val 10000")],
        1000,
        600,
        &[(
            true,
            true,
            &[MoveTo(0, 0), LineTo(940, 0), ArcTo { w_r: 60, h_r: 60, st_ang: 16200000, sw_ang: 5400000 }, LineTo(1000, 600), LineTo(0, 600), Close],
        )],
    );
}

#[test]
fn round2_same_rect_paths() {
    check(
        "round2SameRect",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(100, 0),
                LineTo(900, 0),
                ArcTo { w_r: 100, h_r: 100, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 600),
                ArcTo { w_r: 0, h_r: 0, st_ang: 0, sw_ang: 5400000 },
                LineTo(0, 600),
                ArcTo { w_r: 0, h_r: 0, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 100),
                ArcTo { w_r: 100, h_r: 100, st_ang: 10800000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "round2SameRect",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(56, 0),
                LineTo(277, 0),
                ArcTo { w_r: 56, h_r: 56, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(333, 777),
                ArcTo { w_r: 0, h_r: 0, st_ang: 0, sw_ang: 5400000 },
                LineTo(0, 777),
                ArcTo { w_r: 0, h_r: 0, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 56),
                ArcTo { w_r: 56, h_r: 56, st_ang: 10800000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "round2SameRect",
        &[("adj1", "val 10000"), ("adj2", "val 0")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(60, 0),
                LineTo(940, 0),
                ArcTo { w_r: 60, h_r: 60, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 600),
                ArcTo { w_r: 0, h_r: 0, st_ang: 0, sw_ang: 5400000 },
                LineTo(0, 600),
                ArcTo { w_r: 0, h_r: 0, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 60),
                ArcTo { w_r: 60, h_r: 60, st_ang: 10800000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
}

#[test]
fn round2_diag_rect_paths() {
    check(
        "round2DiagRect",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(100, 0),
                LineTo(1000, 0),
                ArcTo { w_r: 0, h_r: 0, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 500),
                ArcTo { w_r: 100, h_r: 100, st_ang: 0, sw_ang: 5400000 },
                LineTo(0, 600),
                ArcTo { w_r: 0, h_r: 0, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 100),
                ArcTo { w_r: 100, h_r: 100, st_ang: 10800000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "round2DiagRect",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(56, 0),
                LineTo(333, 0),
                ArcTo { w_r: 0, h_r: 0, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(333, 721),
                ArcTo { w_r: 56, h_r: 56, st_ang: 0, sw_ang: 5400000 },
                LineTo(0, 777),
                ArcTo { w_r: 0, h_r: 0, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 56),
                ArcTo { w_r: 56, h_r: 56, st_ang: 10800000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "round2DiagRect",
        &[("adj1", "val 10000"), ("adj2", "val 0")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(60, 0),
                LineTo(1000, 0),
                ArcTo { w_r: 0, h_r: 0, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 540),
                ArcTo { w_r: 60, h_r: 60, st_ang: 0, sw_ang: 5400000 },
                LineTo(0, 600),
                ArcTo { w_r: 0, h_r: 0, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 60),
                ArcTo { w_r: 60, h_r: 60, st_ang: 10800000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
}

#[test]
fn pie_wedge_paths() {
    check(
        "pieWedge",
        &[],
        1000,
        600,
        &[(true, true, &[MoveTo(0, 600), ArcTo { w_r: 1000, h_r: 600, st_ang: 10800000, sw_ang: 5400000 }, LineTo(1000, 600), Close])],
    );
    check(
        "pieWedge",
        &[],
        333,
        777,
        &[(true, true, &[MoveTo(0, 777), ArcTo { w_r: 333, h_r: 777, st_ang: 10800000, sw_ang: 5400000 }, LineTo(333, 777), Close])],
    );
}
