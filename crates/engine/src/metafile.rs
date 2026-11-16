//! EMF ([MS-EMF]) pictures for print: the records Excel's printouts need, turned into vector
//! paths and bitmaps in picture-relative coordinates.
//!
//! Stub: the task implementing it replaces the body.

/// A parsed metafile: what to draw, in order.
#[derive(Debug, Clone, Default)]
pub struct Drawing {
    pub items: Vec<Item>,
}

/// One drawing item. Coordinates are relative to the picture frame: (0, 0) is its top-left
/// corner, (1, 1) its bottom-right.
#[derive(Debug, Clone)]
pub enum Item {
    /// Subpaths (curves already flattened). A stroke width is a fraction of the frame width.
    Path { subpaths: Vec<Vec<[f32; 2]>>, closed: bool, fill: Option<[u8; 3]>, stroke: Option<([u8; 3], f32)>, even_odd: bool },
    /// A bitmap stretched into `rect` = [x, y, w, h]; RGBA8, rows top-down.
    Image { rect: [f32; 4], rgba: Vec<u8>, size: [u32; 2] },
}

/// Parses an EMF byte stream; `None` when `data` is not an EMF or its header is unusable.
pub fn parse_emf(_data: &[u8]) -> Option<Drawing> {
    None
}
