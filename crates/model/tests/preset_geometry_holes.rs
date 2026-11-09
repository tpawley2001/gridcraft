//! Donut and frame outlines (ECMA-376 Part 1 presetShapeDefinitions): one path holding the outer
//! subpath and the hole, the hole wound the other way. Integer guides truncate; each value may
//! differ by one unit.
use gridcraft_model::{PathCommand, ShapeGeometry, preset_paths};

fn geom(preset: &str, adjust: &[(&str, &str)]) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: adjust.iter().map(|(n, f)| (n.to_string(), f.to_string())).collect(), paths: vec![] }
}

fn near(a: i64, b: i64) -> bool {
    (a - b).abs() <= 1
}

/// The commands of the one filled path, split into subpaths at each MoveTo.
fn subpaths(preset: &str, adjust: &[(&str, &str)], w: i64, h: i64) -> Vec<Vec<PathCommand>> {
    let paths = preset_paths(&geom(preset, adjust), w, h).unwrap_or_else(|| panic!("{preset} has an outline"));
    assert_eq!(paths.len(), 1, "{preset}: one path, so the hole is cut out of the fill");
    assert!(paths[0].fill && paths[0].stroke, "{preset}: filled and stroked");
    assert_eq!((paths[0].w, paths[0].h), (w, h));
    let mut out: Vec<Vec<PathCommand>> = Vec::new();
    for c in &paths[0].commands {
        if matches!(c, PathCommand::MoveTo(..)) || out.is_empty() {
            out.push(Vec::new());
        }
        out.last_mut().unwrap().push(c.clone());
    }
    out
}

fn moveto(c: &PathCommand) -> (i64, i64) {
    match *c {
        PathCommand::MoveTo(x, y) => (x, y),
        ref other => panic!("expected MoveTo, got {other:?}"),
    }
}

/// Signed area (shoelace) of a subpath made of MoveTo/LineTo points.
fn area(sub: &[PathCommand]) -> i64 {
    let pts: Vec<(i64, i64)> = sub
        .iter()
        .filter_map(|c| match *c {
            PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((x, y)),
            PathCommand::Close => None,
            ref other => panic!("unexpected command {other:?}"),
        })
        .collect();
    (0..pts.len()).map(|i| pts[i].0 * pts[(i + 1) % pts.len()].1 - pts[(i + 1) % pts.len()].0 * pts[i].1).sum()
}

/// Checks a donut subpath: MoveTo `start`, four quarter arcs of radii `rx`/`ry` sweeping `sweep`
/// from the given start angles (60000ths of a degree), then Close.
fn check_ring(sub: &[PathCommand], start: (i64, i64), rx: i64, ry: i64, angles: [i64; 4], sweep: i64) {
    assert_eq!(sub.len(), 6, "MoveTo, 4 arcs, Close: {sub:?}");
    let m = moveto(&sub[0]);
    assert!(near(m.0, start.0) && near(m.1, start.1), "start {m:?} vs {start:?}");
    for (i, st) in angles.iter().enumerate() {
        match sub[i + 1] {
            PathCommand::ArcTo { w_r, h_r, st_ang, sw_ang } => {
                assert!(near(w_r, rx) && near(h_r, ry), "arc {i} radii ({w_r}, {h_r}) vs ({rx}, {ry})");
                assert_eq!((st_ang, sw_ang), (*st, sweep), "arc {i} angles");
            }
            ref other => panic!("arc {i}: expected ArcTo, got {other:?}"),
        }
    }
    assert_eq!(sub[5], PathCommand::Close);
}

#[test]
fn donut_is_an_outer_ellipse_with_an_inner_ellipse_wound_the_other_way() {
    // w 1000, h 600, adj 25000: ss 600, dr 150, wd2 500, hd2 300, iwd2 350, ihd2 150.
    let s = subpaths("donut", &[], 1000, 600);
    assert_eq!(s.len(), 2, "{s:?}");
    check_ring(&s[0], (0, 300), 500, 300, [10_800_000, 16_200_000, 0, 5_400_000], 5_400_000);
    check_ring(&s[1], (150, 300), 350, 150, [10_800_000, 5_400_000, 0, 16_200_000], -5_400_000);
}

#[test]
fn donut_adjust_is_pinned_to_half_the_short_side() {
    // adj 80000 pins to 50000: dr 300, iwd2 200, ihd2 0.
    let s = subpaths("donut", &[("adj", "val 80000")], 1000, 600);
    assert_eq!(s.len(), 2, "{s:?}");
    check_ring(&s[1], (300, 300), 200, 0, [10_800_000, 5_400_000, 0, 16_200_000], -5_400_000);
    // adj 10000 on a tall box: ss 400, dr 40, iwd2 160, ihd2 460.
    let s = subpaths("donut", &[("adj", "val 10000")], 400, 1000);
    check_ring(&s[0], (0, 500), 200, 500, [10_800_000, 16_200_000, 0, 5_400_000], 5_400_000);
    check_ring(&s[1], (40, 500), 160, 460, [10_800_000, 5_400_000, 0, 16_200_000], -5_400_000);
}

#[test]
fn frame_is_the_box_with_an_inset_box_wound_the_other_way() {
    // w 1000, h 600, adj1 12500: x1 = 600 * 12500 / 100000 = 75, x4 925, y4 525.
    let s = subpaths("frame", &[], 1000, 600);
    assert_eq!(s.len(), 2, "{s:?}");
    let outer = [(0, 0), (1000, 0), (1000, 600), (0, 600)];
    let inner = [(75, 75), (75, 525), (925, 525), (925, 75)];
    for (sub, pts) in [(&s[0], &outer), (&s[1], &inner)] {
        let got: Vec<(i64, i64)> = sub
            .iter()
            .filter_map(|c| match *c {
                PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((x, y)),
                _ => None,
            })
            .collect();
        assert_eq!(got.len(), 4, "{sub:?}");
        assert!(got.iter().zip(pts.iter()).all(|(a, b)| near(a.0, b.0) && near(a.1, b.1)), "{got:?} vs {pts:?}");
        assert_eq!(sub.last(), Some(&PathCommand::Close));
    }
    assert!(area(&s[0]) * area(&s[1]) < 0, "the hole winds against the outline");
}

#[test]
fn frame_adjust_uses_the_short_side_and_is_pinned() {
    // adj1 20000 on 1000 x 600: x1 120, x4 880, y4 480.
    let s = subpaths("frame", &[("adj1", "val 20000")], 1000, 600);
    assert!(near(moveto(&s[1][0]).0, 120) && near(moveto(&s[1][0]).1, 120), "{s:?}");
    // adj1 70000 pins to 50000: x1 = 300 on 1000 x 600, the hole collapses to a line.
    let s = subpaths("frame", &[("adj1", "val 70000")], 1000, 600);
    assert!(near(moveto(&s[1][0]).0, 300) && near(moveto(&s[1][0]).1, 300), "{s:?}");
}
