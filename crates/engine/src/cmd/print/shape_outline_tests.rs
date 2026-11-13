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
    for paint in tests::pdf_paints(&text).into_iter().filter(|p| p.fills() && p.fill == Some([1.0, 0.0, 0.0])) {
        let toks: Vec<&str> = paint.path.split_whitespace().collect();
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
    GeometryPath { w: 100, h: 100, fill, stroke: true, shade: Default::default(), commands }
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
    let mut s = shape_session(ShapeKind::Rectangle, preset("notAPreset"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 4, "{p:?}");
}

#[test]
fn octagon_preset_prints_eight_points_with_cut_corners() {
    let mut s = shape_session(ShapeKind::Rectangle, preset("octagon"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 8, "{p:?}");
    // Each side's two ends are inset from the corners: four distinct x and four distinct y.
    assert_eq!(distinct(p[0].iter().map(|v| v.0)), 4, "{p:?}");
    assert_eq!(distinct(p[0].iter().map(|v| v.1)), 4, "{p:?}");
}

#[test]
fn triangle_preset_prints_three_points_with_a_centred_apex() {
    let mut s = shape_session(ShapeKind::Rectangle, preset("triangle"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 3, "{p:?}");
    assert_eq!(distinct(p[0].iter().map(|v| v.0)), 3, "{p:?}");
    assert_eq!(distinct(p[0].iter().map(|v| v.1)), 2, "{p:?}");
}

/// Each painted path's colour operator line and its path line (`… m … l … h f`), in order: with
/// ` rg` the paths that are only filled, with ` RG` the stroked ones. The colour is the one in
/// effect for the path (the writer does not repeat an unchanged colour operator).
fn painted(text: &str, op: &str) -> Vec<(String, String)> {
    tests::pdf_paints(text)
        .into_iter()
        .filter(|p| p.path.contains(" m "))
        .filter_map(|p| match op {
            " rg" if matches!(p.op.as_str(), "f" | "f*") => Some((p.fill_op(), p.path)),
            " RG" if p.strokes() => Some((p.stroke_op(), p.path)),
            _ => None,
        })
        .collect()
}

fn rgb(op: &str) -> [f32; 3] {
    let v: Vec<f32> = op.split_whitespace().take(3).map(|t| t.parse().unwrap()).collect();
    [v[0], v[1], v[2]]
}

#[test]
fn cube_preset_prints_three_shaded_faces_and_its_outline() {
    let mut s = shape_session(ShapeKind::Rectangle, preset("cube"));
    let text = page_stream(&mut s);
    // The faces are filled without a stroke (`stroke="false"` in the preset): ` h f`.
    let faces: Vec<[f32; 3]> = painted(&text, " rg").into_iter().filter(|(_, p)| p.ends_with(" h f")).map(|(c, _)| rgb(&c)).collect();
    assert_eq!(faces.len(), 3, "{faces:?}\n{text}");
    let (front, side, top) = (faces[0], faces[1], faces[2]);
    assert_eq!(front, [1.0, 0.0, 0.0]);
    assert!(front != side && side != top && front != top, "{faces:?}");
    assert!(side[0] < front[0] && side[1] <= front[1] && side[2] <= front[2], "side darker: {faces:?}");
    assert!(top[0] >= front[0] && top[1] > front[1] && top[2] > front[2], "top lighter: {faces:?}");
    // The outline: the closed silhouette and the two open inner edges, stroked blue.
    let strokes: Vec<String> = painted(&text, " RG").into_iter().filter(|(c, _)| c.starts_with("0 0 1 RG")).map(|(_, p)| p).collect();
    assert_eq!(strokes.iter().filter(|p| p.ends_with(" h S")).count(), 1, "{strokes:?}");
    assert_eq!(strokes.iter().filter(|p| p.ends_with(" S") && !p.ends_with(" h S")).count(), 2, "{strokes:?}");
}

#[test]
fn wedge_callout_prints_its_tail_below_the_box() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let rect = red_paths(&mut s);
    let mut s = shape_session(ShapeKind::Rectangle, preset("wedgeRectCallout"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].len(), 16, "{p:?}");
    // PDF y grows upward: the box's bottom edge is its lowest y, and the tail tip lies an eighth
    // of the printed box height below it.
    let bottom = rect[0].iter().map(|v| v.1).fold(f32::INFINITY, f32::min);
    let top = rect[0].iter().map(|v| v.1).fold(f32::NEG_INFINITY, f32::max);
    let lowest = p[0].iter().map(|v| v.1).fold(f32::INFINITY, f32::min);
    assert!((bottom - lowest - (top - bottom) / 8.0).abs() < 0.5, "box {bottom}..{top}, tip {lowest}: {p:?}");
}

#[test]
fn line_callout_prints_its_leader_left_of_the_box() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let rect = red_paths(&mut s);
    let left = rect[0].iter().map(|v| v.0).fold(f32::INFINITY, f32::min);
    let width = rect[0].iter().map(|v| v.0).fold(f32::NEG_INFINITY, f32::max) - left;
    let mut s = shape_session(ShapeKind::Rectangle, preset("accentBorderCallout1"));
    let text = page_stream(&mut s);
    // The box, the accent bar and the leader are stroked blue; the bar's empty first subpath
    // prints nothing.
    let strokes: Vec<String> = painted(&text, " RG").into_iter().filter(|(c, _)| c.starts_with("0 0 1 RG")).map(|(_, p)| p).collect();
    assert_eq!(strokes.len(), 3, "{strokes:?}");
    let xs: Vec<f32> = strokes[2]
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(3)
        .filter(|t| t[2] == "m" || t[2] == "l")
        .filter_map(|t| t[0].parse().ok())
        .collect();
    assert_eq!(xs.len(), 2, "{strokes:?}");
    // Not clipped to the box: the leader ends 38.333% of the box width left of it.
    let end = xs.iter().copied().fold(f32::INFINITY, f32::min);
    assert!((left - end - width * 0.38333).abs() < 0.5, "box from {left}, width {width}, leader {xs:?}");
}

#[test]
fn moon_prints_its_inner_arc_back_to_the_start_corner() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let rect = red_paths(&mut s);
    let (left, right) = (rect[0].iter().map(|v| v.0).fold(f32::INFINITY, f32::min), rect[0].iter().map(|v| v.0).fold(f32::NEG_INFINITY, f32::max));
    let mut s = shape_session(ShapeKind::Rectangle, preset("moon"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 1, "{p:?}");
    // The arcs' angles are visual angles: the inner arc, wider than the box, ends where the outline
    // started, and its leftmost point is the box's middle.
    let (first, last) = (p[0][0], p[0][p[0].len() - 1]);
    assert!((first.0 - last.0).abs() < 0.5 && (first.1 - last.1).abs() < 0.5, "{p:?}");
    let mid = (left + right) / 2.0;
    assert!(p[0].iter().any(|v| (v.0 - mid).abs() < 0.5), "{p:?}");
}

#[test]
fn cloud_callout_prints_its_bubbles_below_the_box() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let rect = red_paths(&mut s);
    let bottom = rect[0].iter().map(|v| v.1).fold(f32::INFINITY, f32::min);
    let mut s = shape_session(ShapeKind::Rectangle, preset("cloudCallout"));
    let p = red_paths(&mut s);
    assert_eq!(p.len(), 4, "the cloud and three bubbles: {p:?}");
    // PDF y grows upward: the smallest bubble lies below the box's bottom edge.
    let lowest = p[1].iter().map(|v| v.1).fold(f32::INFINITY, f32::min);
    assert!(lowest < bottom - 1.0, "box bottom {bottom}, bubbles {lowest}: {p:?}");
}

/// A shape's preset dash (`a:prstDash`) prints as a PDF dash array in units of the outline width.
#[test]
fn a_dashed_outline_prints_its_dash_pattern_scaled_by_the_width() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let sh = s.doc().unwrap().wb.sheet(0).unwrap().shapes.last().unwrap().clone();
    let mut dashed = sh.clone();
    dashed.format.line_dash = Some(gridcraft_model::LineDash::SysDash);
    assert_eq!(objects::outline_dash(&dashed, 1.0), vec![3.0, 1.0], "1 pt sysDash: 3 pt on, 1 pt off");
    assert_eq!(objects::outline_dash(&dashed, 2.0), vec![6.0, 2.0]);
    assert!(objects::outline_dash(&sh, 1.0).is_empty(), "an unstated dash is solid");

    {
        let d = s.doc_mut().unwrap();
        let shp = std::sync::Arc::make_mut(&mut d.wb).sheet_mut(0).unwrap().shapes.last_mut().unwrap();
        shp.format.line_width_emu = Some(25400);
        shp.format.line_dash = Some(gridcraft_model::LineDash::SysDash);
    }
    let text = page_stream(&mut s);
    // The blue outline is stroked with a [3w 1w] dash array.
    let part = text.split("0 0 1 RG\n").nth(1).expect("the outline is stroked in blue");
    let toks: Vec<&str> = part.split_whitespace().take(6).collect();
    assert_eq!(toks.get(1), Some(&"w"), "{part}");
    let w: f32 = toks[0].parse().unwrap();
    let on: f32 = toks[2].trim_start_matches('[').parse().unwrap();
    let off: f32 = toks[3].trim_end_matches(']').parse().unwrap();
    assert!(w > 0.0 && (on - 3.0 * w).abs() < 0.01 && (off - w).abs() < 0.01, "{part}");
}

/// Without a dash the outline prints solid, as before.
#[test]
fn an_undashed_outline_prints_solid() {
    let mut s = shape_session(ShapeKind::Rectangle, None);
    let text = page_stream(&mut s);
    let part = text.split("0 0 1 RG\n").nth(1).expect("the outline is stroked in blue");
    assert!(part.split_whitespace().nth(2) == Some("[]"), "{part}");
}
