//! The last non-connector presets: `snipRoundRect`, `leftRightRibbon` and `flowChartMultidocument`.
//! Paths, guides and fill/stroke flags follow ECMA-376 Part 1 §20.1.9.18 (`prstGeom`) and its preset
//! shape definitions; expected values are the definitions evaluated in the shape box.

use gridcraft_model::PathCommand::{self, ArcTo, Close, LineTo, MoveTo};
use gridcraft_model::{PathShade, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: &PathCommand, b: &PathCommand) -> bool {
    let d = |x: i64, y: i64, tol: i64| (x - y).abs() <= tol;
    match (a, b) {
        (MoveTo(x1, y1), MoveTo(x2, y2)) | (LineTo(x1, y1), LineTo(x2, y2)) => d(*x1, *x2, 2) && d(*y1, *y2, 2),
        (ArcTo { w_r: w1, h_r: h1, st_ang: s1, sw_ang: e1 }, ArcTo { w_r: w2, h_r: h2, st_ang: s2, sw_ang: e2 }) => {
            d(*w1, *w2, 2) && d(*h1, *h2, 2) && d(*s1, *s2, 100) && d(*e1, *e2, 100)
        }
        (Close, Close) => true,
        _ => false,
    }
}

fn starts_with(actual: &[PathCommand], expected: &[PathCommand]) -> bool {
    actual.len() >= expected.len() && actual.iter().zip(expected).all(|(a, b)| near(a, b))
}

const PRESETS: [&str; 3] = ["snipRoundRect", "leftRightRibbon", "flowChartMultidocument"];

#[test]
fn snip_round_rect_outline() {
    // 1000x600, defaults adj1 = adj2 = 16667: ss = 600, x1 = dx2 = 100, x2 = 900.
    let paths = preset_paths(&geom("snipRoundRect", &[]), 1000, 600).expect("snipRoundRect has an outline");
    assert_eq!(paths.len(), 1, "{paths:?}");
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke, p.shade), (1000, 600, true, true, PathShade::Norm));
    let want = [
        MoveTo(100, 0),
        LineTo(900, 0),
        LineTo(1000, 100),
        LineTo(1000, 600),
        LineTo(0, 600),
        LineTo(0, 100),
        ArcTo { w_r: 100, h_r: 100, st_ang: 10_800_000, sw_ang: 5_400_000 },
        Close,
    ];
    assert!(p.commands.len() == want.len() && starts_with(&p.commands, &want), "actual {:?}", p.commands);

    // adj1 = 50000 (round corner at most half the short side), adj2 = 0 (no snip), adj1 past its pin.
    let paths = preset_paths(&geom("snipRoundRect", &[("adj1", "val 80000"), ("adj2", "val 0")]), 1000, 600).unwrap();
    let want = [
        MoveTo(300, 0),
        LineTo(1000, 0),
        LineTo(1000, 0),
        LineTo(1000, 600),
        LineTo(0, 600),
        LineTo(0, 300),
        ArcTo { w_r: 300, h_r: 300, st_ang: 10_800_000, sw_ang: 5_400_000 },
        Close,
    ];
    assert!(starts_with(&paths[0].commands, &want), "actual {:?}", paths[0].commands);
}

#[test]
fn left_right_ribbon_first_path() {
    // 1000x600 defaults: x1 = 300, dy1 = 150, dy2 = -50, ly1 = 100, ly2 = 250, hc = 500.
    let paths = preset_paths(&geom("leftRightRibbon", &[]), 1000, 600).expect("leftRightRibbon has an outline");
    assert!(paths.len() >= 2, "{paths:?}");
    let p = &paths[0];
    assert_eq!((p.w, p.h, p.fill, p.stroke, p.shade), (1000, 600, true, false, PathShade::Norm));
    assert!(starts_with(&p.commands, &[MoveTo(0, 250), LineTo(300, 0), LineTo(300, 100), LineTo(500, 100)]), "actual {:?}", p.commands);
    // The fold is the darkened, unstroked second path.
    assert_eq!((paths[1].fill, paths[1].stroke, paths[1].shade), (true, false, PathShade::DarkenLess));
    // The outline is stroked somewhere.
    assert!(paths.iter().any(|p| p.stroke && !p.fill), "{paths:?}");
}

#[test]
fn multidocument_first_paths() {
    // Path space 21600 x 21600 scaled to 1000x600: (0, 20782) -> (0, 577); (1532, 3675) -> (71, 102).
    let paths = preset_paths(&geom("flowChartMultidocument", &[]), 1000, 600).expect("flowChartMultidocument has an outline");
    assert!(paths.len() >= 2, "{paths:?}");
    assert_eq!((paths[0].w, paths[0].h, paths[0].fill, paths[0].stroke), (1000, 600, true, false));
    assert!(near(&paths[0].commands[0], &MoveTo(0, 577)), "actual {:?}", paths[0].commands);
    assert_eq!((paths[1].fill, paths[1].stroke), (false, true));
    assert!(starts_with(&paths[1].commands, &[MoveTo(0, 102), LineTo(861, 102), LineTo(861, 501)]), "actual {:?}", paths[1].commands);
}

#[test]
fn hostile_boxes_and_empty_box() {
    for preset in PRESETS {
        assert!(preset_paths(&geom(preset, &[]), 0, 600).is_none(), "{preset}: empty box");
        assert!(preset_paths(&geom(preset, &[]), -5, 600).is_none(), "{preset}: negative box");
        for (w, h) in [(1, 1), (7, 3), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            for adj in [&[][..], &[("adj1", "val -99999999"), ("adj2", "val 99999999"), ("adj3", "val 99999999")][..]] {
                let _ = preset_paths(&geom(preset, adj), w, h);
            }
        }
    }
}
