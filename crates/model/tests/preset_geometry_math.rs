//! Math-symbol presets (ECMA-376 Part 1 presetShapeDefinitions `mathPlus`, `mathMinus`, `mathMultiply`, `mathEqual`,
//! `mathNotEqual`). Expected vertices were computed from the preset guides in floating point, rounded, then pinned into the
//! shape box. Each closed sub-path (MoveTo .. Close) is compared up to its starting vertex and direction; each coordinate may
//! differ by two units (integer guides truncate). Sub-paths are compared in the order the preset lists them.
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

const MATH: [&str; 5] = ["mathPlus", "mathMinus", "mathMultiply", "mathEqual", "mathNotEqual"];

#[test]
fn math_plus_outline() {
    check(
        "mathPlus",
        &[],
        1000000,
        600000,
        &[&[
            (132550, 229440),
            (429440, 229440),
            (429440, 79530),
            (570560, 79530),
            (570560, 229440),
            (867450, 229440),
            (867450, 370560),
            (570560, 370560),
            (570560, 520470),
            (429440, 520470),
            (429440, 370560),
            (132550, 370560),
        ]],
    );
    check(
        "mathPlus",
        &[],
        300000,
        900000,
        &[&[
            (39765, 414720),
            (114720, 414720),
            (114720, 119295),
            (185280, 119295),
            (185280, 414720),
            (260235, 414720),
            (260235, 485280),
            (185280, 485280),
            (185280, 780705),
            (114720, 780705),
            (114720, 485280),
            (39765, 485280),
        ]],
    );
    check(
        "mathPlus",
        &[("adj1", "val 40000")],
        1000000,
        600000,
        &[&[
            (132550, 180000),
            (380000, 180000),
            (380000, 79530),
            (620000, 79530),
            (620000, 180000),
            (867450, 180000),
            (867450, 420000),
            (620000, 420000),
            (620000, 520470),
            (380000, 520470),
            (380000, 420000),
            (132550, 420000),
        ]],
    );
}

#[test]
fn math_minus_outline() {
    check("mathMinus", &[], 1000000, 600000, &[&[(132550, 229440), (867450, 229440), (867450, 370560), (132550, 370560)]]);
    check("mathMinus", &[], 300000, 900000, &[&[(39765, 344160), (260235, 344160), (260235, 555840), (39765, 555840)]]);
    check("mathMinus", &[("adj1", "val 5000")], 1000000, 600000, &[&[(132550, 285000), (867450, 285000), (867450, 315000), (132550, 315000)]]);
}

#[test]
fn math_multiply_outline() {
    check(
        "mathMultiply",
        &[],
        1000000,
        600000,
        &[&[
            (203872, 204610),
            (276478, 83600),
            (500000, 217714),
            (723522, 83600),
            (796128, 204610),
            (637144, 300000),
            (796128, 395390),
            (723522, 516400),
            (500000, 382286),
            (276478, 516400),
            (203872, 395390),
            (362856, 300000),
        ]],
    );
    check(
        "mathMultiply",
        &[],
        300000,
        900000,
        &[&[
            (38583, 227314),
            (105522, 205001),
            (150000, 338435),
            (194478, 205001),
            (261417, 227314),
            (187188, 450000),
            (261417, 672686),
            (194478, 694999),
            (150000, 561565),
            (105522, 694999),
            (38583, 672686),
            (112812, 450000),
        ]],
    );
    check(
        "mathMultiply",
        &[("adj1", "val 40000")],
        1000000,
        1000000,
        &[&[
            (98754, 381596),
            (381596, 98754),
            (500000, 217157),
            (618404, 98754),
            (901246, 381596),
            (782843, 500000),
            (901246, 618404),
            (618404, 901246),
            (500000, 782843),
            (381596, 901246),
            (98754, 618404),
            (217157, 500000),
        ]],
    );
}

#[test]
fn math_equal_outline() {
    check(
        "mathEqual",
        &[],
        1000000,
        600000,
        &[
            &[(132550, 123600), (867450, 123600), (867450, 264720), (132550, 264720)],
            &[(132550, 335280), (867450, 335280), (867450, 476400), (132550, 476400)],
        ],
    );
    check(
        "mathEqual",
        &[],
        300000,
        900000,
        &[
            &[(39765, 185400), (260235, 185400), (260235, 397080), (39765, 397080)],
            &[(39765, 502920), (260235, 502920), (260235, 714600), (39765, 714600)],
        ],
    );
    check(
        "mathEqual",
        &[("adj1", "val 10000"), ("adj2", "val 30000")],
        1000000,
        600000,
        &[
            &[(132550, 150000), (867450, 150000), (867450, 210000), (132550, 210000)],
            &[(132550, 390000), (867450, 390000), (867450, 450000), (132550, 450000)],
        ],
    );
}

#[test]
fn math_not_equal_outline() {
    check(
        "mathNotEqual",
        &[],
        1000000,
        600000,
        &[&[
            (132550, 123600),
            (489116, 123600),
            (534103, 0),
            (666712, 48266),
            (639293, 123600),
            (867450, 123600),
            (867450, 264720),
            (587929, 264720),
            (562248, 335280),
            (867450, 335280),
            (867450, 476400),
            (510884, 476400),
            (465897, 600000),
            (333288, 551734),
            (360707, 476400),
            (132550, 476400),
            (132550, 335280),
            (412071, 335280),
            (437752, 264720),
            (132550, 264720),
        ]],
    );
    check(
        "mathNotEqual",
        &[],
        300000,
        900000,
        &[&[
            (39765, 185400),
            (133674, 185400),
            (201154, 0),
            (300000, 72399),
            (300000, 185400),
            (260235, 185400),
            (260235, 397080),
            (281894, 397080),
            (243371, 502920),
            (260235, 502920),
            (260235, 714600),
            (166326, 714600),
            (98846, 900000),
            (0, 827601),
            (0, 714600),
            (39765, 714600),
            (39765, 502920),
            (18106, 502920),
            (56629, 397080),
            (39765, 397080),
        ]],
    );
    check(
        "mathNotEqual",
        &[("adj1", "val 10000"), ("adj2", "val 7200000"), ("adj3", "val 20000")],
        1000000,
        600000,
        &[&[
            (132550, 180000),
            (511751, 180000),
            (577266, 0),
            (633647, 20521),
            (575602, 180000),
            (867450, 180000),
            (867450, 240000),
            (553764, 240000),
            (510087, 360000),
            (867450, 360000),
            (867450, 420000),
            (488249, 420000),
            (422734, 600000),
            (366353, 579479),
            (424398, 420000),
            (132550, 420000),
            (132550, 360000),
            (446236, 360000),
            (489913, 240000),
            (132550, 240000),
        ]],
    );
    check(
        "mathNotEqual",
        &[("adj2", "val 4200000")],
        1000000,
        600000,
        &[&[
            (132550, 123600),
            (360707, 123600),
            (333288, 48266),
            (465897, 0),
            (510884, 123600),
            (867450, 123600),
            (867450, 264720),
            (562248, 264720),
            (587929, 335280),
            (867450, 335280),
            (867450, 476400),
            (639293, 476400),
            (666712, 551734),
            (534103, 600000),
            (489116, 476400),
            (132550, 476400),
            (132550, 335280),
            (437752, 335280),
            (412071, 264720),
            (132550, 264720),
        ]],
    );
}

#[test]
fn math_vertices_stay_in_hostile_boxes() {
    for preset in MATH {
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            let paths = preset_paths(&geom(preset, &[]), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
            for (x, y) in polygons(&paths).into_iter().flatten() {
                assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} in {w}x{h}: ({x}, {y}) outside the box");
            }
        }
    }
}

#[test]
fn math_hostile_adjustments_do_not_panic() {
    for preset in MATH {
        for name in ["adj1", "adj2", "adj3"] {
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
fn math_with_empty_box_has_no_outline() {
    for preset in MATH {
        assert_eq!(preset_paths(&geom(preset, &[]), 0, 100), None, "{preset}");
        assert_eq!(preset_paths(&geom(preset, &[]), 100, -1), None, "{preset}");
    }
}
