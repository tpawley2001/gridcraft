//! Freeform shape geometry: DrawingML custom-geometry paths flattened to screen points (curves and
//! arcs included) and closed outlines triangulated for filling, concave ones too.

use egui::{Pos2, Rect, pos2};
use gridcraft_engine::model::{GeometryPath, PathCommand};
use gridcraft_ui_egui::freeform::{outlines, triangulate};

fn path(w: i64, h: i64, commands: Vec<PathCommand>) -> GeometryPath {
    GeometryPath { w, h, fill: true, stroke: true, shade: Default::default(), commands }
}

fn square() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0))
}

fn near(a: Pos2, b: Pos2) -> bool {
    (a - b).length() <= 0.5
}

fn has_point_near(points: &[Pos2], p: Pos2) -> bool {
    points.iter().any(|q| near(*q, p))
}

#[test]
fn cubic_bezier_is_flattened_through_its_midpoint() {
    use PathCommand::*;
    let out = outlines(&[path(100, 100, vec![MoveTo(0, 0), CubicBezTo([(0, 100), (100, 100), (100, 0)])])], square());
    assert_eq!(out.len(), 1);
    let pts = &out[0].points;
    assert!(pts.len() >= 17, "{} points", pts.len());
    assert!(near(pts[0], pos2(0.0, 0.0)));
    assert!(near(*pts.last().unwrap(), pos2(100.0, 0.0)));
    // B(0.5) = (50, 75).
    assert!(has_point_near(pts, pos2(50.0, 75.0)), "{pts:?}");
    assert!(!out[0].closed);
}

#[test]
fn quadratic_bezier_is_flattened_through_its_midpoint() {
    use PathCommand::*;
    let out = outlines(&[path(100, 100, vec![MoveTo(0, 0), QuadBezTo([(50, 100), (100, 0)])])], square());
    let pts = &out[0].points;
    assert!(near(*pts.last().unwrap(), pos2(100.0, 0.0)));
    // Q(0.5) = (50, 50).
    assert!(has_point_near(pts, pos2(50.0, 50.0)), "{pts:?}");
}

#[test]
fn arc_to_sweeps_clockwise_from_the_current_point() {
    use PathCommand::*;
    // Start at angle 0 of a circle of radius 50 centred on (50, 50); sweep 90 degrees.
    let cmds = vec![MoveTo(100, 50), ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: 5_400_000 }];
    let out = outlines(&[path(100, 100, cmds)], square());
    let pts = &out[0].points;
    assert!(near(*pts.last().unwrap(), pos2(50.0, 100.0)), "{pts:?}");
    // 45 degrees: 50 + 50 cos 45 = 85.355.
    assert!(has_point_near(pts, pos2(85.355, 85.355)), "{pts:?}");
}

#[test]
fn path_coordinates_are_stretched_to_the_shape_box() {
    use PathCommand::*;
    let r = Rect::from_min_max(pos2(10.0, 20.0), pos2(210.0, 120.0));
    let out = outlines(&[path(200, 100, vec![MoveTo(0, 0), LineTo(200, 100)])], r);
    assert_eq!(out[0].points.len(), 2);
    assert!(near(out[0].points[0], pos2(10.0, 20.0)));
    assert!(near(out[0].points[1], pos2(210.0, 120.0)));
}

#[test]
fn move_to_starts_a_subpath_and_close_closes_it() {
    use PathCommand::*;
    let cmds = vec![MoveTo(0, 0), LineTo(50, 0), LineTo(50, 50), Close, MoveTo(60, 60), LineTo(90, 90)];
    let out = outlines(&[path(100, 100, cmds)], square());
    assert_eq!(out.len(), 2);
    assert!(out[0].closed);
    assert_eq!(out[0].points.len(), 3);
    assert!(!out[1].closed);
}

#[test]
fn fill_and_stroke_flags_are_kept_and_empty_paths_skipped() {
    use PathCommand::*;
    let mut p = path(100, 100, vec![MoveTo(0, 0), LineTo(100, 0), LineTo(100, 100), Close]);
    p.stroke = false;
    let zero = path(0, 100, vec![MoveTo(0, 0), LineTo(0, 100)]);
    let out = outlines(&[p, zero], square());
    assert_eq!(out.len(), 1);
    assert!(out[0].fill);
    assert!(!out[0].stroke);
}

fn tri_area(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2.0
}

#[test]
fn concave_polygon_triangulates_in_either_winding() {
    let l = vec![pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 50.0), pos2(50.0, 50.0), pos2(50.0, 100.0), pos2(0.0, 100.0)];
    let mut rev = l.clone();
    rev.reverse();
    for pts in [l, rev] {
        let tris = triangulate(&pts).expect("simple polygon");
        assert_eq!(tris.len(), 4);
        let area: f32 = tris.iter().map(|t| tri_area(pts[t[0]], pts[t[1]], pts[t[2]])).sum();
        assert!((area - 7500.0).abs() < 0.01, "area {area}");
        for t in &tris {
            let c = (pts[t[0]].to_vec2() + pts[t[1]].to_vec2() + pts[t[2]].to_vec2()) / 3.0;
            assert!(!(c.x > 50.0 && c.y > 50.0), "triangle {t:?} lies in the notch");
        }
    }
}

#[test]
fn degenerate_and_self_intersecting_polygons_are_not_filled() {
    assert_eq!(triangulate(&[pos2(0.0, 0.0), pos2(50.0, 50.0), pos2(100.0, 100.0)]), None);
    assert_eq!(triangulate(&[pos2(0.0, 0.0), pos2(100.0, 100.0), pos2(100.0, 0.0), pos2(0.0, 100.0)]), None);
    assert_eq!(triangulate(&[pos2(0.0, 0.0), pos2(1.0, 1.0)]), None);
}

#[test]
fn right_arrow_preset_outline_is_drawn_like_a_freeform() {
    use gridcraft_engine::model::{ShapeGeometry, preset_paths};
    let g = ShapeGeometry { preset: "rightArrow".into(), adjust: Vec::new(), paths: Vec::new() };
    let r = Rect::from_min_max(pos2(10.0, 20.0), pos2(210.0, 120.0));
    let paths = preset_paths(&g, 20_000, 10_000).unwrap();
    let out = outlines(&paths, r);
    assert_eq!(out.len(), 1);
    assert!(out[0].closed && out[0].fill && out[0].stroke);
    assert_eq!(out[0].points.len(), 7, "{:?}", out[0].points);
    // The tip on the right edge, the shaft's back on the left.
    assert!(has_point_near(&out[0].points, pos2(210.0, 70.0)), "{:?}", out[0].points);
    assert!(has_point_near(&out[0].points, pos2(10.0, 45.0)), "{:?}", out[0].points);
    // Concave, so filled through triangulation: 5 triangles.
    assert_eq!(triangulate(&out[0].points).map(|t| t.len()), Some(5));
}

#[test]
fn wedge_callout_tail_is_drawn_outside_the_box() {
    use gridcraft_engine::model::{ShapeGeometry, preset_paths};
    let g = ShapeGeometry { preset: "wedgeRectCallout".into(), adjust: Vec::new(), paths: Vec::new() };
    let r = Rect::from_min_max(pos2(10.0, 20.0), pos2(210.0, 140.0));
    let paths = preset_paths(&g, 20_000, 12_000).unwrap();
    let out = outlines(&paths, r);
    assert_eq!(out.len(), 1);
    // The default tip: 20.833% of the width left of centre, 62.5% of the height below it.
    assert!(has_point_near(&out[0].points, pos2(68.34, 155.0)), "{:?}", out[0].points);
}
