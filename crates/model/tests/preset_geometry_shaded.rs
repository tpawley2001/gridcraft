//! Presets with shaded faces or many sub-paths: can (cylinder), folded corner, bevel, sun and cloud.
//! A path's `fill="lighten" | "lightenLess" | "darken" | "darkenLess"` (ECMA-376 Part 1 §20.1.9.15,
//! ST_PathFillMode) fills that face with the shape's fill shaded, `fill="none"` draws no fill and
//! `stroke="false"` no outline. Paths and guides follow §20.1.9.18 (`prstGeom`) and its preset shape
//! definitions; `a:arcTo` (§20.1.9.3) is kept as `ArcTo` with its radii scaled from the path's own box
//! (`<path w h>`) to the shape box. Expected values are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo};
use gridcraft_model::PathShade::{self, Darken, DarkenLess, Lighten, LightenLess, Norm};
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

const PRESETS: [&str; 5] = ["can", "foldedCorner", "bevel", "sun", "cloud"];

#[test]
fn shaded_preset_points_stay_in_hostile_boxes() {
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
fn shaded_presets_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn shaded_preset_adjustments_are_pinned() {
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
fn can_paths() {
    check(
        "can",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 75),
                    ArcTo { w_r: 500, h_r: 75, st_ang: 10800000, sw_ang: -10800000 },
                    LineTo(1000, 525),
                    ArcTo { w_r: 500, h_r: 75, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (
                true,
                false,
                Lighten,
                &[
                    MoveTo(0, 75),
                    ArcTo { w_r: 500, h_r: 75, st_ang: 10800000, sw_ang: 10800000 },
                    ArcTo { w_r: 500, h_r: 75, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(1000, 75),
                    ArcTo { w_r: 500, h_r: 75, st_ang: 0, sw_ang: 10800000 },
                    ArcTo { w_r: 500, h_r: 75, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(1000, 525),
                    ArcTo { w_r: 500, h_r: 75, st_ang: 0, sw_ang: 10800000 },
                    LineTo(0, 75),
                ],
            ),
        ],
    );
    check(
        "can",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 42),
                    ArcTo { w_r: 166, h_r: 42, st_ang: 10800000, sw_ang: -10800000 },
                    LineTo(333, 735),
                    ArcTo { w_r: 166, h_r: 42, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (
                true,
                false,
                Lighten,
                &[
                    MoveTo(0, 42),
                    ArcTo { w_r: 166, h_r: 42, st_ang: 10800000, sw_ang: 10800000 },
                    ArcTo { w_r: 166, h_r: 42, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(333, 42),
                    ArcTo { w_r: 166, h_r: 42, st_ang: 0, sw_ang: 10800000 },
                    ArcTo { w_r: 166, h_r: 42, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(333, 735),
                    ArcTo { w_r: 166, h_r: 42, st_ang: 0, sw_ang: 10800000 },
                    LineTo(0, 42),
                ],
            ),
        ],
    );
    check(
        "can",
        &[("adj", "val 15000")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 45),
                    ArcTo { w_r: 500, h_r: 45, st_ang: 10800000, sw_ang: -10800000 },
                    LineTo(1000, 555),
                    ArcTo { w_r: 500, h_r: 45, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (
                true,
                false,
                Lighten,
                &[
                    MoveTo(0, 45),
                    ArcTo { w_r: 500, h_r: 45, st_ang: 10800000, sw_ang: 10800000 },
                    ArcTo { w_r: 500, h_r: 45, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(1000, 45),
                    ArcTo { w_r: 500, h_r: 45, st_ang: 0, sw_ang: 10800000 },
                    ArcTo { w_r: 500, h_r: 45, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(1000, 555),
                    ArcTo { w_r: 500, h_r: 45, st_ang: 0, sw_ang: 10800000 },
                    LineTo(0, 45),
                ],
            ),
        ],
    );
}

#[test]
fn folded_corner_paths() {
    check(
        "foldedCorner",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 500), LineTo(900, 600), LineTo(0, 600), Close]),
            (true, false, DarkenLess, &[MoveTo(900, 600), LineTo(920, 520), LineTo(1000, 500), Close]),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(900, 600),
                    LineTo(920, 520),
                    LineTo(1000, 500),
                    LineTo(900, 600),
                    LineTo(0, 600),
                    LineTo(0, 0),
                    LineTo(1000, 0),
                    LineTo(1000, 500),
                ],
            ),
        ],
    );
    check(
        "foldedCorner",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 721), LineTo(277, 777), LineTo(0, 777), Close]),
            (true, false, DarkenLess, &[MoveTo(277, 777), LineTo(289, 733), LineTo(333, 721), Close]),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(277, 777),
                    LineTo(289, 733),
                    LineTo(333, 721),
                    LineTo(277, 777),
                    LineTo(0, 777),
                    LineTo(0, 0),
                    LineTo(333, 0),
                    LineTo(333, 721),
                ],
            ),
        ],
    );
    check(
        "foldedCorner",
        &[("adj", "val 10000")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 540), LineTo(940, 600), LineTo(0, 600), Close]),
            (true, false, DarkenLess, &[MoveTo(940, 600), LineTo(952, 552), LineTo(1000, 540), Close]),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(940, 600),
                    LineTo(952, 552),
                    LineTo(1000, 540),
                    LineTo(940, 600),
                    LineTo(0, 600),
                    LineTo(0, 0),
                    LineTo(1000, 0),
                    LineTo(1000, 540),
                ],
            ),
        ],
    );
}

#[test]
fn bevel_paths() {
    check(
        "bevel",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(75, 75), LineTo(925, 75), LineTo(925, 525), LineTo(75, 525), Close]),
            (true, false, LightenLess, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(925, 75), LineTo(75, 75), Close]),
            (true, false, DarkenLess, &[MoveTo(0, 600), LineTo(75, 525), LineTo(925, 525), LineTo(1000, 600), Close]),
            (true, false, Lighten, &[MoveTo(0, 0), LineTo(75, 75), LineTo(75, 525), LineTo(0, 600), Close]),
            (true, false, Darken, &[MoveTo(1000, 0), LineTo(1000, 600), LineTo(925, 525), LineTo(925, 75), Close]),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 0),
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(75, 75),
                    LineTo(925, 75),
                    LineTo(925, 525),
                    LineTo(75, 525),
                    Close,
                    MoveTo(0, 0),
                    LineTo(75, 75),
                    MoveTo(0, 600),
                    LineTo(75, 525),
                    MoveTo(1000, 0),
                    LineTo(925, 75),
                    MoveTo(1000, 600),
                    LineTo(925, 525),
                ],
            ),
        ],
    );
    check(
        "bevel",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(42, 42), LineTo(291, 42), LineTo(291, 735), LineTo(42, 735), Close]),
            (true, false, LightenLess, &[MoveTo(0, 0), LineTo(333, 0), LineTo(291, 42), LineTo(42, 42), Close]),
            (true, false, DarkenLess, &[MoveTo(0, 777), LineTo(42, 735), LineTo(291, 735), LineTo(333, 777), Close]),
            (true, false, Lighten, &[MoveTo(0, 0), LineTo(42, 42), LineTo(42, 735), LineTo(0, 777), Close]),
            (true, false, Darken, &[MoveTo(333, 0), LineTo(333, 777), LineTo(291, 735), LineTo(291, 42), Close]),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 0),
                    LineTo(333, 0),
                    LineTo(333, 777),
                    LineTo(0, 777),
                    Close,
                    MoveTo(42, 42),
                    LineTo(291, 42),
                    LineTo(291, 735),
                    LineTo(42, 735),
                    Close,
                    MoveTo(0, 0),
                    LineTo(42, 42),
                    MoveTo(0, 777),
                    LineTo(42, 735),
                    MoveTo(333, 0),
                    LineTo(291, 42),
                    MoveTo(333, 777),
                    LineTo(291, 735),
                ],
            ),
        ],
    );
    check(
        "bevel",
        &[("adj", "val 7500")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(45, 45), LineTo(955, 45), LineTo(955, 555), LineTo(45, 555), Close]),
            (true, false, LightenLess, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(955, 45), LineTo(45, 45), Close]),
            (true, false, DarkenLess, &[MoveTo(0, 600), LineTo(45, 555), LineTo(955, 555), LineTo(1000, 600), Close]),
            (true, false, Lighten, &[MoveTo(0, 0), LineTo(45, 45), LineTo(45, 555), LineTo(0, 600), Close]),
            (true, false, Darken, &[MoveTo(1000, 0), LineTo(1000, 600), LineTo(955, 555), LineTo(955, 45), Close]),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 0),
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(45, 45),
                    LineTo(955, 45),
                    LineTo(955, 555),
                    LineTo(45, 555),
                    Close,
                    MoveTo(0, 0),
                    LineTo(45, 45),
                    MoveTo(0, 600),
                    LineTo(45, 555),
                    MoveTo(1000, 0),
                    LineTo(955, 45),
                    MoveTo(1000, 600),
                    LineTo(955, 555),
                ],
            ),
        ],
    );
}

#[test]
fn sun_paths() {
    check(
        "sun",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(1000, 300),
                LineTo(798, 343),
                LineTo(798, 257),
                Close,
                MoveTo(854, 88),
                LineTo(762, 204),
                LineTo(660, 143),
                Close,
                MoveTo(500, 0),
                LineTo(572, 121),
                LineTo(428, 121),
                Close,
                MoveTo(146, 88),
                LineTo(340, 143),
                LineTo(238, 204),
                Close,
                MoveTo(0, 300),
                LineTo(202, 257),
                LineTo(202, 343),
                Close,
                MoveTo(146, 512),
                LineTo(238, 396),
                LineTo(340, 457),
                Close,
                MoveTo(500, 600),
                LineTo(428, 479),
                LineTo(572, 479),
                Close,
                MoveTo(854, 512),
                LineTo(660, 457),
                LineTo(762, 396),
                Close,
                MoveTo(250, 300),
                ArcTo { w_r: 250, h_r: 150, st_ang: 10800000, sw_ang: 21600000 },
                Close,
            ],
        )],
    );
    check(
        "sun",
        &[],
        333,
        777,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(333, 388),
                LineTo(266, 444),
                LineTo(266, 333),
                Close,
                MoveTo(284, 114),
                LineTo(254, 264),
                LineTo(220, 185),
                Close,
                MoveTo(166, 0),
                LineTo(190, 157),
                LineTo(143, 157),
                Close,
                MoveTo(49, 114),
                LineTo(113, 185),
                LineTo(79, 264),
                Close,
                MoveTo(0, 388),
                LineTo(67, 333),
                LineTo(67, 444),
                Close,
                MoveTo(49, 663),
                LineTo(79, 513),
                LineTo(113, 592),
                Close,
                MoveTo(166, 777),
                LineTo(143, 620),
                LineTo(190, 620),
                Close,
                MoveTo(284, 663),
                LineTo(220, 592),
                LineTo(254, 513),
                Close,
                MoveTo(83, 388),
                ArcTo { w_r: 83, h_r: 194, st_ang: 10800000, sw_ang: 21600000 },
                Close,
            ],
        )],
    );
    check(
        "sun",
        &[("adj", "val 15000")],
        1000,
        600,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(1000, 300),
                LineTo(868, 360),
                LineTo(868, 240),
                Close,
                MoveTo(854, 88),
                LineTo(831, 187),
                LineTo(689, 101),
                Close,
                MoveTo(500, 0),
                LineTo(600, 79),
                LineTo(400, 79),
                Close,
                MoveTo(146, 88),
                LineTo(311, 101),
                LineTo(169, 187),
                Close,
                MoveTo(0, 300),
                LineTo(132, 240),
                LineTo(132, 360),
                Close,
                MoveTo(146, 512),
                LineTo(169, 413),
                LineTo(311, 499),
                Close,
                MoveTo(500, 600),
                LineTo(400, 521),
                LineTo(600, 521),
                Close,
                MoveTo(854, 512),
                LineTo(689, 499),
                LineTo(831, 413),
                Close,
                MoveTo(150, 300),
                ArcTo { w_r: 350, h_r: 210, st_ang: 10800000, sw_ang: 21600000 },
                Close,
            ],
        )],
    );
}

#[test]
fn cloud_paths() {
    check(
        "cloud",
        &[],
        1000,
        600,
        &[
            (
                true,
                true,
                Norm,
                &[
                    MoveTo(90, 200),
                    ArcTo { w_r: 156, h_r: 128, st_ang: -11429249, sw_ang: 7426832 },
                    ArcTo { w_r: 123, h_r: 101, st_ang: -8646143, sw_ang: 5396714 },
                    ArcTo { w_r: 101, h_r: 83, st_ang: -8748475, sw_ang: 5983381 },
                    ArcTo { w_r: 112, h_r: 92, st_ang: -7859164, sw_ang: 7034504 },
                    ArcTo { w_r: 123, h_r: 101, st_ang: -4722533, sw_ang: 6541615 },
                    ArcTo { w_r: 157, h_r: 128, st_ang: -2776035, sw_ang: 7816140 },
                    ArcTo { w_r: 134, h_r: 109, st_ang: 37501, sw_ang: 6842000 },
                    ArcTo { w_r: 156, h_r: 128, st_ang: 1347096, sw_ang: 6910353 },
                    ArcTo { w_r: 179, h_r: 146, st_ang: 3974558, sw_ang: 4542661 },
                    ArcTo { w_r: 101, h_r: 82, st_ang: -16496525, sw_ang: 8804134 },
                    ArcTo { w_r: 101, h_r: 83, st_ang: -14809710, sw_ang: 9151131 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(109, 364),
                    ArcTo { w_r: 101, h_r: 83, st_ang: 5204520, sw_ang: 1585770 },
                    MoveTo(160, 485),
                    ArcTo { w_r: 101, h_r: 82, st_ang: 4416628, sw_ang: 686848 },
                    MoveTo(381, 543),
                    ArcTo { w_r: 156, h_r: 128, st_ang: 8257449, sw_ang: 844866 },
                    MoveTo(667, 483),
                    ArcTo { w_r: 156, h_r: 128, st_ang: 387196, sw_ang: 959901 },
                    MoveTo(790, 319),
                    ArcTo { w_r: 134, h_r: 109, st_ang: -4217541, sw_ang: 4255042 },
                    MoveTo(968, 213),
                    ArcTo { w_r: 123, h_r: 101, st_ang: 1819082, sw_ang: 1665090 },
                    MoveTo(887, 75),
                    ArcTo { w_r: 112, h_r: 92, st_ang: -824660, sw_ang: 891534 },
                    MoveTo(673, 55),
                    ArcTo { w_r: 112, h_r: 92, st_ang: -8950887, sw_ang: 1091722 },
                    MoveTo(513, 66),
                    ArcTo { w_r: 101, h_r: 83, st_ang: -9809656, sw_ang: 1061181 },
                    MoveTo(324, 72),
                    ArcTo { w_r: 156, h_r: 128, st_ang: -4002417, sw_ang: 739161 },
                    MoveTo(96, 219),
                    ArcTo { w_r: 156, h_r: 128, st_ang: 9459261, sw_ang: 711490 },
                ],
            ),
        ],
    );
    check(
        "cloud",
        &[],
        333,
        777,
        &[
            (
                true,
                true,
                Norm,
                &[
                    MoveTo(30, 258),
                    ArcTo { w_r: 52, h_r: 165, st_ang: -11429249, sw_ang: 7426832 },
                    ArcTo { w_r: 41, h_r: 131, st_ang: -8646143, sw_ang: 5396714 },
                    ArcTo { w_r: 34, h_r: 107, st_ang: -8748475, sw_ang: 5983381 },
                    ArcTo { w_r: 37, h_r: 119, st_ang: -7859164, sw_ang: 7034504 },
                    ArcTo { w_r: 41, h_r: 131, st_ang: -4722533, sw_ang: 6541615 },
                    ArcTo { w_r: 52, h_r: 166, st_ang: -2776035, sw_ang: 7816140 },
                    ArcTo { w_r: 45, h_r: 141, st_ang: 37501, sw_ang: 6842000 },
                    ArcTo { w_r: 52, h_r: 166, st_ang: 1347096, sw_ang: 6910353 },
                    ArcTo { w_r: 60, h_r: 190, st_ang: 3974558, sw_ang: 4542661 },
                    ArcTo { w_r: 34, h_r: 106, st_ang: -16496525, sw_ang: 8804134 },
                    ArcTo { w_r: 33, h_r: 107, st_ang: -14809710, sw_ang: 9151131 },
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(36, 471),
                    ArcTo { w_r: 33, h_r: 107, st_ang: 5204520, sw_ang: 1585770 },
                    MoveTo(53, 628),
                    ArcTo { w_r: 34, h_r: 106, st_ang: 4416628, sw_ang: 686848 },
                    MoveTo(127, 703),
                    ArcTo { w_r: 52, h_r: 166, st_ang: 8257449, sw_ang: 844866 },
                    MoveTo(222, 625),
                    ArcTo { w_r: 52, h_r: 166, st_ang: 387196, sw_ang: 959901 },
                    MoveTo(263, 413),
                    ArcTo { w_r: 45, h_r: 141, st_ang: -4217541, sw_ang: 4255042 },
                    MoveTo(322, 276),
                    ArcTo { w_r: 41, h_r: 131, st_ang: 1819082, sw_ang: 1665090 },
                    MoveTo(295, 98),
                    ArcTo { w_r: 37, h_r: 119, st_ang: -824660, sw_ang: 891534 },
                    MoveTo(224, 71),
                    ArcTo { w_r: 37, h_r: 119, st_ang: -8950887, sw_ang: 1091722 },
                    MoveTo(171, 85),
                    ArcTo { w_r: 34, h_r: 107, st_ang: -9809656, sw_ang: 1061181 },
                    MoveTo(108, 93),
                    ArcTo { w_r: 52, h_r: 165, st_ang: -4002417, sw_ang: 739161 },
                    MoveTo(32, 284),
                    ArcTo { w_r: 52, h_r: 165, st_ang: 9459261, sw_ang: 711490 },
                ],
            ),
        ],
    );
}
