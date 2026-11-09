//! Explosion 1 and Explosion 2 outlines (ECMA-376 Part 1 presetShapeDefinitions `irregularSeal1`, `irregularSeal2`).
//! Both are fixed polygons in a 21600 x 21600 path box, scaled to the shape box (`w * x / 21600`, truncating).
//! Polygons are compared up to their starting vertex and direction; each coordinate may differ by one unit.
use gridcraft_model::{GeometryPath, PathCommand, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

/// The one closed path's vertices.
fn polygon(paths: &[GeometryPath]) -> Vec<(i64, i64)> {
    assert_eq!(paths.len(), 1, "one path");
    let cmds = &paths[0].commands;
    assert_eq!(cmds.last(), Some(&PathCommand::Close), "path ends with Close");
    assert!(matches!(cmds.first(), Some(PathCommand::MoveTo(..))), "path starts with MoveTo");
    cmds.iter()
        .filter_map(|c| match *c {
            PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((x, y)),
            PathCommand::Close => None,
            ref other => panic!("unexpected command {other:?}"),
        })
        .collect()
}

fn near(a: (i64, i64), b: (i64, i64)) -> bool {
    (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1
}

fn same_cycle(actual: &[(i64, i64)], expected: &[(i64, i64)]) -> bool {
    let n = expected.len();
    if actual.len() != n {
        return false;
    }
    let rev: Vec<_> = expected.iter().rev().copied().collect();
    [expected.to_vec(), rev].iter().any(|e| (0..n).any(|k| (0..n).all(|i| near(actual[(i + k) % n], e[i]))))
}

fn check(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64, expected: &[(i64, i64)]) {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke), (w, h, true, true), "{preset} path box and flags");
    let got = polygon(&paths);
    assert!(got.iter().all(|&(x, y)| (0..=w).contains(&x) && (0..=h).contains(&y)), "{preset} stays in its box: {got:?}");
    assert!(same_cycle(&got, expected), "{preset} {adjust:?} {w}x{h}: got {got:?}, expected {expected:?}");
}

#[test]
fn explosion1_scales_its_fixed_outline_to_the_box() {
    check(
        "irregularSeal1",
        &[],
        21600,
        21600,
        &[
            (10800, 5800),
            (14522, 0),
            (14155, 5325),
            (18380, 4457),
            (16702, 7315),
            (21097, 8137),
            (17607, 10475),
            (21600, 13290),
            (16837, 12942),
            (18145, 18095),
            (14020, 14457),
            (13247, 19737),
            (10532, 14935),
            (8485, 21600),
            (7715, 15627),
            (4762, 17617),
            (5667, 13937),
            (135, 14587),
            (3722, 11775),
            (0, 8615),
            (4627, 7617),
            (370, 2295),
            (7312, 6320),
            (8352, 2295),
        ],
    );
    check(
        "irregularSeal1",
        &[],
        200,
        100,
        &[
            (100, 26),
            (134, 0),
            (131, 24),
            (170, 20),
            (154, 33),
            (195, 37),
            (163, 48),
            (200, 61),
            (155, 59),
            (168, 83),
            (129, 66),
            (122, 91),
            (97, 69),
            (78, 100),
            (71, 72),
            (44, 81),
            (52, 64),
            (1, 67),
            (34, 54),
            (0, 39),
            (42, 35),
            (3, 10),
            (67, 29),
            (77, 10),
        ],
    );
    check(
        "irregularSeal1",
        &[],
        1000,
        333,
        &[
            (500, 89),
            (672, 0),
            (655, 82),
            (850, 68),
            (773, 112),
            (976, 125),
            (815, 161),
            (1000, 204),
            (779, 199),
            (840, 278),
            (649, 222),
            (613, 304),
            (487, 230),
            (392, 333),
            (357, 240),
            (220, 271),
            (262, 214),
            (6, 224),
            (172, 181),
            (0, 132),
            (214, 117),
            (17, 35),
            (338, 97),
            (386, 35),
        ],
    );
}

#[test]
fn explosion2_scales_its_fixed_outline_to_the_box() {
    check(
        "irregularSeal2",
        &[],
        21600,
        21600,
        &[
            (11462, 4342),
            (14790, 0),
            (14525, 5777),
            (18007, 3172),
            (16380, 6532),
            (21600, 6645),
            (16985, 9402),
            (18270, 11290),
            (16380, 12310),
            (18877, 15632),
            (14640, 14350),
            (14942, 17370),
            (12180, 15935),
            (11612, 18842),
            (9872, 17370),
            (8700, 19712),
            (7527, 18125),
            (4917, 21600),
            (4805, 18240),
            (1285, 17825),
            (3330, 15370),
            (0, 12877),
            (3935, 11592),
            (1172, 8270),
            (5372, 7817),
            (4502, 3625),
            (8550, 6382),
            (9722, 1887),
        ],
    );
    check(
        "irregularSeal2",
        &[],
        200,
        100,
        &[
            (106, 20),
            (136, 0),
            (134, 26),
            (166, 14),
            (151, 30),
            (200, 30),
            (157, 43),
            (169, 52),
            (151, 56),
            (174, 72),
            (135, 66),
            (138, 80),
            (112, 73),
            (107, 87),
            (91, 80),
            (80, 91),
            (69, 83),
            (45, 100),
            (44, 84),
            (11, 82),
            (30, 71),
            (0, 59),
            (36, 53),
            (10, 38),
            (49, 36),
            (41, 16),
            (79, 29),
            (90, 8),
        ],
    );
    check(
        "irregularSeal2",
        &[],
        1000,
        333,
        &[
            (530, 66),
            (684, 0),
            (672, 89),
            (833, 48),
            (758, 100),
            (1000, 102),
            (786, 144),
            (845, 174),
            (758, 189),
            (873, 240),
            (677, 221),
            (691, 267),
            (563, 245),
            (537, 290),
            (457, 267),
            (402, 303),
            (348, 279),
            (227, 333),
            (222, 281),
            (59, 274),
            (154, 236),
            (0, 198),
            (182, 178),
            (54, 127),
            (248, 120),
            (208, 55),
            (395, 98),
            (450, 29),
        ],
    );
}

#[test]
fn explosions_ignore_stray_adjusts_and_tiny_boxes_stay_inside() {
    for preset in ["irregularSeal1", "irregularSeal2"] {
        let plain = preset_paths(&geom(preset, &[]), 200, 100).expect("explosion has an outline");
        let adjusted = preset_paths(&geom(preset, &[("adj", "val 40000")]), 200, 100).expect("explosion has an outline");
        assert_eq!(plain, adjusted, "{preset} has no adjust handles");
        for (w, h) in [(1, 1), (3, 7), (i64::MAX, 2)] {
            let got = polygon(&preset_paths(&geom(preset, &[]), w, h).expect("explosion has an outline"));
            assert!(got.iter().all(|&(x, y)| (0..=w).contains(&x) && (0..=h).contains(&y)), "{preset} {w}x{h}: {got:?}");
        }
    }
    assert!(preset_paths(&geom("cloud", &[]), 200, 100).is_none(), "other presets unchanged");
}
