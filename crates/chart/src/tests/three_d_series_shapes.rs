//! A 3-D column chart draws each series with its own shape (`c:ser/c:shape`, ECMA-376 Part 1
//! §21.2.2.165) and falls back to the group's (`c:bar3DChart/c:shape`) for series without one.

use super::*;
use gridcraft_model::Chart3d;

const NORTH: Rgba = [20, 96, 130, 255];
const EAST: Rgba = [200, 80, 40, 255];

/// A 3-D clustered column chart with group shape `group` and per-series shapes `own`
/// (index = series index in `sample`: North, East, South).
fn shaped(group: Option<&str>, own: &[Option<&str>]) -> Vec<Prim> {
    let mut c = chart(ChartKind::ColumnClustered);
    c.view_3d =
        Some(Chart3d { rot_x: Some(15), rot_y: Some(20), right_angle_axes: Some(false), shape: group.map(str::to_string), ..Chart3d::default() });
    c.series = own.iter().map(|s| Series { values: "A1:A6".into(), bar_shape: s.map(str::to_string), ..Series::default() }).collect();
    let d = sample(c.kind);
    let p = render(&c, &d, 640.0, 400.0, &ApproxMeasure);
    assert_sane(&p, 640.0, 400.0);
    p
}

/// The outlines of the polygons filled in `color` (a series' front faces).
fn outlines(p: &[Prim], color: Rgba) -> Vec<Vec<[f32; 2]>> {
    p.iter()
        .filter_map(|x| match x {
            Prim::Polygon { pts, fill, .. } if *fill == color => Some(pts.clone()),
            _ => None,
        })
        .collect()
}

fn curved(o: &[Vec<[f32; 2]>]) -> bool {
    o.iter().any(|x| x.len() > 8)
}

fn all_quads(o: &[Vec<[f32; 2]>]) -> bool {
    !o.is_empty() && o.iter().all(|x| x.len() == 4)
}

fn width(pts: &[[f32; 2]]) -> f32 {
    pts.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max) - pts.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min)
}

/// Horizontal spread of the outline's points within 1 px of its top.
fn top_spread(pts: &[[f32; 2]]) -> f32 {
    let top = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let xs: Vec<f32> = pts.iter().filter(|p| p[1] <= top + 1.0).map(|p| p[0]).collect();
    xs.iter().copied().fold(f32::NEG_INFINITY, f32::max) - xs.iter().copied().fold(f32::INFINITY, f32::min)
}

#[test]
fn a_series_cylinder_overrides_the_group_box() {
    let p = shaped(None, &[Some("cylinder"), None, None]);
    let north = outlines(&p, NORTH);
    assert!(!north.is_empty());
    assert!(curved(&north), "North has its own c:shape cylinder: a curved outline");
    assert!(all_quads(&outlines(&p, EAST)), "East has no shape of its own: the group's boxes");
}

#[test]
fn a_series_box_overrides_the_group_cylinder() {
    let p = shaped(Some("cylinder"), &[None, Some("box"), None]);
    assert!(all_quads(&outlines(&p, EAST)), "East has its own c:shape box");
    assert!(curved(&outlines(&p, NORTH)), "North keeps the group's cylinders");
}

#[test]
fn a_series_cone_ends_in_a_point() {
    let p = shaped(Some("box"), &[Some("cone"), None, None]);
    let north = outlines(&p, NORTH);
    let widest = north.iter().max_by(|a, b| width(a).total_cmp(&width(b))).expect("North drawn");
    assert!(width(widest) > 4.0, "a visible solid");
    assert!(top_spread(widest) < 1.0, "North's cone has its apex at the value end, spread {}", top_spread(widest));
    assert!(all_quads(&outlines(&p, EAST)), "East stays a box");
}

#[test]
fn without_series_shapes_the_group_shape_applies_to_all() {
    let p = shaped(Some("cylinder"), &[None, None, None]);
    assert!(curved(&outlines(&p, NORTH)) && curved(&outlines(&p, EAST)));
    let p = shaped(None, &[]);
    assert!(all_quads(&outlines(&p, NORTH)) && all_quads(&outlines(&p, EAST)));
}
