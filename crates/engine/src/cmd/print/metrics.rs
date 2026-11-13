//! Printed row heights. Excel does not print a row at its point height: it places rows on the
//! printer's 600-dpi grid, each row a whole number of dots that depends on the workbook's Normal
//! font (the font of cell style 0). A row `px` screen pixels tall (points x 96 / 72) prints
//! `floor((8 x px x D + P) / (8 x P))` dots, where P is the Normal font's default row height in
//! screen pixels and D the printed height of that default row in dots (both measured on Excel's
//! own PDFs). A zoomed sheet prints `dots x scale / 100` rounded half up.
//!
//! A page holds rows while their dots sum to at most the printable height in dots less 8.

use gridcraft_model::{Sheet, Workbook};

/// One printer dot (1/600 inch) in points.
pub(crate) const DOT_PT: f32 = 0.12;
/// Dots of the printable height a page's rows may not use (measured: 5692 of 5700 fit).
const PAGE_SLACK_DOTS: u32 = 8;

/// (font name, size in points, P = default row height in screen pixels, D = its printed dots).
/// One line per font; a font not listed prints with Calibri 11's pair.
const FONTS: &[(&str, f32, u32, u32)] = &[
    ("Calibri", 11.0, 20, 121),
    ("Calibri", 10.0, 17, 109),
    ("Calibri", 12.0, 21, 130),
    ("Arial", 8.0, 15, 85),
    ("Arial", 9.0, 16, 93),
    ("Arial", 10.0, 17, 103),
    ("Arial", 11.0, 19, 114),
    ("Arial", 12.0, 20, 123),
    ("Times New Roman", 8.0, 15, 87),
    ("Times New Roman", 10.0, 17, 106),
    ("Times New Roman", 11.0, 20, 116),
    ("Times New Roman", 12.0, 21, 126),
    ("Tahoma", 8.0, 14, 89),
    ("Tahoma", 10.0, 17, 108),
    ("Courier New", 8.0, 15, 79),
    ("Courier New", 10.0, 18, 96),
    ("Courier New", 12.0, 21, 115),
    ("Courier", 8.0, 16, 85),
    ("Aptos Narrow", 11.0, 20, 127),
    ("Aptos Narrow", 12.0, 21, 137),
    ("MS Sans Serif", 10.0, 17, 103),
    ("Verdana", 10.0, 17, 108),
    ("Cambria", 11.0, 19, 115),
    ("MS PGothic", 11.0, 18, 102),
    ("SimSun", 12.0, 19, 122),
];

/// Calibri 11's pair, for fonts outside [`FONTS`].
const FALLBACK: (u32, u32) = (20, 121);

/// The Normal font's (P, D): default row height in screen pixels and its printed dots.
fn font_pair(font_name: &str, font_size: f32) -> (u32, u32) {
    let name = font_name.trim();
    FONTS.iter().find(|(n, s, ..)| n.eq_ignore_ascii_case(name) && (s - font_size).abs() < 0.01).map_or(FALLBACK, |&(_, _, p, d)| (p, d))
}

/// Printed dots at 100 % of a row `px` screen pixels tall, with the Normal font's (P, D).
fn dots_of_px(px: f64, (p, d): (u32, u32)) -> u32 {
    if !px.is_finite() || px <= 0.0 || p == 0 {
        return 0;
    }
    // Heights come through f32 points-to-pixels conversions: snap to a thousandth of a pixel so
    // a whole-pixel height is exactly whole.
    let px = (px * 1000.0).round() / 1000.0;
    let (p, d) = (f64::from(p), f64::from(d));
    let dots = ((8.0 * px * d + p) / (8.0 * p)).floor();
    // Saturating float-to-int conversion: a hostile height only meets u32::MAX.
    dots.clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Printed dots of a row `height_pt` points tall at 100 % under the Normal font `font_name` at
/// `font_size` (name compared without case; an unknown font uses Calibri 11's pair).
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn printed_row_dots(height_pt: f64, font_name: &str, font_size: f32) -> u32 {
    dots_of_px(height_pt * 96.0 / 72.0, font_pair(font_name, font_size))
}

/// `dots` at print scale `scale` (1.0 = 100 %), rounded half up.
fn scaled(dots: u32, scale: f32) -> u32 {
    let pct = f64::from(scale) * 100.0;
    if !pct.is_finite() || pct <= 0.0 {
        return 0;
    }
    let whole = pct.round();
    let v = if (pct - whole).abs() < 1e-3 {
        // A whole percentage: exact, so a product ending in .5 goes up.
        ((f64::from(dots) * whole + 50.0) / 100.0).floor()
    } else {
        (f64::from(dots) * pct / 100.0 + 0.5).floor()
    };
    v.clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Dots a page of `avail_pt` printable points holds for rows.
pub(crate) fn page_dots(avail_pt: f32) -> u32 {
    let dots = (f64::from(avail_pt) * 600.0 / 72.0 + 1e-6).floor();
    (dots.clamp(0.0, f64::from(u32::MAX)) as u32).saturating_sub(PAGE_SLACK_DOTS)
}

/// The printed height of a sheet's rows: their pixel height for printing and the Normal font's
/// pitch, at a print scale.
#[derive(Clone, Copy)]
pub(crate) struct RowPitch<'a> {
    wb: &'a Workbook,
    sh: &'a Sheet,
    pair: (u32, u32),
    normal_size: f32,
    scale: f32,
}

impl<'a> RowPitch<'a> {
    pub(crate) fn new(wb: &'a Workbook, sh: &'a Sheet, scale: f32) -> Self {
        let normal = &wb.styles.get(gridcraft_model::StyleId::DEFAULT).font;
        RowPitch { wb, sh, pair: font_pair(&normal.name, normal.size), normal_size: normal.size, scale }
    }

    /// The Normal font's default row height in screen pixels.
    fn default_px(&self) -> f32 {
        self.pair.0 as f32
    }

    /// Row `r`'s height in screen pixels for printing. A sheet read from an `.xlsx` file prints
    /// a row whose height is not custom (ECMA-376 Part 1, 18.3.1.73 `row@customHeight`) at the
    /// height that fits its content, not at the stored height: a row of Normal-sized, unwrapped
    /// text prints at the Normal font's default row (likewise the rows a `defaultRowHeight`
    /// without `customHeight` covers, 18.3.1.81). Other sheets print the stored height.
    pub(crate) fn row_px(&self, r: u32) -> f32 {
        let sh = self.sh;
        if !sh.fit_auto_row_heights_in_print {
            return sh.row_height(r);
        }
        if sh.is_row_hidden(r) {
            return 0.0;
        }
        let default = if sh.default_row_custom_height == Some(true) { sh.default_row_height } else { self.default_px() };
        match sh.rows.get(&r) {
            Some(i) if i.custom => i.size.unwrap_or(default),
            Some(gridcraft_model::LineInfo { size: Some(h), .. }) if !self.plain_row(r) => *h,
            _ => default,
        }
    }

    /// Every cell of row `r` is in a font no bigger than the Normal font and does not wrap.
    fn plain_row(&self, r: u32) -> bool {
        self.sh.cells.row(r, 0, gridcraft_core::MAX_COLS - 1).all(|(_, cell)| {
            let st = self.wb.styles.get(cell.style);
            st.font.size <= self.normal_size + 0.01 && !st.align.wrap
        })
    }

    /// Printed dots, at the print scale, of a row `px` screen pixels tall.
    pub(crate) fn dots_px(&self, px: f32) -> u32 {
        scaled(dots_of_px(f64::from(px), self.pair), self.scale)
    }

    /// Printed dots of row `r` at the print scale.
    pub(crate) fn dots(&self, r: u32) -> u32 {
        self.dots_px(self.row_px(r))
    }

    /// Printed height in points of rows `a..=b`.
    pub(crate) fn span_pt(&self, (a, b): (u32, u32)) -> f32 {
        (a..=b).map(|r| self.dots(r) as f64).sum::<f64>() as f32 * DOT_PT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scaled_row_rounds_half_up() {
        assert_eq!(scaled(121, 0.5), 61);
        assert_eq!(scaled(363, 0.5), 182);
        assert_eq!(scaled(121, 0.7), 85);
        assert_eq!(scaled(121, 1.15), 139);
        assert_eq!(scaled(0, 1.0), 0);
        assert_eq!(scaled(u32::MAX, 4.0), u32::MAX);
    }

    #[test]
    fn hostile_heights_do_not_overflow() {
        assert_eq!(dots_of_px(f64::NAN, FALLBACK), 0);
        assert_eq!(dots_of_px(-5.0, FALLBACK), 0);
        assert_eq!(dots_of_px(f64::INFINITY, FALLBACK), 0);
        assert_eq!(dots_of_px(1e300, FALLBACK), u32::MAX);
        assert_eq!(page_dots(684.0), 5692);
        assert_eq!(page_dots(-1.0), 0);
    }
}
