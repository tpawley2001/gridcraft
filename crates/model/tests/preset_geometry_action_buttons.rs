//! Smiley face and the action buttons (blank, back/previous, forward/next, beginning, end, home): several paths per preset,
//! some of them shaded. A path's `fill="lighten" | "lightenLess" | "darken" | "darkenLess"` (ECMA-376 Part 1 §20.1.9.15,
//! ST_PathFillMode) fills that face with the shape's fill shaded, `fill="none"` draws no fill and `stroke="false"` no
//! outline. Paths and guides follow §20.1.9.18 (`prstGeom`) and its preset shape definitions; `a:arcTo` (§20.1.9.3) is
//! kept as `ArcTo`, `a:quadBezTo` as `QuadBezTo`. Expected values are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo, QuadBezTo};
use gridcraft_model::PathShade::{self, Darken, DarkenLess, Norm};
use gridcraft_model::{ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    let d = |x: i64, y: i64, tol: i64| (x - y).abs() <= tol;
    let p = |a: &(i64, i64), b: &(i64, i64)| d(a.0, b.0, 2) && d(a.1, b.1, 2);
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => d(*x1, *x2, 2) && d(*y1, *y2, 2),
        (QuadBezTo(a), QuadBezTo(b)) => a.iter().zip(b).all(|(a, b)| p(a, b)),
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

const PRESETS: [&str; 7] = [
    "smileyFace",
    "actionButtonBlank",
    "actionButtonBackPrevious",
    "actionButtonForwardNext",
    "actionButtonBeginning",
    "actionButtonEnd",
    "actionButtonHome",
];

fn assert_in_box(preset: &str, what: &str, paths: &[gridcraft_model::GeometryPath], w: i64, h: i64) {
    let inside = |x: i64, y: i64| (0..=w).contains(&x) && (0..=h).contains(&y);
    for c in paths.iter().flat_map(|p| &p.commands) {
        match c {
            MoveTo(x, y) | LineTo(x, y) => assert!(inside(*x, *y), "{preset} {what}: ({x}, {y}) outside {w}x{h}"),
            QuadBezTo(pts) => assert!(pts.iter().all(|&(x, y)| inside(x, y)), "{preset} {what}: {pts:?} outside {w}x{h}"),
            ArcTo { w_r, h_r, .. } => assert!((0..=w).contains(w_r) && (0..=h).contains(h_r), "{preset} {what}: radii {w_r}, {h_r}"),
            _ => {}
        }
    }
}

#[test]
fn smiley_and_action_button_points_stay_in_hostile_boxes() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            assert_in_box(preset, "default", &paths, w, h);
        }
    }
}

#[test]
fn smiley_and_action_buttons_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn smiley_adjustment_is_pinned() {
    for v in ["-9223372036854775808", "-5000", "9223372036854775807", "21600000"] {
        let paths = preset_paths(&geom("smileyFace", &[("adj", &format!("val {v}"))]), 1000, 700).expect("supported preset");
        assert_in_box("smileyFace", &format!("adj={v}"), &paths, 1000, 700);
    }
}

#[test]
fn smiley_face_paths() {
    check(
        "smileyFace",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 300), ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 21600000 }, Close]),
            (
                true,
                true,
                DarkenLess,
                &[
                    MoveTo(288, 210),
                    ArcTo { w_r: 52, h_r: 31, st_ang: 10800000, sw_ang: 21600000 },
                    MoveTo(608, 210),
                    ArcTo { w_r: 52, h_r: 31, st_ang: 10800000, sw_ang: 21600000 },
                ],
            ),
            (false, true, Norm, &[MoveTo(229, 431), QuadBezTo([(500, 543), (770, 431)])]),
            (false, true, Norm, &[MoveTo(0, 300), ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 21600000 }, Close]),
        ],
    );
    check(
        "smileyFace",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 388), ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 21600000 }, Close]),
            (
                true,
                true,
                DarkenLess,
                &[
                    MoveTo(96, 272),
                    ArcTo { w_r: 17, h_r: 40, st_ang: 10800000, sw_ang: 21600000 },
                    MoveTo(202, 272),
                    ArcTo { w_r: 17, h_r: 40, st_ang: 10800000, sw_ang: 21600000 },
                ],
            ),
            (false, true, Norm, &[MoveTo(76, 558), QuadBezTo([(166, 703), (257, 558)])]),
            (false, true, Norm, &[MoveTo(0, 388), ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 21600000 }, Close]),
        ],
    );
    check(
        "smileyFace",
        &[("adj", "val 2791")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 300), ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 21600000 }, Close]),
            (
                true,
                true,
                DarkenLess,
                &[
                    MoveTo(288, 210),
                    ArcTo { w_r: 52, h_r: 31, st_ang: 10800000, sw_ang: 21600000 },
                    MoveTo(608, 210),
                    ArcTo { w_r: 52, h_r: 31, st_ang: 10800000, sw_ang: 21600000 },
                ],
            ),
            (false, true, Norm, &[MoveTo(229, 442), QuadBezTo([(500, 509), (770, 442)])]),
            (false, true, Norm, &[MoveTo(0, 300), ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 21600000 }, Close]),
        ],
    );
}

#[test]
fn action_button_blank_paths() {
    check("actionButtonBlank", &[], 1000, 600, &[(true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close])]);
    check("actionButtonBlank", &[], 333, 777, &[(true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close])]);
}

#[test]
fn action_button_back_previous_paths() {
    check(
        "actionButtonBackPrevious",
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
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(275, 300),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    Close,
                ],
            ),
            (true, false, Darken, &[MoveTo(275, 300), LineTo(725, 75), LineTo(725, 525), Close]),
            (false, true, Norm, &[MoveTo(275, 300), LineTo(725, 75), LineTo(725, 525), Close]),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
        ],
    );
    check(
        "actionButtonBackPrevious",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close, MoveTo(42, 388), LineTo(291, 264), LineTo(291, 513), Close],
            ),
            (true, false, Darken, &[MoveTo(42, 388), LineTo(291, 264), LineTo(291, 513), Close]),
            (false, true, Norm, &[MoveTo(42, 388), LineTo(291, 264), LineTo(291, 513), Close]),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
        ],
    );
}

#[test]
fn action_button_forward_next_paths() {
    check(
        "actionButtonForwardNext",
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
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(725, 300),
                    LineTo(275, 75),
                    LineTo(275, 525),
                    Close,
                ],
            ),
            (true, false, Darken, &[MoveTo(725, 300), LineTo(275, 75), LineTo(275, 525), Close]),
            (false, true, Norm, &[MoveTo(725, 300), LineTo(275, 525), LineTo(275, 75), Close]),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
        ],
    );
    check(
        "actionButtonForwardNext",
        &[],
        333,
        777,
        &[
            (
                true,
                false,
                Norm,
                &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close, MoveTo(291, 388), LineTo(42, 264), LineTo(42, 513), Close],
            ),
            (true, false, Darken, &[MoveTo(291, 388), LineTo(42, 264), LineTo(42, 513), Close]),
            (false, true, Norm, &[MoveTo(291, 388), LineTo(42, 513), LineTo(42, 264), Close]),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
        ],
    );
}

#[test]
fn action_button_beginning_paths() {
    check(
        "actionButtonBeginning",
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
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(388, 300),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    Close,
                    MoveTo(331, 75),
                    LineTo(275, 75),
                    LineTo(275, 525),
                    LineTo(331, 525),
                    Close,
                ],
            ),
            (
                true,
                false,
                Darken,
                &[
                    MoveTo(388, 300),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    Close,
                    MoveTo(331, 75),
                    LineTo(275, 75),
                    LineTo(275, 525),
                    LineTo(331, 525),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(388, 300),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    Close,
                    MoveTo(331, 75),
                    LineTo(331, 525),
                    LineTo(275, 525),
                    LineTo(275, 75),
                    Close,
                ],
            ),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
        ],
    );
    check(
        "actionButtonBeginning",
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
                    LineTo(333, 0),
                    LineTo(333, 777),
                    LineTo(0, 777),
                    Close,
                    MoveTo(104, 388),
                    LineTo(291, 264),
                    LineTo(291, 513),
                    Close,
                    MoveTo(73, 264),
                    LineTo(42, 264),
                    LineTo(42, 513),
                    LineTo(73, 513),
                    Close,
                ],
            ),
            (
                true,
                false,
                Darken,
                &[
                    MoveTo(104, 388),
                    LineTo(291, 264),
                    LineTo(291, 513),
                    Close,
                    MoveTo(73, 264),
                    LineTo(42, 264),
                    LineTo(42, 513),
                    LineTo(73, 513),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(104, 388),
                    LineTo(291, 264),
                    LineTo(291, 513),
                    Close,
                    MoveTo(73, 264),
                    LineTo(73, 513),
                    LineTo(42, 513),
                    LineTo(42, 264),
                    Close,
                ],
            ),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
        ],
    );
}

#[test]
fn action_button_end_paths() {
    check(
        "actionButtonEnd",
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
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(612, 300),
                    LineTo(275, 75),
                    LineTo(275, 525),
                    Close,
                    MoveTo(669, 75),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    LineTo(669, 525),
                    Close,
                ],
            ),
            (
                true,
                false,
                Darken,
                &[
                    MoveTo(612, 300),
                    LineTo(275, 75),
                    LineTo(275, 525),
                    Close,
                    MoveTo(669, 75),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    LineTo(669, 525),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(612, 300),
                    LineTo(275, 525),
                    LineTo(275, 75),
                    Close,
                    MoveTo(669, 75),
                    LineTo(725, 75),
                    LineTo(725, 525),
                    LineTo(669, 525),
                    Close,
                ],
            ),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
        ],
    );
    check(
        "actionButtonEnd",
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
                    LineTo(333, 0),
                    LineTo(333, 777),
                    LineTo(0, 777),
                    Close,
                    MoveTo(229, 388),
                    LineTo(42, 264),
                    LineTo(42, 513),
                    Close,
                    MoveTo(260, 264),
                    LineTo(291, 264),
                    LineTo(291, 513),
                    LineTo(260, 513),
                    Close,
                ],
            ),
            (
                true,
                false,
                Darken,
                &[
                    MoveTo(229, 388),
                    LineTo(42, 264),
                    LineTo(42, 513),
                    Close,
                    MoveTo(260, 264),
                    LineTo(291, 264),
                    LineTo(291, 513),
                    LineTo(260, 513),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(229, 388),
                    LineTo(42, 513),
                    LineTo(42, 264),
                    Close,
                    MoveTo(260, 264),
                    LineTo(291, 264),
                    LineTo(291, 513),
                    LineTo(260, 513),
                    Close,
                ],
            ),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
        ],
    );
}

#[test]
fn action_button_home_paths() {
    check(
        "actionButtonHome",
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
                    LineTo(1000, 0),
                    LineTo(1000, 600),
                    LineTo(0, 600),
                    Close,
                    MoveTo(500, 75),
                    LineTo(275, 300),
                    LineTo(331, 300),
                    LineTo(331, 525),
                    LineTo(669, 525),
                    LineTo(669, 300),
                    LineTo(725, 300),
                    LineTo(641, 216),
                    LineTo(641, 103),
                    LineTo(584, 103),
                    LineTo(584, 159),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(641, 216),
                    LineTo(641, 103),
                    LineTo(584, 103),
                    LineTo(584, 159),
                    Close,
                    MoveTo(331, 300),
                    LineTo(331, 525),
                    LineTo(472, 525),
                    LineTo(472, 412),
                    LineTo(528, 412),
                    LineTo(528, 525),
                    LineTo(669, 525),
                    LineTo(669, 300),
                    Close,
                ],
            ),
            (
                true,
                false,
                Darken,
                &[
                    MoveTo(500, 75),
                    LineTo(275, 300),
                    LineTo(725, 300),
                    Close,
                    MoveTo(472, 412),
                    LineTo(528, 412),
                    LineTo(528, 525),
                    LineTo(472, 525),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(500, 75),
                    LineTo(584, 159),
                    LineTo(584, 103),
                    LineTo(641, 103),
                    LineTo(641, 216),
                    LineTo(725, 300),
                    LineTo(669, 300),
                    LineTo(669, 525),
                    LineTo(331, 525),
                    LineTo(331, 300),
                    LineTo(275, 300),
                    Close,
                    MoveTo(584, 159),
                    LineTo(641, 216),
                    MoveTo(669, 300),
                    LineTo(331, 300),
                    MoveTo(472, 525),
                    LineTo(472, 412),
                    LineTo(528, 412),
                    LineTo(528, 525),
                ],
            ),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
        ],
    );
    check(
        "actionButtonHome",
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
                    LineTo(333, 0),
                    LineTo(333, 777),
                    LineTo(0, 777),
                    Close,
                    MoveTo(166, 264),
                    LineTo(42, 388),
                    LineTo(73, 388),
                    LineTo(73, 513),
                    LineTo(260, 513),
                    LineTo(260, 388),
                    LineTo(291, 388),
                    LineTo(245, 342),
                    LineTo(245, 279),
                    LineTo(213, 279),
                    LineTo(213, 310),
                    Close,
                ],
            ),
            (
                true,
                false,
                DarkenLess,
                &[
                    MoveTo(245, 342),
                    LineTo(245, 279),
                    LineTo(213, 279),
                    LineTo(213, 310),
                    Close,
                    MoveTo(73, 388),
                    LineTo(73, 513),
                    LineTo(151, 513),
                    LineTo(151, 451),
                    LineTo(182, 451),
                    LineTo(182, 513),
                    LineTo(260, 513),
                    LineTo(260, 388),
                    Close,
                ],
            ),
            (
                true,
                false,
                Darken,
                &[
                    MoveTo(166, 264),
                    LineTo(42, 388),
                    LineTo(291, 388),
                    Close,
                    MoveTo(151, 451),
                    LineTo(182, 451),
                    LineTo(182, 513),
                    LineTo(151, 513),
                    Close,
                ],
            ),
            (
                false,
                true,
                Norm,
                &[
                    MoveTo(166, 264),
                    LineTo(213, 310),
                    LineTo(213, 279),
                    LineTo(245, 279),
                    LineTo(245, 342),
                    LineTo(291, 388),
                    LineTo(260, 388),
                    LineTo(260, 513),
                    LineTo(73, 513),
                    LineTo(73, 388),
                    LineTo(42, 388),
                    Close,
                    MoveTo(213, 310),
                    LineTo(245, 342),
                    MoveTo(260, 388),
                    LineTo(73, 388),
                    MoveTo(151, 513),
                    LineTo(151, 451),
                    LineTo(182, 451),
                    LineTo(182, 513),
                ],
            ),
            (false, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
        ],
    );
}
