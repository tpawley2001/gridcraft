//! Printed row metrics (stub).
#[allow(dead_code)] // stub: the tests call it until the layout does
pub(super) fn printed_row_dots(height_pt: f64, _font_name: &str, _font_size: f32) -> u32 {
    (height_pt * 25.0 / 3.0).round() as u32
}
