//! Line callouts: `callout1-3` (no border), `borderCallout1-3`, `accentCallout1-3` (accent bar, no border) and
//! `accentBorderCallout1-3`. Each is the box (filled; stroked only for the border variants), for the accent
//! variants an open vertical accent bar, and the leader: an open, stroke-only polyline whose points the adjust
//! values place as fractions of the box (`*/ w adjN 100000`). By default the leader lies OUTSIDE the box, to
//! its left and below it, as in Excel, so leader points are not clamped into the box; they are bounded to ten
//! box sizes from the centre. Paths and guides follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`) and its preset
//! shape definitions; expected values are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, Close, LineTo, MoveTo};
use gridcraft_model::PathShade::{self, Norm};
use gridcraft_model::{ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    let d = |x: i64, y: i64| (x - y).abs() <= 2;
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => d(*x1, *x2) && d(*y1, *y2),
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

const PRESETS: [&str; 12] = [
    "callout1",
    "callout2",
    "callout3",
    "borderCallout1",
    "borderCallout2",
    "borderCallout3",
    "accentCallout1",
    "accentCallout2",
    "accentCallout3",
    "accentBorderCallout1",
    "accentBorderCallout2",
    "accentBorderCallout3",
];

#[test]
fn the_default_leader_lies_left_of_the_box() {
    for preset in PRESETS {
        let paths = preset_paths(&geom(preset, &[]), 1000, 600).unwrap_or_else(|| panic!("{preset} has an outline"));
        let leader = paths.last().expect("a leader path");
        assert!(!leader.fill && leader.stroke, "{preset}: the leader is stroked, not filled");
        let points: Vec<(i64, i64)> =
            leader.commands.iter().filter_map(|c| if let MoveTo(x, y) | LineTo(x, y) = *c { Some((x, y)) } else { None }).collect();
        assert!(points.len() >= 2, "{preset}: leader points {points:?}");
        assert!(points.iter().all(|&(x, _)| x < 0), "{preset}: leader kept outside the 1000x600 box, not clamped: {points:?}");
    }
}

#[test]
fn hostile_boxes_and_adjustments_stay_within_ten_box_sizes() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            assert!(preset_paths(&geom(preset, &[]), w, h).is_some(), "{preset} in {w}x{h}");
        }
        for name in ["adj1", "adj2", "adj3", "adj4", "adj5", "adj6", "adj7", "adj8"] {
            for v in ["-9223372036854775808", "-5", "9223372036854775807", "21600000", "-21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for c in paths.iter().flat_map(|p| &p.commands) {
                    if let MoveTo(x, y) | LineTo(x, y) = *c {
                        assert!((-9_500..=10_500).contains(&x) && (-6_650..=7_350).contains(&y), "{preset} {name}={v}: point ({x}, {y})");
                    }
                }
            }
        }
    }
}

#[test]
fn line_callouts_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn callout1_paths() {
    check(
        "callout1",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-383, 675)]),
        ],
    );
    check(
        "callout1",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-128, 874)]),
        ],
    );
    check(
        "callout1",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 67500"), ("adj4", "val -22999")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-230, 405)]),
        ],
    );
}

#[test]
fn callout2_paths() {
    check(
        "callout2",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-467, 675)]),
        ],
    );
    check(
        "callout2",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-155, 874)]),
        ],
    );
    check(
        "callout2",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 11250"), ("adj4", "val -10000"), ("adj5", "val 67500"), ("adj6", "val -28000")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-280, 405)]),
        ],
    );
}

#[test]
fn callout3_paths() {
    check(
        "callout3",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-167, 600), LineTo(-83, 678)]),
        ],
    );
    check(
        "callout3",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-56, 777), LineTo(-28, 878)]),
        ],
    );
    check(
        "callout3",
        &[
            ("adj1", "val 11250"),
            ("adj2", "val -4999"),
            ("adj3", "val 11250"),
            ("adj4", "val -10000"),
            ("adj5", "val 60000"),
            ("adj6", "val -10000"),
            ("adj7", "val 67777"),
            ("adj8", "val -4999"),
        ],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-100, 360), LineTo(-50, 407)]),
        ],
    );
}

#[test]
fn border_callout1_paths() {
    check(
        "borderCallout1",
        &[],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-383, 675)]),
        ],
    );
    check(
        "borderCallout1",
        &[],
        333,
        777,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-128, 874)]),
        ],
    );
    check(
        "borderCallout1",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 67500"), ("adj4", "val -22999")],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-230, 405)]),
        ],
    );
}

#[test]
fn border_callout2_paths() {
    check(
        "borderCallout2",
        &[],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-467, 675)]),
        ],
    );
    check(
        "borderCallout2",
        &[],
        333,
        777,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-155, 874)]),
        ],
    );
    check(
        "borderCallout2",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 11250"), ("adj4", "val -10000"), ("adj5", "val 67500"), ("adj6", "val -28000")],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-280, 405)]),
        ],
    );
}

#[test]
fn border_callout3_paths() {
    check(
        "borderCallout3",
        &[],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-167, 600), LineTo(-83, 678)]),
        ],
    );
    check(
        "borderCallout3",
        &[],
        333,
        777,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-56, 777), LineTo(-28, 878)]),
        ],
    );
    check(
        "borderCallout3",
        &[
            ("adj1", "val 11250"),
            ("adj2", "val -4999"),
            ("adj3", "val 11250"),
            ("adj4", "val -10000"),
            ("adj5", "val 60000"),
            ("adj6", "val -10000"),
            ("adj7", "val 67777"),
            ("adj8", "val -4999"),
        ],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-100, 360), LineTo(-50, 407)]),
        ],
    );
}

#[test]
fn accent_callout1_paths() {
    check(
        "accentCallout1",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 0), Close, LineTo(-83, 600)]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-383, 675)]),
        ],
    );
    check(
        "accentCallout1",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 0), Close, LineTo(-28, 777)]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-128, 874)]),
        ],
    );
    check(
        "accentCallout1",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 67500"), ("adj4", "val -22999")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 0), Close, LineTo(-50, 600)]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-230, 405)]),
        ],
    );
}

#[test]
fn accent_callout2_paths() {
    check(
        "accentCallout2",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 0), Close, LineTo(-83, 600)]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-467, 675)]),
        ],
    );
    check(
        "accentCallout2",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 0), Close, LineTo(-28, 777)]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-155, 874)]),
        ],
    );
    check(
        "accentCallout2",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 11250"), ("adj4", "val -10000"), ("adj5", "val 67500"), ("adj6", "val -28000")],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 0), Close, LineTo(-50, 600)]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-280, 405)]),
        ],
    );
}

#[test]
fn accent_callout3_paths() {
    check(
        "accentCallout3",
        &[],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 0), Close, LineTo(-83, 600)]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-167, 600), LineTo(-83, 678)]),
        ],
    );
    check(
        "accentCallout3",
        &[],
        333,
        777,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 0), Close, LineTo(-28, 777)]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-56, 777), LineTo(-28, 878)]),
        ],
    );
    check(
        "accentCallout3",
        &[
            ("adj1", "val 11250"),
            ("adj2", "val -4999"),
            ("adj3", "val 11250"),
            ("adj4", "val -10000"),
            ("adj5", "val 60000"),
            ("adj6", "val -10000"),
            ("adj7", "val 67777"),
            ("adj8", "val -4999"),
        ],
        1000,
        600,
        &[
            (true, false, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 0), Close, LineTo(-50, 600)]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-100, 360), LineTo(-50, 407)]),
        ],
    );
}

#[test]
fn accent_border_callout1_paths() {
    check(
        "accentBorderCallout1",
        &[],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 0), Close, LineTo(-83, 600)]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-383, 675)]),
        ],
    );
    check(
        "accentBorderCallout1",
        &[],
        333,
        777,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 0), Close, LineTo(-28, 777)]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-128, 874)]),
        ],
    );
    check(
        "accentBorderCallout1",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 67500"), ("adj4", "val -22999")],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 0), Close, LineTo(-50, 600)]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-230, 405)]),
        ],
    );
}

#[test]
fn accent_border_callout2_paths() {
    check(
        "accentBorderCallout2",
        &[],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 0), Close, LineTo(-83, 600)]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-467, 675)]),
        ],
    );
    check(
        "accentBorderCallout2",
        &[],
        333,
        777,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 0), Close, LineTo(-28, 777)]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-155, 874)]),
        ],
    );
    check(
        "accentBorderCallout2",
        &[("adj1", "val 11250"), ("adj2", "val -4999"), ("adj3", "val 11250"), ("adj4", "val -10000"), ("adj5", "val 67500"), ("adj6", "val -28000")],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 0), Close, LineTo(-50, 600)]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-280, 405)]),
        ],
    );
}

#[test]
fn accent_border_callout3_paths() {
    check(
        "accentBorderCallout3",
        &[],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-83, 0), Close, LineTo(-83, 600)]),
            (false, true, Norm, &[MoveTo(-83, 112), LineTo(-167, 112), LineTo(-167, 600), LineTo(-83, 678)]),
        ],
    );
    check(
        "accentBorderCallout3",
        &[],
        333,
        777,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(333, 0), LineTo(333, 777), LineTo(0, 777), Close]),
            (false, true, Norm, &[MoveTo(-28, 0), Close, LineTo(-28, 777)]),
            (false, true, Norm, &[MoveTo(-28, 146), LineTo(-56, 146), LineTo(-56, 777), LineTo(-28, 878)]),
        ],
    );
    check(
        "accentBorderCallout3",
        &[
            ("adj1", "val 11250"),
            ("adj2", "val -4999"),
            ("adj3", "val 11250"),
            ("adj4", "val -10000"),
            ("adj5", "val 60000"),
            ("adj6", "val -10000"),
            ("adj7", "val 67777"),
            ("adj8", "val -4999"),
        ],
        1000,
        600,
        &[
            (true, true, Norm, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(1000, 600), LineTo(0, 600), Close]),
            (false, true, Norm, &[MoveTo(-50, 0), Close, LineTo(-50, 600)]),
            (false, true, Norm, &[MoveTo(-50, 68), LineTo(-100, 68), LineTo(-100, 360), LineTo(-50, 407)]),
        ],
    );
}
