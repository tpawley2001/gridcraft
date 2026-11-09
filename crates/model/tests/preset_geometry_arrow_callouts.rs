//! Arrow callout presets (ECMA-376 Part 1 presetShapeDefinitions `leftArrowCallout`, `rightArrowCallout`, `upArrowCallout`,
//! `downArrowCallout`, `leftRightArrowCallout`, `upDownArrowCallout`, `quadArrowCallout`). Expected vertices were computed from
//! the preset guides in floating point, rounded, then pinned into the shape box. Each closed sub-path (MoveTo .. Close) is compared up
//! to its starting vertex and direction; each coordinate may differ by two units (integer guides truncate).
use gridcraft_model::{GeometryPath, PathCommand, PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

/// The one path's closed sub-paths, as vertex lists.
fn polygons(paths: &[GeometryPath]) -> Vec<Vec<(i64, i64)>> {
    assert_eq!(paths.len(), 1, "one path");
    let mut out: Vec<Vec<(i64, i64)>> = Vec::new();
    let mut open = false;
    for c in &paths[0].commands {
        match *c {
            PathCommand::MoveTo(x, y) => {
                assert!(!open, "sub-path closed before the next MoveTo");
                out.push(vec![(x, y)]);
                open = true;
            }
            PathCommand::LineTo(x, y) => {
                assert!(open, "LineTo inside a sub-path");
                out.last_mut().expect("sub-path").push((x, y));
            }
            PathCommand::Close => {
                assert!(open, "Close ends a sub-path");
                open = false;
            }
            ref other => panic!("unexpected command {other:?}"),
        }
    }
    assert!(!open, "last sub-path is closed");
    out
}

fn near(a: (i64, i64), b: (i64, i64)) -> bool {
    (a.0 - b.0).abs() <= 2 && (a.1 - b.1).abs() <= 2
}

fn same_cycle(actual: &[(i64, i64)], expected: &[(i64, i64)]) -> bool {
    let n = expected.len();
    if actual.len() != n {
        return false;
    }
    let rev: Vec<_> = expected.iter().rev().copied().collect();
    [expected.to_vec(), rev].iter().any(|e| (0..n).any(|k| (0..n).all(|i| near(actual[(i + k) % n], e[i]))))
}

fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[&[(i64, i64)]]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    let p = &paths[0];
    assert!(p.fill && p.stroke, "{preset}: filled and stroked");
    assert_eq!(p.shade, PathShade::Norm, "{preset}: plain fill");
    assert_eq!((p.w, p.h), (w, h), "{preset}: path box is the shape box");
    let actual = polygons(&paths);
    assert_eq!(actual.len(), expected.len(), "{preset} {adjust:?} in {w}x{h}: sub-path count, actual {actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(same_cycle(a, e), "{preset} {adjust:?} in {w}x{h}:\n actual   {a:?}\n expected {e:?}");
    }
}

const CALLOUTS: [&str; 7] = [
    "leftArrowCallout",
    "rightArrowCallout",
    "upArrowCallout",
    "downArrowCallout",
    "leftRightArrowCallout",
    "upDownArrowCallout",
    "quadArrowCallout",
];

#[test]
fn left_arrow_callout_outline() {
    check(
        "leftArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (150000, 150000),
            (150000, 225000),
            (350230, 225000),
            (350230, 0),
            (1000000, 0),
            (1000000, 600000),
            (350230, 600000),
            (350230, 375000),
            (150000, 375000),
            (150000, 450000),
        ]],
    );
    check(
        "leftArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 450000),
            (75000, 375000),
            (75000, 412500),
            (105069, 412500),
            (105069, 0),
            (300000, 0),
            (300000, 900000),
            (105069, 900000),
            (105069, 487500),
            (75000, 487500),
            (75000, 525000),
        ]],
    );
    check(
        "leftArrowCallout",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 38986")],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (90000, 210000),
            (90000, 255000),
            (610140, 255000),
            (610140, 0),
            (1000000, 0),
            (1000000, 600000),
            (610140, 600000),
            (610140, 345000),
            (90000, 345000),
            (90000, 390000),
        ]],
    );
}

#[test]
fn right_arrow_callout_outline() {
    check(
        "rightArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 0),
            (649770, 0),
            (649770, 225000),
            (850000, 225000),
            (850000, 150000),
            (1000000, 300000),
            (850000, 450000),
            (850000, 375000),
            (649770, 375000),
            (649770, 600000),
            (0, 600000),
        ]],
    );
    check(
        "rightArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 0),
            (194931, 0),
            (194931, 412500),
            (225000, 412500),
            (225000, 375000),
            (300000, 450000),
            (225000, 525000),
            (225000, 487500),
            (194931, 487500),
            (194931, 900000),
            (0, 900000),
        ]],
    );
    check(
        "rightArrowCallout",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 38986")],
        1000000,
        600000,
        &[&[
            (0, 0),
            (389860, 0),
            (389860, 255000),
            (910000, 255000),
            (910000, 210000),
            (1000000, 300000),
            (910000, 390000),
            (910000, 345000),
            (389860, 345000),
            (389860, 600000),
            (0, 600000),
        ]],
    );
}

#[test]
fn up_arrow_callout_outline() {
    check(
        "upArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 210138),
            (425000, 210138),
            (425000, 150000),
            (350000, 150000),
            (500000, 0),
            (650000, 150000),
            (575000, 150000),
            (575000, 210138),
            (1000000, 210138),
            (1000000, 600000),
            (0, 600000),
        ]],
    );
    check(
        "upArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 315207),
            (112500, 315207),
            (112500, 75000),
            (75000, 75000),
            (150000, 0),
            (225000, 75000),
            (187500, 75000),
            (187500, 315207),
            (300000, 315207),
            (300000, 900000),
            (0, 900000),
        ]],
    );
    check(
        "upArrowCallout",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 38986")],
        1000000,
        600000,
        &[&[
            (0, 366084),
            (455000, 366084),
            (455000, 90000),
            (410000, 90000),
            (500000, 0),
            (590000, 90000),
            (545000, 90000),
            (545000, 366084),
            (1000000, 366084),
            (1000000, 600000),
            (0, 600000),
        ]],
    );
}

#[test]
fn down_arrow_callout_outline() {
    check(
        "downArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 0),
            (1000000, 0),
            (1000000, 389862),
            (575000, 389862),
            (575000, 450000),
            (650000, 450000),
            (500000, 600000),
            (350000, 450000),
            (425000, 450000),
            (425000, 389862),
            (0, 389862),
        ]],
    );
    check(
        "downArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 0),
            (300000, 0),
            (300000, 584793),
            (187500, 584793),
            (187500, 825000),
            (225000, 825000),
            (150000, 900000),
            (75000, 825000),
            (112500, 825000),
            (112500, 584793),
            (0, 584793),
        ]],
    );
    check(
        "downArrowCallout",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 38986")],
        1000000,
        600000,
        &[&[
            (0, 0),
            (1000000, 0),
            (1000000, 233916),
            (545000, 233916),
            (545000, 510000),
            (590000, 510000),
            (500000, 600000),
            (410000, 510000),
            (455000, 510000),
            (455000, 233916),
            (0, 233916),
        ]],
    );
}

#[test]
fn left_right_arrow_callout_outline() {
    check(
        "leftRightArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (150000, 150000),
            (150000, 225000),
            (259385, 225000),
            (259385, 0),
            (740615, 0),
            (740615, 225000),
            (850000, 225000),
            (850000, 150000),
            (1000000, 300000),
            (850000, 450000),
            (850000, 375000),
            (740615, 375000),
            (740615, 600000),
            (259385, 600000),
            (259385, 375000),
            (150000, 375000),
            (150000, 450000),
        ]],
    );
    check(
        "leftRightArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 450000),
            (75000, 375000),
            (75000, 412500),
            (77816, 412500),
            (77816, 0),
            (222184, 0),
            (222184, 412500),
            (225000, 412500),
            (225000, 375000),
            (300000, 450000),
            (225000, 525000),
            (225000, 487500),
            (222184, 487500),
            (222184, 900000),
            (77816, 900000),
            (77816, 487500),
            (75000, 487500),
            (75000, 525000),
        ]],
    );
    check(
        "leftRightArrowCallout",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 28873")],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (90000, 210000),
            (90000, 255000),
            (355635, 255000),
            (355635, 0),
            (644365, 0),
            (644365, 255000),
            (910000, 255000),
            (910000, 210000),
            (1000000, 300000),
            (910000, 390000),
            (910000, 345000),
            (644365, 345000),
            (644365, 600000),
            (355635, 600000),
            (355635, 345000),
            (90000, 345000),
            (90000, 390000),
        ]],
    );
}

#[test]
fn up_down_arrow_callout_outline() {
    check(
        "upDownArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 155631),
            (425000, 155631),
            (425000, 150000),
            (350000, 150000),
            (500000, 0),
            (650000, 150000),
            (575000, 150000),
            (575000, 155631),
            (1000000, 155631),
            (1000000, 444369),
            (575000, 444369),
            (575000, 450000),
            (650000, 450000),
            (500000, 600000),
            (350000, 450000),
            (425000, 450000),
            (425000, 444369),
            (0, 444369),
        ]],
    );
    check(
        "upDownArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 233446),
            (112500, 233446),
            (112500, 75000),
            (75000, 75000),
            (150000, 0),
            (225000, 75000),
            (187500, 75000),
            (187500, 233446),
            (300000, 233446),
            (300000, 666554),
            (187500, 666554),
            (187500, 825000),
            (225000, 825000),
            (150000, 900000),
            (75000, 825000),
            (112500, 825000),
            (112500, 666554),
            (0, 666554),
        ]],
    );
    check(
        "upDownArrowCallout",
        &[("adj1", "val 15000"), ("adj2", "val 15000"), ("adj3", "val 15000"), ("adj4", "val 28873")],
        1000000,
        600000,
        &[&[
            (0, 213381),
            (455000, 213381),
            (455000, 90000),
            (410000, 90000),
            (500000, 0),
            (590000, 90000),
            (545000, 90000),
            (545000, 213381),
            (1000000, 213381),
            (1000000, 386619),
            (545000, 386619),
            (545000, 510000),
            (590000, 510000),
            (500000, 600000),
            (410000, 510000),
            (455000, 510000),
            (455000, 386619),
            (0, 386619),
        ]],
    );
}

#[test]
fn quad_arrow_callout_outline() {
    check(
        "quadArrowCallout",
        &[],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (111090, 188910),
            (111090, 244455),
            (259385, 244455),
            (259385, 155631),
            (444455, 155631),
            (444455, 111090),
            (388910, 111090),
            (500000, 0),
            (611090, 111090),
            (555545, 111090),
            (555545, 155631),
            (740615, 155631),
            (740615, 244455),
            (888910, 244455),
            (888910, 188910),
            (1000000, 300000),
            (888910, 411090),
            (888910, 355545),
            (740615, 355545),
            (740615, 444369),
            (555545, 444369),
            (555545, 488910),
            (611090, 488910),
            (500000, 600000),
            (388910, 488910),
            (444455, 488910),
            (444455, 444369),
            (259385, 444369),
            (259385, 355545),
            (111090, 355545),
            (111090, 411090),
        ]],
    );
    check(
        "quadArrowCallout",
        &[],
        300000,
        900000,
        &[&[
            (0, 450000),
            (55545, 394455),
            (55545, 422228),
            (77816, 422228),
            (77816, 233446),
            (122228, 233446),
            (122228, 55545),
            (94455, 55545),
            (150000, 0),
            (205545, 55545),
            (177772, 55545),
            (177772, 233446),
            (222184, 233446),
            (222184, 422228),
            (244455, 422228),
            (244455, 394455),
            (300000, 450000),
            (244455, 505545),
            (244455, 477772),
            (222184, 477772),
            (222184, 666554),
            (177772, 666554),
            (177772, 844455),
            (205545, 844455),
            (150000, 900000),
            (94455, 844455),
            (122228, 844455),
            (122228, 666554),
            (77816, 666554),
            (77816, 477772),
            (55545, 477772),
            (55545, 505545),
        ]],
    );
    check(
        "quadArrowCallout",
        &[("adj1", "val 11109"), ("adj2", "val 11109"), ("adj3", "val 11109"), ("adj4", "val 28873")],
        1000000,
        600000,
        &[&[
            (0, 300000),
            (66654, 233346),
            (66654, 266673),
            (355635, 266673),
            (355635, 213381),
            (466673, 213381),
            (466673, 66654),
            (433346, 66654),
            (500000, 0),
            (566654, 66654),
            (533327, 66654),
            (533327, 213381),
            (644365, 213381),
            (644365, 266673),
            (933346, 266673),
            (933346, 233346),
            (1000000, 300000),
            (933346, 366654),
            (933346, 333327),
            (644365, 333327),
            (644365, 386619),
            (533327, 386619),
            (533327, 533346),
            (566654, 533346),
            (500000, 600000),
            (433346, 533346),
            (466673, 533346),
            (466673, 386619),
            (355635, 386619),
            (355635, 333327),
            (66654, 333327),
            (66654, 366654),
        ]],
    );
}

#[test]
fn callouts_vertices_stay_in_hostile_boxes() {
    for preset in CALLOUTS {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            for (x, y) in polygons(&paths).into_iter().flatten() {
                assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} in {w}x{h}: ({x}, {y}) outside the box");
            }
        }
    }
}

#[test]
fn callouts_hostile_adjustments_do_not_panic() {
    for preset in CALLOUTS {
        for name in ["adj", "adj1", "adj2", "adj3", "adj4"] {
            for v in ["-9223372036854775808", "-5", "9223372036854775807", "21600000"] {
                let paths = preset_paths(&geom(preset, &[(name, &format!("val {v}"))]), 1000, 700).expect("supported preset");
                for (x, y) in polygons(&paths).into_iter().flatten() {
                    assert!((0..=1000).contains(&x) && (0..=700).contains(&y), "{preset} {name}={v}: ({x}, {y}) outside the box");
                }
            }
        }
    }
}

#[test]
fn callouts_with_empty_box_has_no_outline() {
    for preset in CALLOUTS {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}
