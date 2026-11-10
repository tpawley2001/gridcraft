//! Plaque tabs (four corner tabs, one path each), the funnel and the six- and nine-tooth gears. Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
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

const PRESETS: [&str; 4] = ["plaqueTabs", "funnel", "gear6", "gear9"];

#[test]
fn gear_shape_points_stay_in_hostile_boxes() {
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
fn gear_shapes_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn gear_shape_adjustments_are_pinned() {
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
fn plaque_tabs_paths() {
    check(
        "plaqueTabs",
        &[],
        1000,
        600,
        &[
            (true, true, &[MoveTo(0, 0), LineTo(58, 0), ArcTo { w_r: 58, h_r: 58, st_ang: 0, sw_ang: 5400000 }, Close]),
            (true, true, &[MoveTo(0, 542), ArcTo { w_r: 58, h_r: 58, st_ang: 16200000, sw_ang: 5400000 }, LineTo(0, 600), Close]),
            (true, true, &[MoveTo(1000, 0), LineTo(1000, 58), ArcTo { w_r: 58, h_r: 58, st_ang: 5400000, sw_ang: 5400000 }, Close]),
            (true, true, &[MoveTo(942, 600), ArcTo { w_r: 58, h_r: 58, st_ang: 10800000, sw_ang: 5400000 }, LineTo(1000, 600), Close]),
        ],
    );
    check(
        "plaqueTabs",
        &[],
        333,
        777,
        &[
            (true, true, &[MoveTo(0, 0), LineTo(42, 0), ArcTo { w_r: 42, h_r: 42, st_ang: 0, sw_ang: 5400000 }, Close]),
            (true, true, &[MoveTo(0, 735), ArcTo { w_r: 42, h_r: 42, st_ang: 16200000, sw_ang: 5400000 }, LineTo(0, 777), Close]),
            (true, true, &[MoveTo(333, 0), LineTo(333, 42), ArcTo { w_r: 42, h_r: 42, st_ang: 5400000, sw_ang: 5400000 }, Close]),
            (true, true, &[MoveTo(291, 777), ArcTo { w_r: 42, h_r: 42, st_ang: 10800000, sw_ang: 5400000 }, LineTo(333, 777), Close]),
        ],
    );
}

#[test]
fn funnel_paths() {
    check(
        "funnel",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(5, 171),
                ArcTo { w_r: 500, h_r: 150, st_ang: 10655143, sw_ang: 11089715 },
                LineTo(624, 568),
                ArcTo { w_r: 125, h_r: 38, st_ang: 144857, sw_ang: 10510285 },
                Close,
                MoveTo(30, 150),
                ArcTo { w_r: 470, h_r: 120, st_ang: 10800000, sw_ang: -21600000 },
                Close,
            ],
        )],
    );
    check(
        "funnel",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(2, 221),
                ArcTo { w_r: 166, h_r: 194, st_ang: 10241304, sw_ang: 11917393 },
                LineTo(208, 735),
                ArcTo { w_r: 42, h_r: 49, st_ang: 558696, sw_ang: 9682607 },
                Close,
                MoveTo(17, 194),
                ArcTo { w_r: 150, h_r: 178, st_ang: 10800000, sw_ang: -21600000 },
                Close,
            ],
        )],
    );
}

#[test]
fn gear6_paths() {
    check(
        "gear6",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(791, 152),
                LineTo(866, 99),
                LineTo(940, 164),
                LineTo(896, 245),
                ArcTo { w_r: 410, h_r: 210, st_ang: -474573, sw_ang: 949145 },
                LineTo(940, 436),
                LineTo(866, 501),
                LineTo(791, 448),
                ArcTo { w_r: 410, h_r: 210, st_ang: 1618718, sw_ang: 2141727 },
                LineTo(584, 593),
                LineTo(416, 593),
                LineTo(395, 503),
                ArcTo { w_r: 410, h_r: 210, st_ang: 7039555, sw_ang: 2141727 },
                LineTo(134, 501),
                LineTo(60, 436),
                LineTo(104, 355),
                ArcTo { w_r: 410, h_r: 210, st_ang: 10325427, sw_ang: 949145 },
                LineTo(60, 164),
                LineTo(134, 99),
                LineTo(209, 152),
                ArcTo { w_r: 410, h_r: 210, st_ang: -9181282, sw_ang: 2141727 },
                LineTo(416, 7),
                LineTo(584, 7),
                LineTo(605, 97),
                ArcTo { w_r: 410, h_r: 210, st_ang: -3760445, sw_ang: 2141727 },
                Close,
            ],
        )],
    );
    check(
        "gear6",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(249, 150),
                LineTo(300, 152),
                LineTo(326, 279),
                LineTo(279, 300),
                ArcTo { w_r: 117, h_r: 339, st_ang: -2294089, sw_ang: 4588179 },
                LineTo(326, 498),
                LineTo(300, 625),
                LineTo(249, 627),
                ArcTo { w_r: 117, h_r: 339, st_ang: 4253680, sw_ang: 833965 },
                LineTo(185, 766),
                LineTo(148, 766),
                LineTo(137, 716),
                ArcTo { w_r: 117, h_r: 339, st_ang: 5712356, sw_ang: 833965 },
                LineTo(33, 625),
                LineTo(7, 498),
                LineTo(54, 477),
                ArcTo { w_r: 117, h_r: 339, st_ang: 8505911, sw_ang: 4588179 },
                LineTo(7, 279),
                LineTo(33, 152),
                LineTo(84, 150),
                ArcTo { w_r: 117, h_r: 339, st_ang: -6546320, sw_ang: 833965 },
                LineTo(148, 11),
                LineTo(185, 11),
                LineTo(196, 61),
                ArcTo { w_r: 117, h_r: 339, st_ang: -5087644, sw_ang: 833965 },
                Close,
            ],
        )],
    );
    check(
        "gear6",
        &[("adj1", "val 9000"), ("adj2", "val 2115")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(853, 149),
                LineTo(899, 119),
                LineTo(941, 159),
                LineTo(913, 207),
                ArcTo { w_r: 446, h_r: 246, st_ang: -764087, sw_ang: 1528175 },
                LineTo(941, 441),
                LineTo(899, 481),
                LineTo(853, 451),
                ArcTo { w_r: 446, h_r: 246, st_ang: 1386182, sw_ang: 3185405 },
                LineTo(547, 598),
                LineTo(453, 598),
                LineTo(440, 544),
                ArcTo { w_r: 446, h_r: 246, st_ang: 6228413, sw_ang: 3185405 },
                LineTo(101, 481),
                LineTo(59, 441),
                LineTo(87, 393),
                ArcTo { w_r: 446, h_r: 246, st_ang: 10035913, sw_ang: 1528175 },
                LineTo(59, 159),
                LineTo(101, 119),
                LineTo(147, 149),
                ArcTo { w_r: 446, h_r: 246, st_ang: -9413818, sw_ang: 3185405 },
                LineTo(453, 2),
                LineTo(547, 2),
                LineTo(560, 56),
                ArcTo { w_r: 446, h_r: 246, st_ang: -4571587, sw_ang: 3185405 },
                Close,
            ],
        )],
    );
}

#[test]
fn gear9_paths() {
    check(
        "gear9",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(731, 96),
                LineTo(765, 46),
                LineTo(844, 82),
                LineTo(829, 141),
                ArcTo { w_r: 440, h_r: 240, st_ang: -1552065, sw_ang: 936247 },
                LineTo(978, 216),
                LineTo(994, 265),
                LineTo(940, 293),
                ArcTo { w_r: 440, h_r: 240, st_ang: -53529, sw_ang: 785996 },
                LineTo(943, 439),
                LineTo(894, 485),
                LineTo(845, 449),
                ArcTo { w_r: 440, h_r: 240, st_ang: 1401208, sw_ang: 1316379 },
                LineTo(710, 572),
                LineTo(611, 592),
                LineTo(589, 535),
                ArcTo { w_r: 440, h_r: 240, st_ang: 4159088, sw_ang: 2481824 },
                LineTo(389, 592),
                LineTo(290, 572),
                LineTo(291, 511),
                ArcTo { w_r: 440, h_r: 240, st_ang: 8082412, sw_ang: 1316379 },
                LineTo(106, 485),
                LineTo(57, 439),
                LineTo(91, 388),
                ArcTo { w_r: 440, h_r: 240, st_ang: 10067534, sw_ang: 785996 },
                LineTo(6, 265),
                LineTo(22, 216),
                LineTo(82, 224),
                ArcTo { w_r: 440, h_r: 240, st_ang: -10184182, sw_ang: 936247 },
                LineTo(156, 82),
                LineTo(235, 46),
                LineTo(269, 96),
                ArcTo { w_r: 440, h_r: 240, st_ang: -8308698, sw_ang: 2003506 },
                LineTo(447, 3),
                LineTo(553, 3),
                LineTo(564, 63),
                ArcTo { w_r: 440, h_r: 240, st_ang: -4494807, sw_ang: 2003506 },
                Close,
            ],
        )],
    );
    check(
        "gear9",
        &[],
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(236, 86),
                LineTo(269, 78),
                LineTo(294, 134),
                LineTo(266, 153),
                ArcTo { w_r: 133, h_r: 355, st_ang: -4027657, sw_ang: 1536631 },
                LineTo(327, 280),
                LineTo(332, 370),
                LineTo(300, 378),
                ArcTo { w_r: 133, h_r: 355, st_ang: -261217, sw_ang: 3057605 },
                LineTo(322, 532),
                LineTo(305, 610),
                LineTo(271, 609),
                ArcTo { w_r: 133, h_r: 355, st_ang: 3879096, sw_ang: 834170 },
                LineTo(249, 729),
                LineTo(221, 756),
                LineTo(193, 736),
                ArcTo { w_r: 133, h_r: 355, st_ang: 5135072, sw_ang: 529856 },
                LineTo(112, 756),
                LineTo(84, 729),
                LineTo(103, 701),
                ArcTo { w_r: 133, h_r: 355, st_ang: 6086734, sw_ang: 834170 },
                LineTo(28, 610),
                LineTo(11, 532),
                LineTo(43, 519),
                ArcTo { w_r: 133, h_r: 355, st_ang: 8003612, sw_ang: 3057605 },
                LineTo(1, 370),
                LineTo(6, 280),
                LineTo(40, 277),
                ArcTo { w_r: 133, h_r: 355, st_ang: -8308975, sw_ang: 1536631 },
                LineTo(39, 134),
                LineTo(64, 78),
                LineTo(97, 86),
                ArcTo { w_r: 133, h_r: 355, st_ang: -6180496, sw_ang: 591134 },
                LineTo(153, 4),
                LineTo(180, 4),
                LineTo(186, 37),
                ArcTo { w_r: 133, h_r: 355, st_ang: -5210638, sw_ang: 591134 },
                Close,
            ],
        )],
    );
    check(
        "gear9",
        &[("adj1", "val 6000"), ("adj2", "val 1057")],
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(769, 85),
                LineTo(790, 55),
                LineTo(835, 77),
                LineTo(826, 112),
                ArcTo { w_r: 464, h_r: 264, st_ang: -1799792, sw_ang: 1294522 },
                LineTo(985, 229),
                LineTo(994, 258),
                LineTo(962, 275),
                ArcTo { w_r: 464, h_r: 264, st_ang: -185233, sw_ang: 1092923 },
                LineTo(940, 443),
                LineTo(912, 471),
                LineTo(882, 450),
                ArcTo { w_r: 464, h_r: 264, st_ang: 1285151, sw_ang: 1786638 },
                LineTo(694, 577),
                LineTo(637, 588),
                LineTo(623, 555),
                ArcTo { w_r: 464, h_r: 264, st_ang: 3848669, sw_ang: 3102662 },
                LineTo(363, 588),
                LineTo(306, 577),
                LineTo(307, 540),
                ArcTo { w_r: 464, h_r: 264, st_ang: 7728211, sw_ang: 1786638 },
                LineTo(88, 471),
                LineTo(60, 443),
                LineTo(81, 413),
                ArcTo { w_r: 464, h_r: 264, st_ang: 9892310, sw_ang: 1092923 },
                LineTo(6, 258),
                LineTo(15, 229),
                LineTo(51, 234),
                ArcTo { w_r: 464, h_r: 264, st_ang: -10294730, sw_ang: 1294522 },
                LineTo(165, 77),
                LineTo(210, 55),
                LineTo(231, 85),
                ArcTo { w_r: 464, h_r: 264, st_ang: -8480063, sw_ang: 2598699 },
                LineTo(469, 1),
                LineTo(531, 1),
                LineTo(537, 37),
                ArcTo { w_r: 464, h_r: 264, st_ang: -4918636, sw_ang: 2598699 },
                Close,
            ],
        )],
    );
}
