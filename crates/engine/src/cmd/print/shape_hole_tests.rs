//! Shapes with holes (donut, frame, a freeform path whose second subpath lies inside the first)
//! print as one filled path, so the hole stays open, as Excel prints them.

use gridcraft_model::{GeometryPath, PathCommand, ShapeGeometry, ShapeKind};
use serde_json::json;

use super::*;

/// A session with one red-filled, blue-outlined 144 x 96 pt shape at B2 with `geometry` (a freeform
/// when it has no preset).
fn shape_session(geometry: ShapeGeometry) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "x"})).unwrap();
    s.execute("insert.shape", json!({"kind": "rectangle", "at": "B2", "width": 144, "height": 96, "fill": "#FF0000", "line": "#0000FF"})).unwrap();
    let d = s.doc_mut().unwrap();
    let sh = std::sync::Arc::make_mut(&mut d.wb).sheet_mut(0).unwrap().shapes.last_mut().unwrap();
    sh.kind = if geometry.preset.is_empty() { ShapeKind::Freeform } else { ShapeKind::Rectangle };
    sh.geometry = Some(geometry);
    s
}

/// Each red-filled painting operation: (number of subpaths, i.e. `m` operators, and the painting
/// operator that ends it: `f`, `f*`, `B` or `B*`).
fn red_fills(s: &mut Session) -> Vec<(usize, String)> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let bytes = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut out = Vec::new();
    for part in text.split("1 0 0 rg\n").skip(1) {
        let toks: Vec<&str> = part.split_whitespace().collect();
        let Some(end) = toks.iter().position(|t| matches!(*t, "f" | "f*" | "B" | "B*" | "S" | "n")) else { continue };
        let moves = toks[..end].iter().filter(|t| **t == "m").count();
        out.push((moves, toks[end].to_string()));
    }
    out
}

fn preset(name: &str) -> ShapeGeometry {
    ShapeGeometry { preset: name.into(), adjust: Vec::new(), paths: Vec::new() }
}

fn square(x0: i64, y0: i64, x1: i64, y1: i64, clockwise: bool) -> Vec<PathCommand> {
    let mut pts = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    if !clockwise {
        pts.reverse();
    }
    let mut c = vec![PathCommand::MoveTo(pts[0].0, pts[0].1)];
    c.extend(pts[1..].iter().map(|&(x, y)| PathCommand::LineTo(x, y)));
    c.push(PathCommand::Close);
    c
}

fn one_fill_with_hole(s: &mut Session) {
    let f = red_fills(s);
    assert_eq!(f.len(), 1, "one red fill for the whole shape: {f:?}");
    assert_eq!(f[0].0, 2, "outline and hole in the same path: {f:?}");
    assert!(f[0].1 == "f*" || f[0].1 == "B*", "even-odd fill keeps the hole open: {f:?}");
}

#[test]
fn donut_prints_its_hole() {
    one_fill_with_hole(&mut shape_session(preset("donut")));
}

#[test]
fn frame_prints_its_hole() {
    one_fill_with_hole(&mut shape_session(preset("frame")));
}

#[test]
fn freeform_subpath_inside_another_prints_as_a_hole() {
    let mut cmds = square(0, 0, 100, 100, true);
    cmds.extend(square(25, 25, 75, 75, false));
    let g = ShapeGeometry {
        preset: String::new(),
        adjust: Vec::new(),
        paths: vec![GeometryPath { w: 100, h: 100, fill: true, stroke: true, commands: cmds }],
    };
    one_fill_with_hole(&mut shape_session(g));
}

#[test]
fn separate_filled_paths_still_print_separately() {
    // Two `a:path` elements are two fills: the second paints over the first, no hole.
    let a = GeometryPath { w: 100, h: 100, fill: true, stroke: true, commands: square(0, 0, 100, 100, true) };
    let b = GeometryPath { w: 100, h: 100, fill: true, stroke: true, commands: square(25, 25, 75, 75, false) };
    let g = ShapeGeometry { preset: String::new(), adjust: Vec::new(), paths: vec![a, b] };
    let f = red_fills(&mut shape_session(g));
    assert_eq!(f.len(), 2, "{f:?}");
    assert!(f.iter().all(|(m, _)| *m == 1), "{f:?}");
}
