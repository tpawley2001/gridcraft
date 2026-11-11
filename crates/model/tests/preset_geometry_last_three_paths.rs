//! Every path of `snipRoundRect`, `leftRightRibbon` and `flowChartMultidocument` (ECMA-376 Part 1
//! presetShapeDefinitions), evaluated by hand in a 1000 × 600 box; each coordinate within 2 units.

use gridcraft_model::PathCommand::{self, ArcTo, Close, CubicBezTo, LineTo, MoveTo};
use gridcraft_model::{GeometryPath, PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn d(a: i64, b: i64) -> bool {
    (a - b).abs() <= 2
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => d(*x1, *x2) && d(*y1, *y2),
        (ArcTo { w_r: w1, h_r: h1, st_ang: s1, sw_ang: e1 }, ArcTo { w_r: w2, h_r: h2, st_ang: s2, sw_ang: e2 }) => {
            d(*w1, *w2) && d(*h1, *h2) && s1 == s2 && e1 == e2
        }
        (CubicBezTo(p), CubicBezTo(q)) => p.iter().zip(q).all(|(a, b)| d(a.0, b.0) && d(a.1, b.1)),
        (Close, Close) => true,
        _ => false,
    }
}

fn check(p: &GeometryPath, flags: (bool, bool, PathShade), want: &[PathCommand]) {
    assert_eq!((p.w, p.h, p.fill, p.stroke, p.shade), (1000, 600, flags.0, flags.1, flags.2));
    assert!(p.commands.len() == want.len() && p.commands.iter().zip(want).all(|(a, b)| near(a, b)), "actual {:?}", p.commands);
}

const CD4: i64 = 5_400_000;
const CD2: i64 = 10_800_000;
const CD34: i64 = 16_200_000;

#[test]
fn snip_round_rect_snip_only() {
    // adj1 = 0 (no rounding), adj2 = 50000: the snip is half the short side.
    let paths = preset_paths(&geom("snipRoundRect", &[("adj1", "val 0"), ("adj2", "val 50000")]), 1000, 600).unwrap();
    assert_eq!(paths.len(), 1);
    let want = [
        MoveTo(0, 0),
        LineTo(700, 0),
        LineTo(1000, 300),
        LineTo(1000, 600),
        LineTo(0, 600),
        LineTo(0, 0),
        ArcTo { w_r: 0, h_r: 0, st_ang: CD2, sw_ang: CD4 },
        Close,
    ];
    check(&paths[0], (true, true, PathShade::Norm), &want);
}

#[test]
fn left_right_ribbon_every_path() {
    // wd32 = 31.25, x1 = 300, x4 = 700, ly1 = 100, ly2 = 250, ly3 = 400, ly4 = 500, ry1 = 100,
    // ry2 = 200, ry3 = 350, ry4 = 500, hR = 25, x2 = 469, x3 = 531, y1 = 125, y2 = 175.
    let paths = preset_paths(&geom("leftRightRibbon", &[]), 1000, 600).unwrap();
    assert_eq!(paths.len(), 3);
    let arc = |st_ang, sw_ang| ArcTo { w_r: 31, h_r: 25, st_ang, sw_ang };
    let body = vec![
        MoveTo(0, 250),
        LineTo(300, 0),
        LineTo(300, 100),
        LineTo(500, 100),
        arc(CD34, CD2),
        arc(CD34, -CD2),
        LineTo(700, 200),
        LineTo(700, 100),
        LineTo(1000, 350),
        LineTo(700, 600),
        LineTo(700, 500),
        LineTo(500, 500),
        arc(CD4, CD4),
        LineTo(469, 400),
        LineTo(300, 400),
        LineTo(300, 500),
        Close,
    ];
    check(&paths[0], (true, false, PathShade::Norm), &body);
    check(&paths[1], (true, false, PathShade::DarkenLess), &[MoveTo(531, 125), arc(0, CD4), arc(CD34, -CD2), LineTo(531, 200), Close]);
    let mut outline = body;
    outline.extend([MoveTo(531, 125), LineTo(531, 200), MoveTo(469, 175), LineTo(469, 400)]);
    check(&paths[2], (false, true, PathShade::Norm), &outline);
}

#[test]
fn multidocument_every_path() {
    // x scales by 1000/21600, y by 600/21600 (1/36). The wave's control point dips below the box.
    let paths = preset_paths(&geom("flowChartMultidocument", &[]), 1000, 600).unwrap();
    assert_eq!(paths.len(), 3);
    let wave = CubicBezTo([(430, 654), (430, 501), (861, 501)]);
    let back = [MoveTo(71, 102), LineTo(71, 50), LineTo(926, 50), LineTo(926, 451), CubicBezTo([(893, 451), (861, 454), (861, 454)])];
    let top = [MoveTo(138, 50), LineTo(138, 0), LineTo(1000, 0), LineTo(1000, 400), CubicBezTo([(963, 400), (926, 402), (926, 402)])];
    let mut fill = vec![MoveTo(0, 577), wave.clone(), LineTo(861, 102), LineTo(0, 102), Close];
    fill.extend(back.clone());
    fill.extend([LineTo(861, 102), Close]);
    fill.extend(top.clone());
    fill.extend([LineTo(926, 50), Close]);
    check(&paths[0], (true, false, PathShade::Norm), &fill);
    let mut outline = vec![MoveTo(0, 102), LineTo(861, 102), LineTo(861, 501), CubicBezTo([(430, 501), (430, 654), (0, 577)]), Close];
    outline.extend(back);
    outline.extend(top);
    check(&paths[1], (false, true, PathShade::Norm), &outline);
    let edge = [
        MoveTo(0, 577),
        wave,
        LineTo(861, 454),
        CubicBezTo([(861, 454), (893, 451), (926, 451)]),
        LineTo(926, 402),
        CubicBezTo([(926, 402), (963, 400), (1000, 400)]),
        LineTo(1000, 0),
        LineTo(138, 0),
        LineTo(138, 50),
        LineTo(71, 50),
        LineTo(71, 102),
        LineTo(0, 102),
        Close,
    ];
    // Neither filled nor stroked, kept as the definition gives it.
    check(&paths[2], (false, false, PathShade::Norm), &edge);
}

#[test]
fn hostile_adjusts_stay_in_the_box() {
    for preset in ["snipRoundRect", "leftRightRibbon", "flowChartMultidocument"] {
        for (w, h) in [(3, 900), (900, 3), (i64::MAX, i64::MAX)] {
            let adj = [("adj1", "val -99999999"), ("adj2", "val 99999999"), ("adj3", "val 99999999")];
            let paths = preset_paths(&geom(preset, &adj), w, h).unwrap();
            for c in paths.iter().flat_map(|p| &p.commands) {
                if let MoveTo(x, y) | LineTo(x, y) = *c {
                    assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset} {w}x{h}: {c:?}");
                }
            }
        }
    }
}
