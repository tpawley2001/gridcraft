//! Curved ribbons (`ellipseRibbon`, `ellipseRibbon2`), `moon`, `cloudCallout` and `swooshArrow`. Each keeps a value the
//! definition puts outside the shape box, as Excel does: the ribbons' quadratic Bézier control points (`a:quadBezTo`,
//! ECMA-376 Part 1 §20.1.9.21) just below or above the box, the moon's inner arc (`a:arcTo`, §20.1.9.3) with radii larger
//! than the box, and the cloud callout's thought bubbles placed by `adj1`/`adj2` below the box. Values are bounded to ten
//! box sizes from the centre. Paths and guides follow §20.1.9.18 (`prstGeom`) and its preset shape definitions; a path's
//! `fill="darkenLess"` (§20.1.9.15) is `PathShade::DarkenLess`; arc radii and points of a `<path w h>` are scaled to the
//! shape box. Expected values are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo, QuadBezTo};
use gridcraft_model::PathShade::{self, DarkenLess, Norm};
use gridcraft_model::{ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    let d = |x: i64, y: i64, tol: i64| (x - y).abs() <= tol;
    let pt = |p: &(i64, i64), q: &(i64, i64)| d(p.0, q.0, 2) && d(p.1, q.1, 2);
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => d(*x1, *x2, 2) && d(*y1, *y2, 2),
        (QuadBezTo(p), QuadBezTo(q)) => p.iter().zip(q).all(|(p, q)| pt(p, q)),
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

const PRESETS: [&str; 5] = ["ellipseRibbon", "ellipseRibbon2", "moon", "cloudCallout", "swooshArrow"];

fn points(paths: &[gridcraft_model::GeometryPath]) -> Vec<(i64, i64)> {
    let mut out = vec![];
    for c in paths.iter().flat_map(|p| &p.commands) {
        match c {
            MoveTo(x, y) | LineTo(x, y) => out.push((*x, *y)),
            QuadBezTo(p) => out.extend_from_slice(p),
            PathCommand::CubicBezTo(p) => out.extend_from_slice(p),
            _ => {}
        }
    }
    out
}

#[test]
fn values_outside_the_box_are_kept() {
    let ribbon = points(&preset_paths(&geom("ellipseRibbon", &[]), 1000, 600).expect("ellipseRibbon"));
    assert!(ribbon.iter().any(|&(_, y)| y > 600), "ellipseRibbon: a control point below the box is kept: {ribbon:?}");
    let ribbon2 = points(&preset_paths(&geom("ellipseRibbon2", &[]), 1000, 600).expect("ellipseRibbon2"));
    assert!(ribbon2.iter().any(|&(_, y)| y < 0), "ellipseRibbon2: a control point above the box is kept: {ribbon2:?}");
    let cloud = points(&preset_paths(&geom("cloudCallout", &[]), 1000, 600).expect("cloudCallout"));
    assert!(cloud.iter().any(|&(_, y)| y > 640), "cloudCallout: the default bubbles lie below the box: {cloud:?}");
    let moon = preset_paths(&geom("moon", &[]), 1000, 600).expect("moon");
    let big = moon.iter().flat_map(|p| &p.commands).any(|c| matches!(*c, ArcTo { w_r, .. } if w_r > 1000));
    assert!(big, "moon: the inner arc's radius is larger than the box: {moon:?}");
}

#[test]
fn hostile_boxes_and_adjustments_stay_within_ten_box_sizes() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            assert!(preset_paths(&geom(preset, &[]), w, h).is_some(), "{preset} in {w}x{h}");
        }
        for name in ["adj", "adj1", "adj2", "adj3", "adj4"] {
            for v in ["-9223372036854775808", "-5", "0", "9223372036854775807", "21600000", "-21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for (x, y) in points(&paths) {
                    assert!((-9_500..=10_500).contains(&x) && (-6_650..=7_350).contains(&y), "{preset} {name}={v}: point ({x}, {y})");
                }
                for c in paths.iter().flat_map(|p| &p.commands) {
                    if let ArcTo { w_r, h_r, .. } = *c {
                        assert!((0..=10_000).contains(&w_r) && (0..=7_000).contains(&h_r), "{preset} {name}={v}: radii {w_r}, {h_r}");
                    }
                }
            }
        }
    }
}

#[test]
fn curved_outside_presets_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn ellipse_ribbon_paths() {
    check(
        "ellipseRibbon",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 0),
                    QuadBezTo([(188, 56), (375, 70)]),
                    LineTo(250, 131),
                    QuadBezTo([(500, 169), (750, 131)]),
                    LineTo(625, 70),
                    QuadBezTo([(812, 56), (1000, 0)]),
                    LineTo(875, 258),
                    LineTo(1000, 450),
                    QuadBezTo([(875, 488), (750, 506)]),
                    LineTo(750, 581),
                    QuadBezTo([(500, 619), (250, 581)]),
                    LineTo(250, 506),
                    QuadBezTo([(125, 488), (0, 450)]),
                    LineTo(125, 258),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(375, 145),
                    LineTo(375, 70),
                    LineTo(250, 131),
                    QuadBezTo([(500, 169), (750, 131)]),
                    LineTo(625, 70),
                    LineTo(625, 145),
                    QuadBezTo([(500, 155), (375, 145)]),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 0),
                    QuadBezTo([(188, 56), (375, 70)]),
                    LineTo(250, 131),
                    QuadBezTo([(500, 169), (750, 131)]),
                    LineTo(625, 70),
                    QuadBezTo([(812, 56), (1000, 0)]),
                    LineTo(875, 258),
                    LineTo(1000, 450),
                    QuadBezTo([(875, 488), (750, 506)]),
                    LineTo(750, 581),
                    QuadBezTo([(500, 619), (250, 581)]),
                    LineTo(250, 506),
                    QuadBezTo([(125, 488), (0, 450)]),
                    LineTo(125, 258),
                    Close,
                    MoveTo(250, 506),
                    LineTo(250, 131),
                    MoveTo(750, 131),
                    LineTo(750, 506),
                    MoveTo(375, 70),
                    LineTo(375, 145),
                    MoveTo(625, 145),
                    LineTo(625, 70),
                ],
            ),
        ],
    );
    check(
        "ellipseRibbon",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 0),
                    QuadBezTo([(62, 73), (125, 91)]),
                    LineTo(83, 170),
                    QuadBezTo([(166, 219), (250, 170)]),
                    LineTo(208, 91),
                    QuadBezTo([(271, 73), (333, 0)]),
                    LineTo(291, 334),
                    LineTo(333, 583),
                    QuadBezTo([(291, 631), (250, 656)]),
                    LineTo(250, 753),
                    QuadBezTo([(166, 801), (83, 753)]),
                    LineTo(83, 656),
                    QuadBezTo([(42, 631), (0, 583)]),
                    LineTo(42, 334),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(125, 188),
                    LineTo(125, 91),
                    LineTo(83, 170),
                    QuadBezTo([(166, 219), (250, 170)]),
                    LineTo(208, 91),
                    LineTo(208, 188),
                    QuadBezTo([(166, 200), (125, 188)]),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 0),
                    QuadBezTo([(62, 73), (125, 91)]),
                    LineTo(83, 170),
                    QuadBezTo([(166, 219), (250, 170)]),
                    LineTo(208, 91),
                    QuadBezTo([(271, 73), (333, 0)]),
                    LineTo(291, 334),
                    LineTo(333, 583),
                    QuadBezTo([(291, 631), (250, 656)]),
                    LineTo(250, 753),
                    QuadBezTo([(166, 801), (83, 753)]),
                    LineTo(83, 656),
                    QuadBezTo([(42, 631), (0, 583)]),
                    LineTo(42, 334),
                    Close,
                    MoveTo(83, 656),
                    LineTo(83, 170),
                    MoveTo(250, 170),
                    LineTo(250, 656),
                    MoveTo(125, 91),
                    LineTo(125, 188),
                    MoveTo(208, 188),
                    LineTo(208, 91),
                ],
            ),
        ],
    );
    check(
        "ellipseRibbon",
        &[("adj1", "val 15000"), ("adj2", "val 30000"), ("adj3", "val 7500")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 0),
                    QuadBezTo([(238, 43), (475, 45)]),
                    LineTo(350, 86),
                    QuadBezTo([(500, 94), (650, 86)]),
                    LineTo(525, 45),
                    QuadBezTo([(762, 43), (1000, 0)]),
                    LineTo(875, 275),
                    LineTo(1000, 510),
                    QuadBezTo([(825, 542), (650, 551)]),
                    LineTo(650, 596),
                    QuadBezTo([(500, 604), (350, 596)]),
                    LineTo(350, 551),
                    QuadBezTo([(175, 542), (0, 510)]),
                    LineTo(125, 275),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(475, 90),
                    LineTo(475, 45),
                    LineTo(350, 86),
                    QuadBezTo([(500, 94), (650, 86)]),
                    LineTo(525, 45),
                    LineTo(525, 90),
                    QuadBezTo([(500, 90), (475, 90)]),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 0),
                    QuadBezTo([(238, 43), (475, 45)]),
                    LineTo(350, 86),
                    QuadBezTo([(500, 94), (650, 86)]),
                    LineTo(525, 45),
                    QuadBezTo([(762, 43), (1000, 0)]),
                    LineTo(875, 275),
                    LineTo(1000, 510),
                    QuadBezTo([(825, 542), (650, 551)]),
                    LineTo(650, 596),
                    QuadBezTo([(500, 604), (350, 596)]),
                    LineTo(350, 551),
                    QuadBezTo([(175, 542), (0, 510)]),
                    LineTo(125, 275),
                    Close,
                    MoveTo(350, 551),
                    LineTo(350, 86),
                    MoveTo(650, 86),
                    LineTo(650, 551),
                    MoveTo(475, 45),
                    LineTo(475, 90),
                    MoveTo(525, 90),
                    LineTo(525, 45),
                ],
            ),
        ],
    );
}

#[test]
fn ellipse_ribbon2_paths() {
    check(
        "ellipseRibbon2",
        &[],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 600),
                    QuadBezTo([(188, 544), (375, 530)]),
                    LineTo(250, 469),
                    QuadBezTo([(500, 431), (750, 469)]),
                    LineTo(625, 530),
                    QuadBezTo([(812, 544), (1000, 600)]),
                    LineTo(875, 342),
                    LineTo(1000, 150),
                    QuadBezTo([(875, 112), (750, 94)]),
                    LineTo(750, 19),
                    QuadBezTo([(500, -19), (250, 19)]),
                    LineTo(250, 94),
                    QuadBezTo([(125, 112), (0, 150)]),
                    LineTo(125, 342),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(375, 455),
                    LineTo(375, 530),
                    LineTo(250, 469),
                    QuadBezTo([(500, 431), (750, 469)]),
                    LineTo(625, 530),
                    LineTo(625, 455),
                    QuadBezTo([(500, 445), (375, 455)]),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 600),
                    LineTo(125, 342),
                    LineTo(0, 150),
                    QuadBezTo([(125, 112), (250, 94)]),
                    LineTo(250, 19),
                    QuadBezTo([(500, -19), (750, 19)]),
                    LineTo(750, 94),
                    QuadBezTo([(875, 112), (1000, 150)]),
                    LineTo(875, 342),
                    LineTo(1000, 600),
                    QuadBezTo([(812, 544), (625, 530)]),
                    LineTo(750, 469),
                    QuadBezTo([(500, 431), (250, 469)]),
                    LineTo(375, 530),
                    QuadBezTo([(188, 544), (0, 600)]),
                    Close,
                    MoveTo(250, 469),
                    LineTo(250, 94),
                    MoveTo(750, 94),
                    LineTo(750, 469),
                    MoveTo(375, 455),
                    LineTo(375, 530),
                    MoveTo(625, 530),
                    LineTo(625, 455),
                ],
            ),
        ],
    );
    check(
        "ellipseRibbon2",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 777),
                    QuadBezTo([(62, 704), (125, 686)]),
                    LineTo(83, 607),
                    QuadBezTo([(166, 558), (250, 607)]),
                    LineTo(208, 686),
                    QuadBezTo([(271, 704), (333, 777)]),
                    LineTo(291, 443),
                    LineTo(333, 194),
                    QuadBezTo([(291, 146), (250, 121)]),
                    LineTo(250, 24),
                    QuadBezTo([(166, -24), (83, 24)]),
                    LineTo(83, 121),
                    QuadBezTo([(42, 146), (0, 194)]),
                    LineTo(42, 443),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(125, 589),
                    LineTo(125, 686),
                    LineTo(83, 607),
                    QuadBezTo([(166, 558), (250, 607)]),
                    LineTo(208, 686),
                    LineTo(208, 589),
                    QuadBezTo([(166, 577), (125, 589)]),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 777),
                    LineTo(42, 443),
                    LineTo(0, 194),
                    QuadBezTo([(42, 146), (83, 121)]),
                    LineTo(83, 24),
                    QuadBezTo([(166, -24), (250, 24)]),
                    LineTo(250, 121),
                    QuadBezTo([(291, 146), (333, 194)]),
                    LineTo(291, 443),
                    LineTo(333, 777),
                    QuadBezTo([(271, 704), (208, 686)]),
                    LineTo(250, 607),
                    QuadBezTo([(166, 558), (83, 607)]),
                    LineTo(125, 686),
                    QuadBezTo([(62, 704), (0, 777)]),
                    Close,
                    MoveTo(83, 607),
                    LineTo(83, 121),
                    MoveTo(250, 121),
                    LineTo(250, 607),
                    MoveTo(125, 589),
                    LineTo(125, 686),
                    MoveTo(208, 686),
                    LineTo(208, 589),
                ],
            ),
        ],
    );
    check(
        "ellipseRibbon2",
        &[("adj1", "val 15000"), ("adj2", "val 30000"), ("adj3", "val 7500")],
        1000,
        600,
        &[
            (
                true,
                false,
                Norm,
                &[
                    MoveTo(0, 600),
                    QuadBezTo([(238, 557), (475, 555)]),
                    LineTo(350, 514),
                    QuadBezTo([(500, 506), (650, 514)]),
                    LineTo(525, 555),
                    QuadBezTo([(762, 557), (1000, 600)]),
                    LineTo(875, 325),
                    LineTo(1000, 90),
                    QuadBezTo([(825, 58), (650, 49)]),
                    LineTo(650, 4),
                    QuadBezTo([(500, -4), (350, 4)]),
                    LineTo(350, 49),
                    QuadBezTo([(175, 58), (0, 90)]),
                    LineTo(125, 325),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(475, 510),
                    LineTo(475, 555),
                    LineTo(350, 514),
                    QuadBezTo([(500, 506), (650, 514)]),
                    LineTo(525, 555),
                    LineTo(525, 510),
                    QuadBezTo([(500, 510), (475, 510)]),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(0, 600),
                    LineTo(125, 325),
                    LineTo(0, 90),
                    QuadBezTo([(175, 58), (350, 49)]),
                    LineTo(350, 4),
                    QuadBezTo([(500, -4), (650, 4)]),
                    LineTo(650, 49),
                    QuadBezTo([(825, 58), (1000, 90)]),
                    LineTo(875, 325),
                    LineTo(1000, 600),
                    QuadBezTo([(762, 557), (525, 555)]),
                    LineTo(650, 514),
                    QuadBezTo([(500, 506), (350, 514)]),
                    LineTo(475, 555),
                    QuadBezTo([(238, 557), (0, 600)]),
                    Close,
                    MoveTo(350, 514),
                    LineTo(350, 49),
                    MoveTo(650, 49),
                    LineTo(650, 514),
                    MoveTo(475, 510),
                    LineTo(475, 555),
                    MoveTo(525, 555),
                    LineTo(525, 510),
                ],
            ),
        ],
    );
}

#[test]
fn moon_paths() {
    check(
        "moon",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(1000, 600),
                ArcTo { w_r: 1000, h_r: 300, st_ang: 5400000, sw_ang: 10800000 },
                ArcTo { w_r: 1250, h_r: 375, st_ang: -9491915, sw_ang: -2616169 },
                Close,
            ],
        )],
    );
    check(
        "moon",
        &[],
        333,
        777,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(333, 777),
                ArcTo { w_r: 333, h_r: 388, st_ang: 5400000, sw_ang: 10800000 },
                ArcTo { w_r: 416, h_r: 486, st_ang: -7364114, sw_ang: -6871773 },
                Close,
            ],
        )],
    );
    check(
        "moon",
        &[("adj", "val 30000")],
        1000,
        600,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(1000, 600),
                ArcTo { w_r: 1000, h_r: 300, st_ang: 5400000, sw_ang: 10800000 },
                ArcTo { w_r: 1064, h_r: 319, st_ang: -8431652, sw_ang: -4736695 },
                Close,
            ],
        )],
    );
}

#[test]
fn cloud_callout_paths() {
    check(
        "cloudCallout",
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
            (true, true, Norm, &[MoveTo(308, 675), ArcTo { w_r: 17, h_r: 17, st_ang: 0, sw_ang: 21600000 }, Close]),
            (true, true, Norm, &[MoveTo(336, 655), ArcTo { w_r: 33, h_r: 33, st_ang: 0, sw_ang: 21600000 }, Close]),
            (true, true, Norm, &[MoveTo(381, 605), ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: 21600000 }, Close]),
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
        "cloudCallout",
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
            (true, true, Norm, &[MoveTo(106, 874), ArcTo { w_r: 9, h_r: 9, st_ang: 0, sw_ang: 21600000 }, Close]),
            (true, true, Norm, &[MoveTo(120, 841), ArcTo { w_r: 18, h_r: 18, st_ang: 0, sw_ang: 21600000 }, Close]),
            (true, true, Norm, &[MoveTo(137, 790), ArcTo { w_r: 28, h_r: 28, st_ang: 0, sw_ang: 21600000 }, Close]),
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
    check(
        "cloudCallout",
        &[("adj1", "val -12499"), ("adj2", "val 37500")],
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
            (true, true, Norm, &[MoveTo(392, 525), ArcTo { w_r: 17, h_r: 17, st_ang: 0, sw_ang: 21600000 }, Close]),
            (true, true, Norm, &[MoveTo(403, 535), ArcTo { w_r: 33, h_r: 33, st_ang: 0, sw_ang: 21600000 }, Close]),
            (true, true, Norm, &[MoveTo(398, 574), ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: 21600000 }, Close]),
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
}

#[test]
fn swoosh_arrow_paths() {
    check(
        "swooshArrow",
        &[],
        1000,
        600,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(0, 600),
                QuadBezTo([(167, 200), (900, 75)]),
                LineTo(892, 0),
                LineTo(1000, 120),
                LineTo(925, 300),
                LineTo(917, 225),
                QuadBezTo([(250, 275), (0, 600)]),
                Close,
            ],
        )],
    );
    check(
        "swooshArrow",
        &[],
        333,
        777,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(0, 777),
                QuadBezTo([(56, 259), (277, 42)]),
                LineTo(273, 0),
                LineTo(333, 100),
                LineTo(304, 278),
                LineTo(299, 236),
                QuadBezTo([(83, 301), (0, 777)]),
                Close,
            ],
        )],
    );
    check(
        "swooshArrow",
        &[("adj1", "val 15000"), ("adj2", "val 10000")],
        1000,
        600,
        &[(
            true,
            true,
            Norm,
            &[
                MoveTo(0, 600),
                QuadBezTo([(167, 200), (940, 75)]),
                LineTo(932, 0),
                LineTo(1000, 90),
                LineTo(959, 240),
                LineTo(950, 165),
                QuadBezTo([(250, 215), (0, 600)]),
                Close,
            ],
        )],
    );
}
