//! Freeform shape geometry: DrawingML custom-geometry paths (ECMA-376 Part 1 §20.1.9.15)
//! flattened to screen points, and closed outlines triangulated for filling.

use egui::{Pos2, Rect};
use gridcraft_engine::model::GeometryPath;

/// One subpath of a freeform in screen coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct FreeformOutline {
    pub points: Vec<Pos2>,
    pub closed: bool,
    pub fill: bool,
    pub stroke: bool,
}

/// Flattens every path with a non-empty coordinate space into screen points inside `r`.
pub fn outlines(_paths: &[GeometryPath], _r: Rect) -> Vec<FreeformOutline> {
    todo!("flatten freeform paths")
}

/// Ear-clipping triangulation of a simple polygon; `None` when degenerate or self-intersecting.
pub fn triangulate(_points: &[Pos2]) -> Option<Vec<[usize; 3]>> {
    todo!("triangulate a simple polygon")
}
