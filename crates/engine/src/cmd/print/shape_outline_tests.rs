//! Freeform shapes (custom geometry, ECMA-376 Part 1 §20.1.9.8) and rounded rectangles print
//! with their outlines, as Excel prints them, not as their bounding boxes.

use gridcraft_model::{GeometryPath, PathCommand, ShapeGeometry, ShapeKind};
use serde_json::json;

use super::*;

/// A session with one red-filled, blue-outlined shape at B2 whose kind and geometry are `kind`
/// and `geometry`.
fn shape_session(kind: ShapeKind, geometry: Option<ShapeGeometry>) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "x"})).unwrap();
    s.execute("insert.shape", json!({"kind": "rectangle", "at": "B2", "width": 144, "height": 96, "fill": "#FF0000", "line": "#0000FF"})).unwrap();
    let d = s.doc_mut().unwrap();
    let sh = std::sync::Arc::make_mut(&mut d.wb).sheet_mut(0).unwrap().shapes.last_mut().unwrap();
    sh.kind = kind;
    sh.geometry = geometry;
    s
}

fn page_stream(s: &mut Session) -> String {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let bytes = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The vertices (`x y m` / `x y l`) of every path painted with the red fill, in order.
fn red_paths(s: &mut Session) -> Vec<Vec<(f32, f32)>> {
    let text = page_stream(s);
    let mut out = Vec::new();
    for part in text.split("1 0 0 rg\n").skip(1) {
        let end = [" B", " f", " S"].iter().filter_map(|op| part.find(op)).min().unwrap_or(part.len());
        let toks: Vec<&str> = part[..end].split_whitespace().collect();
        let mut pts = Vec::new();
        for i in 2..toks.len() {
            if toks[i] == "m" || toks[i] == "l" {
                if let (Ok(x), Ok(y)) = (toks[i - 2].parse::<f32>(), toks[i - 1].parse::<f32>()) {
                    pts.push((x, y));
                }
            }
        }
        out.push(pts);
    }
    out
}

fn path(fill: bool, commands: Vec<PathCommand>) -> GeometryPath {
    GeometryPath { w: 100, h: 100, fill, stroke: true, commands }
}

fn freeform(paths: Vec<GeometryPath>) -> Option<ShapeGeometry> {
    Some(ShapeGeometry { preset: String::new(), adjust: Vec::new(), paths })
}

fn distinct(v: impl Iterator<Item = f32>) -> usize {
    let mut v: Vec<i64> = v.map(|x| (x * 10.0).round() as i64).collect();
    v.sort_unstable();
    v.dedup();
    v.len()
}

#[test]
fn rectangle_prints_four_corners() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 4, "{p:?}");
}

#[test]
fn freeform_without_geometry_prints_its_box() {
    let mut s = shape_session(ShapeKind::Freeform, None);
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 4, "{p:?}");
}

#[test]
fn concave_freeform_prints_its_l_shaped_outline() {
    use PathCommand::*;
    let l_shape = vec![MoveTo(0, 0), LineTo(50, 0), LineTo(50, 50), LineTo(100, 50), LineTo(100, 100), LineTo(0, 100), Close];
    let mut s = shape_session(ShapeKind::Freeform, freeform(vec![path(true, l_shape)]));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 6, "{p:?}");
    // The notch: three distinct x and three distinct y values (a box has two of each).
    assert_eq!(distinct(p[0].iter().map(|v| v.0)), 3, "{p:?}");
    assert_eq!(distinct(p[0].iter().map(|v| v.1)), 3, "{p:?}");
}

#[test]
fn freeform_bezier_is_flattened_into_segments() {
    use PathCommand::*;
    let cmds = vec![MoveTo(0, 100), CubicBezTo([(0, 0), (100, 0), (100, 100)]), Close];
    let mut s = shape_session(ShapeKind::Freeform, freeform(vec![path(true, cmds)]));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].len() >= 9, "a cubic needs at least 8 segments: {p:?}");
}

#[test]
fn freeform_full_arc_is_flattened_into_segments() {
    use PathCommand::*;
    let cmds = vec![MoveTo(100, 50), ArcTo { w_r: 50, h_r: 50, st_ang: 0, sw_ang: 21_600_000 }, Close];
    let mut s = shape_session(ShapeKind::Freeform, freeform(vec![path(true, cmds)]));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].len() >= 24, "a 360° arc needs at least one segment per 15°: {p:?}");
    // A circle spans the box: many distinct x values, not the 2 of a box.
    assert!(distinct(p[0].iter().map(|v| v.0)) > 8, "{p:?}");
}

#[test]
fn each_filled_path_prints_and_an_unfilled_path_is_not_filled() {
    use PathCommand::*;
    let sq = |x: i64| vec![MoveTo(x, 0), LineTo(x + 40, 0), LineTo(x + 40, 40), LineTo(x, 40), Close];
    let mut s = shape_session(ShapeKind::Freeform, freeform(vec![path(true, sq(0)), path(true, sq(60))]));
    assert_eq!(red_paths(&mut s).len(), 2);
    let mut s = shape_session(ShapeKind::Freeform, freeform(vec![path(true, sq(0)), path(false, sq(60))]));
    assert_eq!(red_paths(&mut s).len(), 1);
}

#[test]
fn rounded_rectangle_prints_rounded_corners() {
    let mut s = shape_session(ShapeKind::RoundedRectangle, None);
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].len() >= 16, "four quarter arcs of at least 4 segments: {p:?}");
}

#[test]
fn huge_freeform_prints_with_a_bounded_point_count() {
    use PathCommand::*;
    let mut cmds = vec![MoveTo(0, 0)];
    cmds.extend((0..300_000).map(|i| CubicBezTo([(i % 100, 0), (100, i % 100), (i % 97, 100)])));
    cmds.push(Close);
    let mut s = shape_session(ShapeKind::Freeform, freeform(vec![path(true, cmds)]));
    let p = red_paths(&mut s);
    assert!(p.iter().map(Vec::len).sum::<usize>() <= 200_001, "point cap");
}

fn preset(name: &str) -> Option<ShapeGeometry> {
    Some(ShapeGeometry { preset: name.into(), adjust: Vec::new(), paths: Vec::new() })
}

#[test]
fn right_arrow_preset_prints_its_seven_point_outline() {
    let mut s = shape_session(ShapeKind::Rectangle, preset("rightArrow"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 7, "{p:?}");
    // Head top, shaft top, the tip, shaft bottom and head bottom: five distinct y, three distinct x.
    assert_eq!(distinct(p[0].iter().map(|v| v.1)), 5, "{p:?}");
    assert_eq!(distinct(p[0].iter().map(|v| v.0)), 3, "{p:?}");
}

#[test]
fn diamond_preset_prints_four_points_off_the_corners() {
    let mut s = shape_session(ShapeKind::Rectangle, preset("diamond"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 4, "{p:?}");
    assert_eq!(distinct(p[0].iter().map(|v| v.0)), 3, "{p:?}");
}

#[test]
fn preset_without_an_outline_prints_its_box() {
    let mut s = shape_session(ShapeKind::Rectangle, preset("star5"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 4, "{p:?}");
}
