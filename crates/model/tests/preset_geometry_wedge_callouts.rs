//! Wedge callouts (rectangle, rounded rectangle, oval): the callout's tail tip is placed by `adj1`/`adj2`
//! relative to the shape's centre and, as in Excel, usually lies OUTSIDE the shape box (the default
//! puts it below the bottom edge). Paths and guides follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`) and
//! its preset shape definitions; expected values are the definitions evaluated in the shape box.

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

const PRESETS: [&str; 3] = ["wedgeRectCallout", "wedgeRoundRectCallout", "wedgeEllipseCallout"];

#[test]
fn the_default_tail_tip_lies_below_the_box() {
    for preset in PRESETS {
        let paths = preset_paths(&geom(preset, &[]), 1000, 600).unwrap_or_else(|| panic!("{preset} has an outline"));
        let below = paths
            .iter()
            .flat_map(|p| &p.commands)
            .any(|c| matches!(*c, MoveTo(x, y) | LineTo(x, y) if (280..=300).contains(&x) && (670..=680).contains(&y)));
        assert!(below, "{preset}: tail tip (292, 675) kept outside the 1000x600 box: {paths:?}");
    }
}

#[test]
fn hostile_boxes_and_adjustments_do_not_panic() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            assert!(preset_paths(&geom(preset, &[]), w, h).is_some(), "{preset} in {w}x{h}");
        }
        for name in ["adj1", "adj2", "adj3"] {
            for v in ["-9223372036854775808", "-5", "9223372036854775807", "21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for c in paths.iter().flat_map(|p| &p.commands) {
                    if let ArcTo { w_r, h_r, .. } = *c {
                        assert!((0..=1000).contains(&w_r) && (0..=700).contains(&h_r), "{preset} {name}={v}: radii {w_r}, {h_r}");
                    }
                }
            }
        }
    }
}

#[test]
fn wedge_callouts_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn wedge_rect_callout_paths() {
    check(
        "wedgeRectCallout",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 0),
                LineTo(167, 0),
                LineTo(167, 0),
                LineTo(417, 0),
                LineTo(1000, 0),
                LineTo(1000, 350),
                LineTo(1000, 350),
                LineTo(1000, 500),
                LineTo(1000, 600),
                LineTo(417, 600),
                LineTo(292, 675),
                LineTo(167, 600),
                LineTo(0, 600),
                LineTo(0, 500),
                LineTo(0, 350),
                LineTo(0, 350),
                Close,
            ],
        )],
    );
    check(
        "wedgeRectCallout",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 0),
                LineTo(56, 0),
                LineTo(56, 0),
                LineTo(139, 0),
                LineTo(333, 0),
                LineTo(333, 453),
                LineTo(333, 453),
                LineTo(333, 648),
                LineTo(333, 777),
                LineTo(139, 777),
                LineTo(97, 874),
                LineTo(56, 777),
                LineTo(0, 777),
                LineTo(0, 648),
                LineTo(0, 453),
                LineTo(0, 453),
                Close,
            ],
        )],
    );
    check(
        "wedgeRectCallout",
        &[("adj1", "val -12499"), ("adj2", "val 37500")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 0),
                LineTo(167, 0),
                LineTo(167, 0),
                LineTo(417, 0),
                LineTo(1000, 0),
                LineTo(1000, 350),
                LineTo(1000, 350),
                LineTo(1000, 500),
                LineTo(1000, 600),
                LineTo(417, 600),
                LineTo(375, 525),
                LineTo(167, 600),
                LineTo(0, 600),
                LineTo(0, 500),
                LineTo(0, 350),
                LineTo(0, 350),
                Close,
            ],
        )],
    );
}

#[test]
fn wedge_round_rect_callout_paths() {
    check(
        "wedgeRoundRectCallout",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 100),
                ArcTo { w_r: 100, h_r: 100, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(167, 0),
                LineTo(167, 0),
                LineTo(417, 0),
                LineTo(900, 0),
                ArcTo { w_r: 100, h_r: 100, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 350),
                LineTo(1000, 350),
                LineTo(1000, 500),
                LineTo(1000, 500),
                ArcTo { w_r: 100, h_r: 100, st_ang: 0, sw_ang: 5400000 },
                LineTo(417, 600),
                LineTo(292, 675),
                LineTo(167, 600),
                LineTo(100, 600),
                ArcTo { w_r: 100, h_r: 100, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 500),
                LineTo(0, 350),
                LineTo(0, 350),
                Close,
            ],
        )],
    );
    check(
        "wedgeRoundRectCallout",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 56),
                ArcTo { w_r: 56, h_r: 56, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(56, 0),
                LineTo(56, 0),
                LineTo(139, 0),
                LineTo(277, 0),
                ArcTo { w_r: 56, h_r: 56, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(333, 453),
                LineTo(333, 453),
                LineTo(333, 648),
                LineTo(333, 721),
                ArcTo { w_r: 56, h_r: 56, st_ang: 0, sw_ang: 5400000 },
                LineTo(139, 777),
                LineTo(97, 874),
                LineTo(56, 777),
                LineTo(56, 777),
                ArcTo { w_r: 56, h_r: 56, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 648),
                LineTo(0, 453),
                LineTo(0, 453),
                Close,
            ],
        )],
    );
    check(
        "wedgeRoundRectCallout",
        &[("adj1", "val -12499"), ("adj2", "val 37500"), ("adj3", "val 10000")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 60),
                ArcTo { w_r: 60, h_r: 60, st_ang: 10800000, sw_ang: 5400000 },
                LineTo(167, 0),
                LineTo(167, 0),
                LineTo(417, 0),
                LineTo(940, 0),
                ArcTo { w_r: 60, h_r: 60, st_ang: 16200000, sw_ang: 5400000 },
                LineTo(1000, 350),
                LineTo(1000, 350),
                LineTo(1000, 500),
                LineTo(1000, 540),
                ArcTo { w_r: 60, h_r: 60, st_ang: 0, sw_ang: 5400000 },
                LineTo(417, 600),
                LineTo(375, 525),
                LineTo(167, 600),
                LineTo(60, 600),
                ArcTo { w_r: 60, h_r: 60, st_ang: 5400000, sw_ang: 5400000 },
                LineTo(0, 500),
                LineTo(0, 350),
                LineTo(0, 350),
                Close,
            ],
        )],
    );
}

#[test]
fn wedge_ellipse_callout_paths() {
    check(
        "wedgeEllipseCallout",
        &[],
        1000,
        600,
        &[(true, true, &[MoveTo(292, 675), LineTo(254, 561), ArcTo { w_r: 500, h_r: 300, st_ang: 7994529, sw_ang: 19741675 }, Close])],
    );
    check(
        "wedgeEllipseCallout",
        &[],
        333,
        777,
        &[(true, true, &[MoveTo(97, 874), LineTo(85, 727), ArcTo { w_r: 166, h_r: 388, st_ang: 6215688, sw_ang: 20976369 }, Close])],
    );
    check(
        "wedgeEllipseCallout",
        &[("adj1", "val -12499"), ("adj2", "val 37500")],
        1000,
        600,
        &[(true, true, &[MoveTo(375, 525), LineTo(254, 561), ArcTo { w_r: 500, h_r: 300, st_ang: 7994452, sw_ang: 19741645 }, Close])],
    );
}
