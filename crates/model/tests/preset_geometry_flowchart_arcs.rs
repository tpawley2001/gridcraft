//! Curved flowchart presets drawn from arcs: magnetic disk, magnetic drum, magnetic tape (sequential
//! access storage), display, connector, or, summing junction, stored data (online storage) and
//! punched tape. Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`)
//! and its preset shape definitions; `a:arcTo` (§20.1.9.3) is kept as `ArcTo` with its radii scaled
//! from the path's own box (`<path w h>`) to the shape box. Expected values are the definitions
//! evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo};
use gridcraft_model::{PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] }
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
fn check(preset: &str, w: i64, h: i64, expected: &[(bool, bool, &[PathCommand])]) {
    let paths = preset_paths(&geom(preset), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), expected.len(), "{preset} in {w}x{h}: path count, actual {paths:?}");
    for (i, (p, (fill, stroke, cmds))) in paths.iter().zip(expected).enumerate() {
        assert_eq!((p.w, p.h), (w, h), "{preset} path {i}: path box is the shape box");
        assert_eq!((p.fill, p.stroke), (*fill, *stroke), "{preset} path {i}: fill/stroke flags");
        assert_eq!(p.shade, PathShade::Norm, "{preset} path {i}: plain fill");
        let ok = p.commands.len() == cmds.len() && p.commands.iter().zip(cmds.iter()).all(|(a, b)| near(a, b));
        assert!(ok, "{preset} in {w}x{h} path {i}:\n actual   {:?}\n expected {cmds:?}", p.commands);
    }
}

const PRESETS: [&str; 9] = [
    "flowChartMagneticDisk",
    "flowChartMagneticDrum",
    "flowChartMagneticTape",
    "flowChartDisplay",
    "flowChartConnector",
    "flowChartOr",
    "flowChartSummingJunction",
    "flowChartOnlineStorage",
    "flowChartPunchedTape",
];

#[test]
fn curved_flowchart_points_stay_in_hostile_boxes() {
    for preset in PRESETS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
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
fn curved_flowcharts_with_an_empty_box_have_no_outline() {
    for preset in PRESETS {
        assert_eq!(preset_paths(&geom(preset), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset), 100, -1), None, "{preset}");
    }
}

#[test]
fn flow_chart_magnetic_disk_paths() {
    check(
        "flowChartMagneticDisk",
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 100),
                    ArcTo { w_r: 500, h_r: 100, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(1000, 500),
                    ArcTo { w_r: 500, h_r: 100, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(1000, 100), ArcTo { w_r: 500, h_r: 100, st_ang: 0, sw_ang: 10800000 }]),
            (
                false,
                true,
                &[
                    MoveTo(0, 100),
                    ArcTo { w_r: 500, h_r: 100, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(1000, 500),
                    ArcTo { w_r: 500, h_r: 100, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
        ],
    );
    check(
        "flowChartMagneticDisk",
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 130),
                    ArcTo { w_r: 166, h_r: 130, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(333, 648),
                    ArcTo { w_r: 166, h_r: 130, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(333, 130), ArcTo { w_r: 166, h_r: 130, st_ang: 0, sw_ang: 10800000 }]),
            (
                false,
                true,
                &[
                    MoveTo(0, 130),
                    ArcTo { w_r: 166, h_r: 130, st_ang: 10800000, sw_ang: 10800000 },
                    LineTo(333, 648),
                    ArcTo { w_r: 166, h_r: 130, st_ang: 0, sw_ang: 10800000 },
                    Close,
                ],
            ),
        ],
    );
}

#[test]
fn flow_chart_magnetic_drum_paths() {
    check(
        "flowChartMagneticDrum",
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(167, 0),
                    LineTo(833, 0),
                    ArcTo { w_r: 167, h_r: 300, st_ang: 16200000, sw_ang: 10800000 },
                    LineTo(167, 600),
                    ArcTo { w_r: 167, h_r: 300, st_ang: 5400000, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(833, 600), ArcTo { w_r: 167, h_r: 300, st_ang: 5400000, sw_ang: 10800000 }]),
            (
                false,
                true,
                &[
                    MoveTo(167, 0),
                    LineTo(833, 0),
                    ArcTo { w_r: 167, h_r: 300, st_ang: 16200000, sw_ang: 10800000 },
                    LineTo(167, 600),
                    ArcTo { w_r: 167, h_r: 300, st_ang: 5400000, sw_ang: 10800000 },
                    Close,
                ],
            ),
        ],
    );
    check(
        "flowChartMagneticDrum",
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(56, 0),
                    LineTo(278, 0),
                    ArcTo { w_r: 56, h_r: 388, st_ang: 16200000, sw_ang: 10800000 },
                    LineTo(56, 777),
                    ArcTo { w_r: 56, h_r: 388, st_ang: 5400000, sw_ang: 10800000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(278, 777), ArcTo { w_r: 56, h_r: 388, st_ang: 5400000, sw_ang: 10800000 }]),
            (
                false,
                true,
                &[
                    MoveTo(56, 0),
                    LineTo(278, 0),
                    ArcTo { w_r: 56, h_r: 388, st_ang: 16200000, sw_ang: 10800000 },
                    LineTo(56, 777),
                    ArcTo { w_r: 56, h_r: 388, st_ang: 5400000, sw_ang: 10800000 },
                    Close,
                ],
            ),
        ],
    );
}

#[test]
fn flow_chart_magnetic_tape_paths() {
    check(
        "flowChartMagneticTape",
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(500, 600),
                ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 1857825 },
                LineTo(1000, 512),
                LineTo(1000, 600),
                Close,
            ],
        )],
    );
    check(
        "flowChartMagneticTape",
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(166, 777),
                ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 4008085 },
                LineTo(333, 663),
                LineTo(333, 777),
                Close,
            ],
        )],
    );
}

#[test]
fn flow_chart_display_paths() {
    check(
        "flowChartDisplay",
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                LineTo(167, 0),
                LineTo(833, 0),
                ArcTo { w_r: 167, h_r: 300, st_ang: 16200000, sw_ang: 10800000 },
                LineTo(167, 600),
                Close,
            ],
        )],
    );
    check(
        "flowChartDisplay",
        333,
        777,
        &[(
            true,
            true,
            &[MoveTo(0, 388), LineTo(56, 0), LineTo(278, 0), ArcTo { w_r: 56, h_r: 388, st_ang: 16200000, sw_ang: 10800000 }, LineTo(56, 777), Close],
        )],
    );
}

#[test]
fn flow_chart_connector_paths() {
    check(
        "flowChartConnector",
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 300),
                ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
    check(
        "flowChartConnector",
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 388),
                ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                Close,
            ],
        )],
    );
}

#[test]
fn flow_chart_or_paths() {
    check(
        "flowChartOr",
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 300),
                    ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(500, 0), LineTo(500, 600), MoveTo(0, 300), LineTo(1000, 300)]),
            (
                false,
                true,
                &[
                    MoveTo(0, 300),
                    ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
        ],
    );
    check(
        "flowChartOr",
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 388),
                    ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(166, 0), LineTo(166, 777), MoveTo(0, 388), LineTo(333, 388)]),
            (
                false,
                true,
                &[
                    MoveTo(0, 388),
                    ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
        ],
    );
}

#[test]
fn flow_chart_summing_junction_paths() {
    check(
        "flowChartSummingJunction",
        1000,
        600,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 300),
                    ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(146, 88), LineTo(854, 512), MoveTo(854, 88), LineTo(146, 512)]),
            (
                false,
                true,
                &[
                    MoveTo(0, 300),
                    ArcTo { w_r: 500, h_r: 300, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 500, h_r: 300, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
        ],
    );
    check(
        "flowChartSummingJunction",
        333,
        777,
        &[
            (
                true,
                false,
                &[
                    MoveTo(0, 388),
                    ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
            (false, true, &[MoveTo(49, 114), LineTo(284, 663), MoveTo(284, 114), LineTo(49, 663)]),
            (
                false,
                true,
                &[
                    MoveTo(0, 388),
                    ArcTo { w_r: 166, h_r: 388, st_ang: 10800000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 16200000, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 0, sw_ang: 5400000 },
                    ArcTo { w_r: 166, h_r: 388, st_ang: 5400000, sw_ang: 5400000 },
                    Close,
                ],
            ),
        ],
    );
}

#[test]
fn flow_chart_online_storage_paths() {
    check(
        "flowChartOnlineStorage",
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(167, 0),
                LineTo(1000, 0),
                ArcTo { w_r: 167, h_r: 300, st_ang: 16200000, sw_ang: -10800000 },
                LineTo(167, 600),
                ArcTo { w_r: 167, h_r: 300, st_ang: 5400000, sw_ang: 10800000 },
                Close,
            ],
        )],
    );
    check(
        "flowChartOnlineStorage",
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(56, 0),
                LineTo(333, 0),
                ArcTo { w_r: 56, h_r: 388, st_ang: 16200000, sw_ang: -10800000 },
                LineTo(56, 777),
                ArcTo { w_r: 56, h_r: 388, st_ang: 5400000, sw_ang: 10800000 },
                Close,
            ],
        )],
    );
}

#[test]
fn flow_chart_punched_tape_paths() {
    check(
        "flowChartPunchedTape",
        1000,
        600,
        &[(
            true,
            true,
            &[
                MoveTo(0, 60),
                ArcTo { w_r: 250, h_r: 60, st_ang: 10800000, sw_ang: -10800000 },
                ArcTo { w_r: 250, h_r: 60, st_ang: 10800000, sw_ang: 10800000 },
                LineTo(1000, 540),
                ArcTo { w_r: 250, h_r: 60, st_ang: 0, sw_ang: -10800000 },
                ArcTo { w_r: 250, h_r: 60, st_ang: 0, sw_ang: 10800000 },
                Close,
            ],
        )],
    );
    check(
        "flowChartPunchedTape",
        333,
        777,
        &[(
            true,
            true,
            &[
                MoveTo(0, 78),
                ArcTo { w_r: 83, h_r: 78, st_ang: 10800000, sw_ang: -10800000 },
                ArcTo { w_r: 83, h_r: 78, st_ang: 10800000, sw_ang: 10800000 },
                LineTo(333, 699),
                ArcTo { w_r: 83, h_r: 78, st_ang: 0, sw_ang: -10800000 },
                ArcTo { w_r: 83, h_r: 78, st_ang: 0, sw_ang: 10800000 },
                Close,
            ],
        )],
    );
}
