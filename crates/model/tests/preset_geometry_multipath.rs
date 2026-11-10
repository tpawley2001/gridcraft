//! Straight-edged presets drawn with several paths, some only filled and some only stroked:
//! sort, collate and offline-storage flowchart symbols, the chart plus/X/star markers and the
//! square/corner tabs. Paths, guides and flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`) and
//! its preset shape definitions (§20.1.10.56); a path's `fill="none"` draws no fill and
//! `stroke="false"` no outline. Expected vertices are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, Close, LineTo, MoveTo};
use gridcraft_model::{PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    let close = |(x1, y1): (i64, i64), (x2, y2): (i64, i64)| (x1 - x2).abs() <= 2 && (y1 - y2).abs() <= 2;
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => close((*x1, *y1), (*x2, *y2)),
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

const PRESETS: [&str; 8] =
    ["flowChartSort", "flowChartCollate", "flowChartOfflineStorage", "chartPlus", "chartX", "chartStar", "squareTabs", "cornerTabs"];

#[test]
fn multi_path_presets_stay_in_hostile_boxes() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            for c in paths.iter().flat_map(|p| &p.commands) {
                if let MoveTo(x, y) | LineTo(x, y) = *c {
                    assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} in {w}x{h}: ({x}, {y}) outside the box");
                }
            }
        }
    }
}

#[test]
fn multi_path_presets_with_hostile_adjustments_stay_in_the_box() {
    for preset in PRESETS {
        for name in ["adj", "adj1", "adj2"] {
            for v in ["-9223372036854775808", "-5", "9223372036854775807", "21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for c in paths.iter().flat_map(|p| &p.commands) {
                    if let MoveTo(x, y) | LineTo(x, y) = *c {
                        assert!((0..=1000).contains(&x) && (0..=700).contains(&y), "{preset} {name}={v}: ({x}, {y}) outside the box");
                    }
                }
            }
        }
    }
}

#[test]
fn multi_path_presets_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}

#[test]
fn flow_chart_sort_paths() {
    check(
        "flowChartSort",
        &[],
        1000,
        600,
        &[
            (true, false, &[MoveTo(0, 300), LineTo(500, 0), LineTo(1000, 300), LineTo(500, 600), Close]),
            (false, true, &[MoveTo(0, 300), LineTo(1000, 300)]),
            (false, true, &[MoveTo(0, 300), LineTo(500, 0), LineTo(1000, 300), LineTo(500, 600), Close]),
        ],
    );
    check(
        "flowChartSort",
        &[],
        333,
        777,
        &[
            (true, false, &[MoveTo(0, 388), LineTo(166, 0), LineTo(333, 388), LineTo(166, 777), Close]),
            (false, true, &[MoveTo(0, 388), LineTo(333, 388)]),
            (false, true, &[MoveTo(0, 388), LineTo(166, 0), LineTo(333, 388), LineTo(166, 777), Close]),
        ],
    );
}

#[test]
fn flow_chart_collate_paths() {
    check(
        "flowChartCollate",
        &[],
        1000,
        600,
        &[(true, true, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(500, 300), LineTo(1000, 600), LineTo(0, 600), LineTo(500, 300), Close])],
    );
    check(
        "flowChartCollate",
        &[],
        333,
        777,
        &[(true, true, &[MoveTo(0, 0), LineTo(333, 0), LineTo(166, 388), LineTo(333, 777), LineTo(0, 777), LineTo(166, 388), Close])],
    );
}

#[test]
fn flow_chart_offline_storage_paths() {
    check(
        "flowChartOfflineStorage",
        &[],
        1000,
        600,
        &[
            (true, false, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(500, 600), Close]),
            (false, true, &[MoveTo(400, 480), LineTo(600, 480)]),
            (false, true, &[MoveTo(0, 0), LineTo(1000, 0), LineTo(500, 600), Close]),
        ],
    );
    check(
        "flowChartOfflineStorage",
        &[],
        333,
        777,
        &[
            (true, false, &[MoveTo(0, 0), LineTo(333, 0), LineTo(166, 777), Close]),
            (false, true, &[MoveTo(133, 622), LineTo(200, 622)]),
            (false, true, &[MoveTo(0, 0), LineTo(333, 0), LineTo(166, 777), Close]),
        ],
    );
}

#[test]
fn chart_plus_paths() {
    check(
        "chartPlus",
        &[],
        1000,
        600,
        &[
            (false, true, &[MoveTo(500, 0), LineTo(500, 600), MoveTo(0, 300), LineTo(1000, 300)]),
            (true, false, &[MoveTo(0, 0), LineTo(0, 600), LineTo(1000, 600), LineTo(1000, 0), Close]),
        ],
    );
    check(
        "chartPlus",
        &[],
        333,
        777,
        &[
            (false, true, &[MoveTo(166, 0), LineTo(166, 777), MoveTo(0, 388), LineTo(333, 388)]),
            (true, false, &[MoveTo(0, 0), LineTo(0, 777), LineTo(333, 777), LineTo(333, 0), Close]),
        ],
    );
}

#[test]
fn chart_x_paths() {
    check(
        "chartX",
        &[],
        1000,
        600,
        &[
            (false, true, &[MoveTo(0, 0), LineTo(1000, 600), MoveTo(0, 600), LineTo(1000, 0)]),
            (true, false, &[MoveTo(0, 0), LineTo(0, 600), LineTo(1000, 600), LineTo(1000, 0), Close]),
        ],
    );
    check(
        "chartX",
        &[],
        333,
        777,
        &[
            (false, true, &[MoveTo(0, 0), LineTo(333, 777), MoveTo(0, 777), LineTo(333, 0)]),
            (true, false, &[MoveTo(0, 0), LineTo(0, 777), LineTo(333, 777), LineTo(333, 0), Close]),
        ],
    );
}

#[test]
fn chart_star_paths() {
    check(
        "chartStar",
        &[],
        1000,
        600,
        &[
            (false, true, &[MoveTo(0, 0), LineTo(1000, 600), MoveTo(0, 600), LineTo(1000, 0), MoveTo(500, 0), LineTo(500, 600)]),
            (true, false, &[MoveTo(0, 0), LineTo(0, 600), LineTo(1000, 600), LineTo(1000, 0), Close]),
        ],
    );
    check(
        "chartStar",
        &[],
        333,
        777,
        &[
            (false, true, &[MoveTo(0, 0), LineTo(333, 777), MoveTo(0, 777), LineTo(333, 0), MoveTo(166, 0), LineTo(166, 777)]),
            (true, false, &[MoveTo(0, 0), LineTo(0, 777), LineTo(333, 777), LineTo(333, 0), Close]),
        ],
    );
}

#[test]
fn square_tabs_paths() {
    check(
        "squareTabs",
        &[],
        1000,
        600,
        &[
            (true, true, &[MoveTo(0, 0), LineTo(58, 0), LineTo(58, 58), LineTo(0, 58), Close]),
            (true, true, &[MoveTo(0, 542), LineTo(58, 542), LineTo(58, 600), LineTo(0, 600), Close]),
            (true, true, &[MoveTo(942, 0), LineTo(1000, 0), LineTo(1000, 58), LineTo(942, 58), Close]),
            (true, true, &[MoveTo(942, 542), LineTo(1000, 542), LineTo(1000, 600), LineTo(942, 600), Close]),
        ],
    );
    check(
        "squareTabs",
        &[],
        333,
        777,
        &[
            (true, true, &[MoveTo(0, 0), LineTo(42, 0), LineTo(42, 42), LineTo(0, 42), Close]),
            (true, true, &[MoveTo(0, 735), LineTo(42, 735), LineTo(42, 777), LineTo(0, 777), Close]),
            (true, true, &[MoveTo(291, 0), LineTo(333, 0), LineTo(333, 42), LineTo(291, 42), Close]),
            (true, true, &[MoveTo(291, 735), LineTo(333, 735), LineTo(333, 777), LineTo(291, 777), Close]),
        ],
    );
}

#[test]
fn corner_tabs_paths() {
    check(
        "cornerTabs",
        &[],
        1000,
        600,
        &[
            (true, true, &[MoveTo(0, 0), LineTo(58, 0), LineTo(0, 58), Close]),
            (true, true, &[MoveTo(0, 542), LineTo(58, 600), LineTo(0, 600), Close]),
            (true, true, &[MoveTo(942, 0), LineTo(1000, 0), LineTo(1000, 58), Close]),
            (true, true, &[MoveTo(1000, 542), LineTo(1000, 600), LineTo(942, 600), Close]),
        ],
    );
    check(
        "cornerTabs",
        &[],
        333,
        777,
        &[
            (true, true, &[MoveTo(0, 0), LineTo(42, 0), LineTo(0, 42), Close]),
            (true, true, &[MoveTo(0, 735), LineTo(42, 777), LineTo(0, 777), Close]),
            (true, true, &[MoveTo(291, 0), LineTo(333, 0), LineTo(333, 42), Close]),
            (true, true, &[MoveTo(333, 735), LineTo(333, 777), LineTo(291, 777), Close]),
        ],
    );
}
