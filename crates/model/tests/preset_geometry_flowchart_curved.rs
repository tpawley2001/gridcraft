//! Curved flowchart presets (ECMA-376 Part 1 presetShapeDefinitions `flowChartTerminator`,
//! `flowChartAlternateProcess`, `flowChartDelay`, `flowChartDocument`). Integer guides, truncating.
use PathCommand::{ArcTo, Close, CubicBezTo, LineTo, MoveTo};
use gridcraft_model::{PathCommand, ShapeGeometry, preset_paths};

const CD4: i64 = 5_400_000;
const CD2: i64 = 10_800_000;
const CD34: i64 = 16_200_000;

fn check(preset: &str, w: i64, h: i64, expected: &[PathCommand]) {
    let g = ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] };
    let paths = preset_paths(&g, w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), 1, "{preset}: one path");
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke), (w, h, true, true), "{preset} path box and flags");
    assert_eq!(p.commands, expected, "{preset} {w}x{h}");
}

#[test]
fn terminator_is_a_stadium_from_its_21600_path_box() {
    // 2160 x 1080: x * 2160 / 21600, y * 1080 / 21600.
    check(
        "flowChartTerminator",
        2160,
        1080,
        &[
            MoveTo(347, 0),
            LineTo(1812, 0),
            ArcTo { w_r: 347, h_r: 540, st_ang: CD34, sw_ang: CD2 },
            LineTo(347, 1080),
            ArcTo { w_r: 347, h_r: 540, st_ang: CD4, sw_ang: CD2 },
            Close,
        ],
    );
}

#[test]
fn alternate_process_rounds_its_corners_by_a_sixth_of_the_short_side() {
    // ss = 600, ssd6 = 100.
    let r = |st| ArcTo { w_r: 100, h_r: 100, st_ang: st, sw_ang: CD4 };
    check(
        "flowChartAlternateProcess",
        1200,
        600,
        &[MoveTo(0, 100), r(CD2), LineTo(1100, 0), r(CD34), LineTo(1200, 500), r(0), LineTo(100, 600), r(CD4), Close],
    );
}

#[test]
fn delay_is_a_half_stadium() {
    check(
        "flowChartDelay",
        1200,
        600,
        &[MoveTo(0, 0), LineTo(600, 0), ArcTo { w_r: 600, h_r: 300, st_ang: CD34, sw_ang: CD2 }, LineTo(0, 600), Close],
    );
}

#[test]
fn document_has_a_wavy_bottom_with_a_control_point_below_the_box() {
    // y: 17322 -> 481, 23922 -> 664 (below h = 600, not clamped), 20172 -> 560.
    check("flowChartDocument", 1200, 600, &[MoveTo(0, 0), LineTo(1200, 0), LineTo(1200, 481), CubicBezTo([(600, 481), (600, 664), (0, 560)]), Close]);
}
