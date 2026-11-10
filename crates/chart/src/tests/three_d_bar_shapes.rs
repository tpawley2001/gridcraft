//! 3-D column charts draw the bar shape `c:shape` asks for: cylinders, cones and pyramids
//! instead of boxes (ECMA-376 Part 1 §21.2.2.165).

use super::*;
use gridcraft_model::Chart3d;

const NORTH: Rgba = [20, 96, 130, 255];

fn shaped(shape: Option<&str>) -> Vec<Prim> {
    let mut c = chart(ChartKind::ColumnClustered);
    c.view_3d =
        Some(Chart3d { rot_x: Some(15), rot_y: Some(20), right_angle_axes: Some(false), shape: shape.map(str::to_string), ..Chart3d::default() });
    let d = sample(c.kind);
    let p = render(&c, &d, 640.0, 400.0, &ApproxMeasure);
    assert_sane(&p, 640.0, 400.0);
    p
}

/// The outlines of the polygons filled in North's front colour.
fn north_outlines(p: &[Prim]) -> Vec<Vec<[f32; 2]>> {
    p.iter()
        .filter_map(|x| match x {
            Prim::Polygon { pts, fill, .. } if *fill == NORTH => Some(pts.clone()),
            _ => None,
        })
        .collect()
}

/// Horizontal spread of the outline's points within 1 px of its top.
fn top_spread(pts: &[[f32; 2]]) -> f32 {
    let top = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let xs: Vec<f32> = pts.iter().filter(|p| p[1] <= top + 1.0).map(|p| p[0]).collect();
    xs.iter().copied().fold(f32::NEG_INFINITY, f32::max) - xs.iter().copied().fold(f32::INFINITY, f32::min)
}

fn width(pts: &[[f32; 2]]) -> f32 {
    pts.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max) - pts.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min)
}

#[test]
fn box_shape_keeps_four_point_faces() {
    for shape in [None, Some("box"), Some("hexagon")] {
        let outlines = north_outlines(&shaped(shape));
        assert!(!outlines.is_empty(), "{shape:?}");
        assert!(outlines.iter().all(|o| o.len() == 4), "{shape:?}: box faces are quads");
    }
}

#[test]
fn cylinder_columns_have_a_curved_outline() {
    let outlines = north_outlines(&shaped(Some("cylinder")));
    assert!(!outlines.is_empty());
    assert!(outlines.iter().any(|o| o.len() > 8), "a cylinder's side follows its end ellipses");
}

#[test]
fn cone_and_pyramid_columns_end_in_a_point() {
    for shape in ["cone", "pyramid"] {
        let outlines = north_outlines(&shaped(Some(shape)));
        assert!(!outlines.is_empty(), "{shape}");
        let widest = outlines.iter().max_by(|a, b| width(a).total_cmp(&width(b))).unwrap();
        assert!(width(widest) > 4.0, "{shape}: a visible solid");
        assert!(top_spread(widest) < 1.0, "{shape}: apex at the value end, spread {}", top_spread(widest));
    }
}
