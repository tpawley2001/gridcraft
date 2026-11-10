//! Curved presets drawn with cubic Béziers (`a:cubicBezTo`, ECMA-376 Part 1 §20.1.9.7): heart, wave
//! and double wave. Control points may lie outside the shape box (the heart's lobes) and are kept
//! as the definition gives them; on-curve points stay in the box. Paths and guides follow §20.1.9.18
//! (`prstGeom`) and its preset shape definitions; expected values are the definitions evaluated in
//! the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, CubicBezTo, LineTo, MoveTo};
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
        (CubicBezTo(p1), CubicBezTo(p2)) => p1.iter().zip(p2).all(|(a, b)| d(a.0, b.0, 2) && d(a.1, b.1, 2)),
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

const PRESETS: [&str; 3] = ["heart", "wave", "doubleWave"];

#[test]
fn wave_and_heart_points_stay_in_hostile_boxes() {
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
fn wave_and_heart_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn wave_adjustments_are_pinned() {
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
fn heart_paths() {
    check(
        "heart",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[MoveTo(500, 150), CubicBezTo([(708, -200), (1521, 150), (500, 600)]), CubicBezTo([(-521, 150), (292, -200), (500, 150)]), Close],
        )],
    );
    check(
        "heart",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[MoveTo(166, 194), CubicBezTo([(236, -259), (506, 194), (166, 777)]), CubicBezTo([(-173, 194), (97, -259), (166, 194)]), Close],
        )],
    );
}

#[test]
fn wave_paths() {
    check(
        "wave",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 75),
                CubicBezTo([(333, -175), (667, 325), (1000, 75)]),
                LineTo(1000, 525),
                CubicBezTo([(667, 775), (333, 275), (0, 525)]),
                Close,
            ],
        )],
    );
    check(
        "wave",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 97),
                CubicBezTo([(111, -227), (222, 421), (333, 97)]),
                LineTo(333, 680),
                CubicBezTo([(222, 1004), (111, 356), (0, 680)]),
                Close,
            ],
        )],
    );
    check(
        "wave",
        &[("adj1", "val 7500"), ("adj2", "val 0")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 45),
                CubicBezTo([(333, -105), (667, 195), (1000, 45)]),
                LineTo(1000, 555),
                CubicBezTo([(667, 705), (333, 405), (0, 555)]),
                Close,
            ],
        )],
    );
}

#[test]
fn double_wave_paths() {
    check(
        "doubleWave",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 38),
                CubicBezTo([(167, -88), (333, 162), (500, 38)]),
                CubicBezTo([(667, -88), (833, 162), (1000, 38)]),
                LineTo(1000, 562),
                CubicBezTo([(833, 688), (667, 438), (500, 562)]),
                CubicBezTo([(333, 688), (167, 438), (0, 562)]),
                Close,
            ],
        )],
    );
    check(
        "doubleWave",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 49),
                CubicBezTo([(56, -113), (111, 210), (166, 49)]),
                CubicBezTo([(222, -113), (278, 210), (333, 49)]),
                LineTo(333, 728),
                CubicBezTo([(278, 890), (222, 567), (166, 728)]),
                CubicBezTo([(111, 890), (56, 567), (0, 728)]),
                Close,
            ],
        )],
    );
    check(
        "doubleWave",
        &[("adj1", "val 3750"), ("adj2", "val 0")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 22),
                CubicBezTo([(167, -52), (333, 98), (500, 22)]),
                CubicBezTo([(667, -52), (833, 98), (1000, 22)]),
                LineTo(1000, 578),
                CubicBezTo([(833, 652), (667, 502), (500, 578)]),
                CubicBezTo([(333, 652), (167, 502), (0, 578)]),
                Close,
            ],
        )],
    );
}
