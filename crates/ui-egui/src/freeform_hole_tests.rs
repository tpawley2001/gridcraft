//! Filling a freeform path whose subpaths nest: an inner subpath is a hole (even-odd), so the
//! grid shows a frame or donut with its middle open.

use egui::{Pos2, Rect, pos2};
use gridcraft_engine::model::{GeometryPath, PathCommand};

use super::fill_triangles;

fn square(x0: i64, y0: i64, x1: i64, y1: i64, clockwise: bool) -> Vec<PathCommand> {
    let mut pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    if !clockwise {
        pts.reverse();
    }
    let mut c = vec![PathCommand::MoveTo(pts[0].0, pts[0].1)];
    c.extend(pts[1..].iter().map(|&(x, y)| PathCommand::LineTo(x, y)));
    c.push(PathCommand::Close);
    c
}

fn path(commands: Vec<PathCommand>) -> GeometryPath {
    GeometryPath { w: 100, h: 100, fill: true, stroke: true, shade: Default::default(), commands }
}

fn tri_area(t: &[Pos2; 3]) -> f32 {
    ((t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[2].x - t[0].x) * (t[1].y - t[0].y)).abs() / 2.0
}

fn total(tris: &[[Pos2; 3]]) -> f32 {
    tris.iter().map(tri_area).sum()
}

fn r() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0))
}

#[test]
fn single_square_fills_completely() {
    let t = fill_triangles(&path(square(0, 0, 100, 100, true)), r());
    assert!((total(&t) - 10_000.0).abs() < 1.0, "{}", total(&t));
}

#[test]
fn inner_square_is_a_hole_whichever_way_it_winds() {
    for cw in [false, true] {
        let mut c = square(0, 0, 100, 100, true);
        c.extend(square(25, 25, 75, 75, cw));
        let t = fill_triangles(&path(c), r());
        assert!((total(&t) - 7_500.0).abs() < 1.0, "inner clockwise={cw}: area {}", total(&t));
        for tri in &t {
            let c = pos2((tri[0].x + tri[1].x + tri[2].x) / 3.0, (tri[0].y + tri[1].y + tri[2].y) / 3.0);
            assert!(!(c.x > 25.0 && c.x < 75.0 && c.y > 25.0 && c.y < 75.0), "triangle {tri:?} inside the hole");
        }
    }
}

#[test]
fn island_inside_the_hole_is_filled_again() {
    let mut c = square(0, 0, 100, 100, true);
    c.extend(square(20, 20, 80, 80, false));
    c.extend(square(40, 40, 60, 60, true));
    let t = fill_triangles(&path(c), r());
    // 10000 - 3600 + 400
    assert!((total(&t) - 6_800.0).abs() < 1.0, "{}", total(&t));
}

#[test]
fn side_by_side_subpaths_both_fill() {
    let mut c = square(0, 0, 40, 40, true);
    c.extend(square(60, 60, 100, 100, true));
    let t = fill_triangles(&path(c), r());
    assert!((total(&t) - 3_200.0).abs() < 1.0, "{}", total(&t));
}

#[test]
fn unfilled_path_has_no_triangles() {
    let mut p = path(square(0, 0, 100, 100, true));
    p.fill = false;
    assert!(fill_triangles(&p, r()).is_empty());
}
