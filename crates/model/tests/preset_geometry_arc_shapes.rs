//! Arc-based basic shapes: partial circle (pie), chord, arc, block arc and plaque. The arc's open
//! outline is a stroke-only path over a fill-only wedge. Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
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

const PRESETS: [&str; 5] = ["pie", "chord", "arc", "blockArc", "plaque"];

#[test]
fn arc_shape_points_stay_in_hostile_boxes() {
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
fn arc_shapes_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn arc_shape_adjustments_are_pinned() {
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
fn pie_paths() {
    check(
        "pie",
        &[],
        1000,
        600,
        &[(true, true, &[MoveTo(1000, 300), ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 16200000 }, LineTo(500, 300), Close])],
    );
    check(
        "pie",
        &[],
        333,
        777,
        &[(true, true, &[MoveTo(333, 388), ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 16200000 }, LineTo(166, 388), Close])],
    );
    check(
        "pie",
        &[("adj1", "val 0"), ("adj2", "val 9720000")],
        1000,
        600,
        &[(true, true, &[MoveTo(1000, 300), ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 9720000 }, LineTo(500, 300), Close])],
    );
}

#[test]
fn chord_paths() {
    check("chord", &[], 1000, 600, &[(true, true, &[MoveTo(757, 557), ArcTo { w_r: 500, h_r: 300, st_ang: 2700000, sw_ang: 13500000 }, Close])]);
    check("chord", &[], 333, 777, &[(true, true, &[MoveTo(320, 542), ArcTo { w_r: 166, h_r: 388, st_ang: 2700000, sw_ang: 13500000 }, Close])]);
    check(
        "chord",
        &[("adj1", "val 1620000"), ("adj2", "val 9720000")],
        1000,
        600,
        &[(true, true, &[MoveTo(881, 494), ArcTo { w_r: 500, h_r: 300, st_ang: 1620000, sw_ang: 8100000 }, Close])],
    );
}

#[test]
fn arc_paths() {
    check(
        "arc",
        &[],
        1000,
        600,
        &[
            (true, false, &[MoveTo(500, 0), ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 }, LineTo(500, 300), Close]),
            (false, true, &[MoveTo(500, 0), ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 }]),
        ],
    );
    check(
        "arc",
        &[],
        333,
        777,
        &[
            (true, false, &[MoveTo(166, 0), ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 }, LineTo(166, 388), Close]),
            (false, true, &[MoveTo(166, 0), ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 }]),
        ],
    );
    check(
        "arc",
        &[("adj1", "val 9720000"), ("adj2", "val 0")],
        1000,
        600,
        &[
            (true, false, &[MoveTo(60, 443), ArcTo { w_r: 500, h_r: 300, st_ang: 9720000, sw_ang: 11880000 }, LineTo(500, 300), Close]),
            (false, true, &[MoveTo(60, 443), ArcTo { w_r: 500, h_r: 300, st_ang: 9720000, sw_ang: 11880000 }]),
        ],
    );
}

#[test]
fn block_arc_paths() {
    check(
        "blockArc",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 10800000 },
                LineTo(850, 300),
                ArcTo { w_r: 350, h_r: 150, st_ang: 0, sw_ang: -10800000 },
                Close,
            ],
        )],
    );
    check(
        "blockArc",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 389),
                ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 10800000 },
                LineTo(250, 388),
                ArcTo { w_r: 83, h_r: 305, st_ang: 0, sw_ang: -10800000 },
                Close,
            ],
        )],
    );
    check(
        "blockArc",
        &[("adj1", "val 6480000"), ("adj2", "val 0"), ("adj3", "val 15000")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(404, 594),
                ArcTo { w_r: 500, h_r: 300, st_ang: 6480000, sw_ang: 15120000 },
                LineTo(910, 300),
                ArcTo { w_r: 410, h_r: 210, st_ang: 0, sw_ang: -15120000 },
                Close,
            ],
        )],
    );
}

#[test]
fn plaque_paths() {
    check(
        "plaque",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 100),
                ArcTo { w_r: 100, h_r: 100, st_ang: 5400000, sw_ang: -5400000 },
                LineTo(900, 0),
                ArcTo { w_r: 100, h_r: 100, st_ang: 10800000, sw_ang: -5400000 },
                LineTo(1000, 500),
                ArcTo { w_r: 100, h_r: 100, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(100, 600),
                ArcTo { w_r: 100, h_r: 100, st_ang: 0, sw_ang: -5400000 },
                Close,
            ],
        )],
    );
    check(
        "plaque",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 56),
                ArcTo { w_r: 56, h_r: 56, st_ang: 5400000, sw_ang: -5400000 },
                LineTo(277, 0),
                ArcTo { w_r: 56, h_r: 56, st_ang: 10800000, sw_ang: -5400000 },
                LineTo(333, 721),
                ArcTo { w_r: 56, h_r: 56, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(56, 777),
                ArcTo { w_r: 56, h_r: 56, st_ang: 0, sw_ang: -5400000 },
                Close,
            ],
        )],
    );
    check(
        "plaque",
        &[("adj", "val 10000")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 60),
                ArcTo { w_r: 60, h_r: 60, st_ang: 5400000, sw_ang: -5400000 },
                LineTo(940, 0),
                ArcTo { w_r: 60, h_r: 60, st_ang: 10800000, sw_ang: -5400000 },
                LineTo(1000, 540),
                ArcTo { w_r: 60, h_r: 60, st_ang: 16200000, sw_ang: -5400000 },
                LineTo(60, 600),
                ArcTo { w_r: 60, h_r: 60, st_ang: 0, sw_ang: -5400000 },
                Close,
            ],
        )],
    );
}
