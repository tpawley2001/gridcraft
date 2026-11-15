//! Elbow connectors `bentConnector2`, `bentConnector3` and `bentConnector4`: one open, stroke-only path (no fill, no
//! close) through the elbow points. The adjust values place the elbows as fractions of the box (`*/ w adj1 100000`,
//! `*/ h adj2 100000`) and are not clamped, so an elbow may lie left of, right of or below the box. Paths and guides
//! follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`) and its preset shape definitions; expected values are the
//! definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, LineTo, MoveTo};
use gridcraft_model::{ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

/// The preset's single path must be open and stroke-only, with exactly these vertices (first is a MoveTo).
fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, points: &[(i64, i64)]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), 1, "{preset} {adjust:?}: one path, actual {paths:?}");
    let p = &paths[0];
    assert_eq!((p.w, p.h), (w, h), "{preset}: path box is the shape box");
    assert!(!p.fill, "{preset}: a connector is not filled");
    assert!(p.stroke, "{preset}: a connector is stroked");
    let expected: Vec<PathCommand> = points.iter().enumerate().map(|(i, &(x, y))| if i == 0 { MoveTo(x, y) } else { LineTo(x, y) }).collect();
    assert_eq!(p.commands, expected, "{preset} {adjust:?} in {w}x{h}: open polyline, no Close");
}

#[test]
fn bent_connector2_is_top_edge_then_right_edge() {
    check("bentConnector2", &[], 1000, 400, &[(0, 0), (1000, 0), (1000, 400)]);
}

#[test]
fn bent_connector3_default_elbow_is_mid_width() {
    check("bentConnector3", &[], 1000, 400, &[(0, 0), (500, 0), (500, 400), (1000, 400)]);
    check("bentConnector3", &[("adj1", "val 50000")], 1000, 400, &[(0, 0), (500, 0), (500, 400), (1000, 400)]);
}

#[test]
fn bent_connector3_elbow_outside_the_box_is_not_clamped() {
    check("bentConnector3", &[("adj1", "val -25000")], 1000, 400, &[(0, 0), (-250, 0), (-250, 400), (1000, 400)]);
    check("bentConnector3", &[("adj1", "val 150000")], 1000, 400, &[(0, 0), (1500, 0), (1500, 400), (1000, 400)]);
}

#[test]
fn bent_connector4_default_and_out_of_box_elbows() {
    check("bentConnector4", &[], 1000, 400, &[(0, 0), (500, 0), (500, 200), (1000, 200), (1000, 400)]);
    check(
        "bentConnector4",
        &[("adj1", "val -10000"), ("adj2", "val 120000")],
        1000,
        400,
        &[(0, 0), (-100, 0), (-100, 480), (1000, 480), (1000, 400)],
    );
}

#[test]
fn bent_connector_guides_truncate_like_drawingml() {
    // 333 * 33333 / 100000 = 110.99 -> 110; 77 * 66667 / 100000 = 51.33 -> 51
    check("bentConnector4", &[("adj1", "val 33333"), ("adj2", "val 66667")], 333, 77, &[(0, 0), (110, 0), (110, 51), (333, 51), (333, 77)]);
}

#[test]
fn huge_adjusts_do_not_panic() {
    for preset in ["bentConnector3", "bentConnector4"] {
        for v in ["val 9223372036854775807", "val -9223372036854775808"] {
            let g = geom(preset, &[("adj1", v), ("adj2", v)]);
            let paths = preset_paths(&g, 1_000_000, 1_000_000).unwrap_or_else(|| panic!("{preset} has an outline"));
            assert_eq!(paths.len(), 1);
        }
    }
}
