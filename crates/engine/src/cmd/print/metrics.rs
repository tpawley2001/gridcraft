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
/// One line per font; a font not listed prints with Calibri 11's pair, except the names Excel
/// prints in Arial, which use Arial's pair at that size when measured here.
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
    ("SimSun", 11.0, 18, 113),
    ("SimSun", 12.0, 19, 122),
    ("Yu Gothic", 10.0, 22, 139),
    ("Yu Gothic", 11.0, 25, 154),
    ("DengXian", 11.0, 19, 104),
    ("PMingLiU", 11.0, 19, 102),
    ("PMingLiU", 12.0, 20, 110),
    ("Trebuchet MS", 11.0, 22, 124),
    ("Garamond", 11.0, 20, 111),
    ("Century Gothic", 9.0, 19, 99),
    ("Arial Cyr", 10.0, 17, 103),
    ("Calibri", 14.0, 25, 150),
];

/// (localized name, English name): the names East Asian editions of Excel store for fonts in
/// [`FONTS`]. Matched exactly (full-width characters, no case folding).
const ALIASES: &[(&str, &str)] =
    &[("ＭＳ Ｐゴシック", "MS PGothic"), ("游ゴシック", "Yu Gothic"), ("宋体", "SimSun"), ("等线", "DengXian"), ("新細明體", "PMingLiU")];

/// The trimmed font name, with a localized name in [`ALIASES`] replaced by its English name.
fn english_name(font_name: &str) -> &str {
    let name = font_name.trim();
    ALIASES.iter().find(|(local, _)| *local == name).map_or(name, |&(_, english)| english)
}

/// Resolve row metrics for the sans-serif names Excel prints in Arial, using the same name
/// normalization and substitution group as `layout::digit_em`. Cambria only shares Arial's
/// digit width: it has its own measured row metrics and must keep them.
fn row_font_name(font_name: &str) -> &str {
    let name = english_name(font_name);
    let normalized = name.to_lowercase();
    let normalized = normalized
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| normalized.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .unwrap_or(&normalized);
    match normalized {
        "liberation sans" | "sans" | "sans-serif" | "sans serif" | "geneva" | "clear sans" | "droid sans fallback" => "Arial",
        n if n.starts_with("helvetica") => "Arial",
        _ => name,
    }
}

/// Calibri 11's pair, for fonts outside [`FONTS`].
const FALLBACK: (u32, u32) = (20, 121);

/// (font name, size in points, P = default row height in screen pixels) for fonts whose printed
/// dots D are not measured, so they cannot go in [`FONTS`]. Only a cell font's P is needed: the
/// row it fits still prints with the Normal font's pitch.
const ROW_PX: &[(&str, f32, u32)] = &[
    // A Calibri 11 sheet with cells in Calibri 20 prints 26 rows a page (35 or 36 px both give 26).
    ("Calibri", 20.0, 35),
    // Unverified, most likely Excel's default row for Arial 14 (18 pt): with a superscript's +4 px
    // it gives the 28 px measured for an Arial 14 superscript run in an Arial 10 cell.
    ("Arial", 14.0, 24),
];

/// Pixels a superscript or subscript (cell font or string run) of `font_size` points adds to its
/// font's default row height when Excel fits an `.xlsx` row. Measured in Excel's print of rows with
/// no stored height (Normal font Arial 10, 17 px rows): an Arial 10 superscript fits 19 px (+2), an
/// Arial 10 subscript 21 px (+4, 15.75 pt), and an Arial 14 superscript run in an Arial 10 cell
/// 28 px (21 pt; Arial 14's own row taken as 24 px, so +4). Unverified: superscript between 11 and
/// 14 pt (taken as +4 from 12 pt) and subscript above 10 pt (taken as +4).
fn script_extra_px(font_size: f32, subscript: bool) -> f32 {
    if subscript || font_size >= 12.0 - 0.01 { 4.0 } else { 2.0 }
}

/// Pixels a row's `thickBot` flag (ECMA-376 Part 1, 18.3.1.73; `fExDes` of the `.xls` ROW record)
/// adds to a fitted row that carries a stored height (`row@ht`). Measured in Excel with Calibri 11:
/// a row `ht="15.75" thickBot="1"` prints 21 px instead of 20 whatever its cells' borders are (none,
/// double, medium or thick bottom), and a row `ht="15"` without the flag prints 20 px even with a
/// medium bottom border; Excel's re-save keeps the flag as read. With Arial 10 and a thick bottom
/// border Excel's own saves go 17 -> 18 px. On a row with no `ht` nothing is added.
const THICK_BOTTOM_EXTRA_PX: f32 = 1.0;

/// Pixels a row's `thickTop` flag (ECMA-376 Part 1, 18.3.1.73; `fExAsc` of the `.xls` ROW record)
/// adds to a fitted row that carries a stored height, like [`THICK_BOTTOM_EXTRA_PX`]. Measured with
/// Trebuchet MS 11 (22 -> 23 px) and Calibri 11 (20 -> 21 px); with `thickBot` too the two add up
/// (Trebuchet MS 11: 24 px). On a row with no `ht` nothing is added.
const THICK_TOP_EXTRA_PX: f32 = 1.0;

/// The default row height in screen pixels of a cell font (P), from [`FONTS`] or [`ROW_PX`];
/// names Excel prints in Arial use Arial's entry at that size. Returns `None` when neither
/// table lists the resolved font and size.
pub(crate) fn cell_font_px(font_name: &str, font_size: f32) -> Option<u32> {
    let name = row_font_name(font_name);
    let same = |n: &str, s: f32| n.eq_ignore_ascii_case(name) && (s - font_size).abs() < 0.01;
    FONTS.iter().find(|(n, s, ..)| same(n, *s)).map(|&(_, _, p, _)| p).or_else(|| ROW_PX.iter().find(|(n, s, _)| same(n, *s)).map(|&(_, _, p)| p))
}

/// The Normal font's (P, D): default row height in screen pixels and its printed dots.
/// Names Excel prints in Arial use Arial's pair at that size; a font and size not listed in
/// [`FONTS`] use Calibri 11's pair.
fn font_pair(font_name: &str, font_size: f32) -> (u32, u32) {
    let name = row_font_name(font_name);
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
/// `font_size` (name compared without case, with Arial substitutions as in [`font_pair`];
/// an unlisted font and size uses Calibri 11's pair).
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

    /// The baseline of an automatic row on an `.xlsx` sheet whose `defaultRowHeight` is not custom
    /// (ECMA-376 Part 1, 18.3.1.81): the Normal font's default row, unless the saved default is
    /// positive and shorter than the Normal font's own size in points, which it then stays.
    ///
    /// Excel prints such rows at Normal's height when the saved default is a little or even a
    /// quarter below it (Calibri 11 saved 14.5 pt prints 15 pt; Yu Gothic 10 saved 12.8 pt prints
    /// 16.5 pt), but keeps a saved default shorter than the font itself (Arial 10 saved 9.6 pt
    /// prints at about 9 pt). A row shorter than its font cannot be Normal's fitted height, so it
    /// reads as chosen. Unverified: the exact cut-off between those observations.
    fn automatic_default_px(&self) -> f32 {
        let saved = self.sh.default_row_height;
        let font_px = self.normal_size * 96.0 / 72.0;
        if saved.is_finite() && saved > 0.0 && saved < font_px { saved.min(self.default_px()) } else { self.default_px() }
    }

    /// Row `r`'s height in screen pixels for printing. A sheet read from an `.xlsx` file prints
    /// a row whose height is not custom (ECMA-376 Part 1, 18.3.1.73 `row@customHeight`) at the
    /// height that fits its content, not at the stored height: the default row height of the
    /// largest font in it, never below the sheet's default (likewise the rows a `defaultRowHeight`
    /// without `customHeight` covers, 18.3.1.81). A non-custom sheet default counts at
    /// Normal's default row height, unless it is shorter than the Normal font itself
    /// ([`Self::automatic_default_px`]). Wrapped text with a value fits the lines it wraps to; a row
    /// with wrapped cells that print no value, or with a font bigger than Normal whose default row
    /// is not known, keeps its stored height. Other sheets print the stored height.
    ///
    /// With `fit_print_rows_without_default_floor` (sheets read from `.xls`, whose ROW records
    /// without fUnsynced Excel re-fits on open, [MS-XLS] 2.4.221) a non-custom row prints at the
    /// largest default row height of its cell fonts, and of the format its empty cells show (the row's
    /// own format, else the column formats on its columns without a cell), with no lower bound; a row
    /// with no cells that count keeps its stored height, else the sheet default.
    pub(crate) fn row_px(&self, r: u32) -> f32 {
        let sh = self.sh;
        if !sh.fit_auto_row_heights_in_print {
            return sh.row_height(r);
        }
        if sh.is_row_hidden(r) {
            return 0.0;
        }
        if sh.fit_print_rows_without_default_floor {
            if let Some(i) = sh.rows.get(&r).filter(|i| i.custom) {
                return i.size.unwrap_or(sh.default_row_height);
            }
            return self.fitted_px_no_floor(r).unwrap_or_else(|| sh.row_height(r));
        }
        let default = if sh.default_row_custom_height == Some(true) { sh.default_row_height } else { self.automatic_default_px() };
        let stored = match sh.rows.get(&r) {
            Some(i) if i.custom => return i.size.unwrap_or(default),
            Some(i) => i.size,
            None => None,
        };
        self.fitted_px(r, default).unwrap_or_else(|| stored.unwrap_or(default))
    }

    /// The height row `r` fits its fonts to, at least `default`: `None` when the row holds a font
    /// bigger than the Normal font whose default row height is not known, or a cell that folds text
    /// (wrapped, justified or distributed) but prints no value, is skipped, is rotated or has a font
    /// whose default row is not known.
    ///
    /// A folding cell with a value needs the lines its text wraps to in its printed width (explicit
    /// line breaks always break; [`super::text::folded_line_count`]) times its font's default row:
    /// Excel re-fits such `.xlsx` rows without `customHeight` in both directions (measured: stored
    /// 18 pt rows of one Arial 10 line print at 12.75 pt, 28 pt rows of two lines at 25.5 pt).
    ///
    /// A counted cell whose font is superscript or subscript needs its font's default row plus
    /// [`script_extra_px`]; a superscript or subscript run of its string needs the run font's default
    /// row plus the extra for the run's size (a run with no `rFont` or `sz` takes the cell font's; a
    /// run font whose row is not known counts as the cell font). On a row that carries a stored
    /// height, the row's `thickBot` flag adds [`THICK_BOTTOM_EXTRA_PX`] to its plain font need and
    /// its `thickTop` flag adds [`THICK_TOP_EXTRA_PX`] (the two add up), whatever its cells' borders
    /// are. On a row without a stored height neither adds anything. The larger need wins.
    fn fitted_px(&self, r: u32, default: f32) -> Option<f32> {
        let sh = self.sh;
        let info = sh.rows.get(&r).filter(|i| i.size.is_some());
        let thick_bottom = info.is_some_and(|i| i.thick_bottom);
        let thick_top = info.is_some_and(|i| i.thick_top);
        let mut need = default;
        let mut script_need = 0.0f32;
        for (c, cell) in sh.cells.row(r, 0, gridcraft_core::MAX_COLS - 1) {
            let st = self.wb.styles.get(cell.style);
            let pos = gridcraft_core::CellRef::new(r, c);
            if st.align.folds() {
                // Folded text with a value needs its lines at the cell font's default row each;
                // without a value (or skipped, rotated, or in a font whose row is not known) the
                // row keeps its stored height.
                if self.skipped(r, c) {
                    return None;
                }
                let lines = super::text::folded_line_count(self.wb, sh, pos, st)?;
                if lines == 0 {
                    return None;
                }
                let p = cell_font_px(&st.font.name, st.font.size)? as f32;
                need = need.max(p * lines.min(u16::MAX as usize) as f32);
            }
            let font_script = st.font.vert != gridcraft_model::VertAlign::Baseline;
            let run_script = !sh.script_run_cells.is_empty() && sh.script_run_cells.contains(&pos);
            let px = cell_font_px(&st.font.name, st.font.size);
            let matters = match px {
                Some(p) => p as f32 > need || font_script || run_script,
                None => st.font.size > self.normal_size + 0.01,
            };
            if !matters {
                continue;
            }
            if self.skipped(r, c) {
                continue;
            }
            // A bigger font whose default row is not known: the row keeps its stored height.
            let p = px? as f32;
            need = need.max(p);
            if font_script {
                let subscript = st.font.vert == gridcraft_model::VertAlign::Subscript;
                script_need = script_need.max(p + script_extra_px(st.font.size, subscript));
            }
            if run_script {
                script_need = script_need.max(self.run_need(pos, &st.font, p));
            }
        }
        let mut plain = need;
        if thick_bottom {
            plain += THICK_BOTTOM_EXTRA_PX;
        }
        if thick_top {
            plain += THICK_TOP_EXTRA_PX;
        }
        Some(plain.max(script_need))
    }

    /// What the superscript and subscript runs of the string in `pos` need: the largest of each run
    /// font's default row plus [`script_extra_px`]. A run font whose row is not known, or a cell
    /// whose run fonts were not recorded, needs the cell font's row `cell_px` plus the extra for the
    /// cell font's size.
    fn run_need(&self, pos: gridcraft_core::CellRef, cell_font: &gridcraft_model::Font, cell_px: f32) -> f32 {
        let fallback = |subscript: bool| cell_px + script_extra_px(cell_font.size, subscript);
        let Some(runs) = self.sh.script_run_fonts.get(&pos).filter(|runs| !runs.is_empty()) else {
            return fallback(false);
        };
        runs.iter()
            .map(|run| {
                let name = run.name.as_deref().unwrap_or(&cell_font.name);
                let size = run.size.unwrap_or(cell_font.size);
                match cell_font_px(name, size) {
                    Some(p) => p as f32 + script_extra_px(size, run.subscript),
                    None => fallback(run.subscript),
                }
            })
            .fold(0.0f32, f32::max)
    }

    /// The largest default row height among row `r`'s cell fonts, with no lower bound: `None` when
    /// no cell counts, or when a counted cell folds text (wrapped, justified or distributed) or has
    /// a font whose default row height is not known (the row then keeps its stored height).
    ///
    /// When a cell in the row counts, the empty cells of the row count too, through the format they
    /// show: the row's own format when it has one (ROW fGhostDirty + ixfe, [MS-XLS] 2.4.221), else the
    /// column format (COLINFO ixfe, [MS-XLS] 2.4.53) of each formatted column with no cell in the row.
    /// An unknown font there also keeps the stored height; wrap on a format is ignored (no text).
    fn fitted_px_no_floor(&self, r: u32) -> Option<f32> {
        let sh = self.sh;
        let mut need: Option<f32> = None;
        let mut cell_cols: Vec<u32> = Vec::new();
        for (c, cell) in sh.cells.row(r, 0, gridcraft_core::MAX_COLS - 1) {
            cell_cols.push(c);
            if self.skipped(r, c) {
                continue;
            }
            let st = self.wb.styles.get(cell.style);
            if st.align.folds() {
                return None;
            }
            let px = cell_font_px(&st.font.name, st.font.size)? as f32;
            need = Some(need.map_or(px, |n| n.max(px)));
        }
        // A row where no cell counts (none at all, or only cells inside merges) keeps its stored height.
        let cells_need = need?;
        let font_px = |id: gridcraft_model::StyleId| {
            let font = &self.wb.styles.get(id).font;
            cell_font_px(&font.name, font.size).map(|p| p as f32)
        };
        if let Some(id) = sh.rows.get(&r).and_then(|i| i.style) {
            // The row's own format, not the column formats, is what its empty cells show.
            let has_empty = cell_cols.len() < gridcraft_core::MAX_COLS as usize;
            return Some(if has_empty { cells_need.max(font_px(id)?) } else { cells_need });
        }
        cell_cols.sort_unstable();
        let mut need = cells_need;
        for (&c, info) in &sh.cols {
            let Some(id) = info.style else { continue };
            if cell_cols.binary_search(&c).is_ok() || self.skipped(r, c) {
                continue;
            }
            need = need.max(font_px(id)?);
        }
        Some(need)
    }

    /// Like the screen autofit: a cell in a hidden or zero-width column, or inside a merged range of
    /// more than one cell, never fits its row.
    fn skipped(&self, r: u32, c: u32) -> bool {
        let sh = self.sh;
        sh.cols.get(&c).is_some_and(|i| i.hidden || i.size.is_some_and(|w| w <= 0.0))
            || sh.merges.iter().any(|m| !m.is_single() && m.contains(gridcraft_core::CellRef::new(r, c)))
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
    fn arial_substitutions_match_column_names_and_keep_other_metrics() {
        for name in [
            "Liberation Sans",
            "Sans",
            "sans-serif",
            "Sans Serif",
            "Geneva",
            "Clear Sans",
            "Droid Sans Fallback",
            "Helvetica",
            "Helvetica Neue",
            "HelveticaNeue LightExt",
        ] {
            for stored in [name.to_string(), format!("  '{}'  ", name.to_uppercase()), format!("\"{name}\"")] {
                for &(_, size, p, d) in FONTS.iter().filter(|(font, ..)| *font == "Arial") {
                    assert_eq!(font_pair(&stored, size), (p, d), "{stored:?} {size}");
                    assert_eq!(cell_font_px(&stored, size), Some(p), "{stored:?} {size}");
                }
                assert_eq!(cell_font_px(&stored, 14.0), Some(24), "{stored:?}");
                assert_eq!(font_pair(&stored, 7.0), FALLBACK, "{stored:?}");
                assert_eq!(cell_font_px(&stored, 7.0), None, "{stored:?}");
            }
        }
        for &(font, size, p, d) in FONTS {
            assert_eq!(font_pair(font, size), (p, d), "{font} {size}");
            assert_eq!(cell_font_px(font, size), Some(p), "{font} {size}");
        }
        for name in ["No Such Font", "Foo Sans", "Sansation", "'Sans\"", "''Sans''", "' Sans '", "\"Arial\""] {
            assert_eq!(font_pair(name, 8.0), FALLBACK, "{name:?}");
            assert_eq!(cell_font_px(name, 8.0), None, "{name:?}");
        }
    }

    #[test]
    fn smaller_default_layout_matches_custom_control_without_changing_imported_metrics() {
        use gridcraft_core::{CellRef, RangeRef, Value};
        use gridcraft_model::{Cell, Style, StyleTable};

        let mut source = Workbook::new();
        let mut normal = Style::default();
        normal.font.name = "Arial".into();
        normal.font.size = 10.0;
        source.styles = StyleTable::with_default(normal);
        let sh = source.sheet_mut(0).unwrap();
        sh.default_row_height = 12.0;
        sh.default_row_custom_height = Some(false);
        sh.print.set_print_areas(vec![RangeRef::new(CellRef::new(0, 0), CellRef::new(149, 0))]);
        for r in [0, 149] {
            sh.set_cell(CellRef::new(r, 0), Cell { value: Value::Number(1.0), ..Cell::default() });
        }
        let read = |wb: &Workbook| gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(wb).unwrap()).unwrap().0;
        let automatic = read(&source);
        source.sheet_mut(0).unwrap().default_row_custom_height = Some(true);
        let control = read(&source);
        let before = serde_json::to_value(&automatic).unwrap();
        let options = super::super::PrintOptions::default();
        let actual = super::super::layout_sheet(&automatic, 0, &options, None).unwrap();
        let expected = super::super::layout_sheet(&control, 0, &options, None).unwrap();
        assert_eq!(actual.row_bands, expected.row_bands);
        assert_eq!(actual.pages, expected.pages);
        assert_eq!(actual.scale, expected.scale);
        assert_eq!(automatic.sheet(0).unwrap().default_row_height, 12.0);
        assert_eq!(automatic.sheet(0).unwrap().default_row_custom_height, Some(false));
        assert_eq!(serde_json::to_value(&automatic).unwrap(), before);
    }

    #[test]
    fn automatic_sheet_baseline_is_positive_finite_and_at_most_normal() {
        let mut wb = Workbook::new();
        let sh = wb.sheet_mut(0).unwrap();
        sh.fit_auto_row_heights_in_print = true;
        sh.default_row_custom_height = Some(false);
        for (saved, expected) in [(12.0, 12.0), (13.5, 13.5), (40.0, 20.0), (0.0, 20.0), (-1.0, 20.0), (f32::NAN, 20.0), (f32::INFINITY, 20.0)] {
            wb.sheet_mut(0).unwrap().default_row_height = saved;
            assert_eq!(RowPitch::new(&wb, wb.sheet(0).unwrap(), 1.0).row_px(0), expected);
        }
    }

    #[test]
    fn smaller_baseline_keeps_stored_fallback_for_wrapped_and_unknown_fonts() {
        use gridcraft_core::{CellRef, Value};
        use gridcraft_model::{Cell, LineInfo, Style};

        let mut wb = Workbook::new();
        let mut wrapped = Style::default();
        wrapped.align.wrap = true;
        let wrapped = wb.styles.intern(wrapped);
        let mut unknown = Style::default();
        unknown.font.name = "Unmeasured font".into();
        unknown.font.size = 30.0;
        let unknown = wb.styles.intern(unknown);
        let sh = wb.sheet_mut(0).unwrap();
        sh.fit_auto_row_heights_in_print = true;
        sh.default_row_custom_height = Some(false);
        sh.default_row_height = 12.0;
        for (row, style, value) in [(0, wrapped, Value::Empty), (1, unknown, Value::Number(1.0)), (3, wrapped, Value::Number(1.0))] {
            sh.set_cell(CellRef::new(row, 0), Cell { style, value, ..Cell::default() });
            sh.rows.insert(row, LineInfo { size: Some(32.0), ..Default::default() });
        }
        let pitch = RowPitch::new(&wb, wb.sheet(0).unwrap(), 1.0);
        assert_eq!(pitch.row_px(0), 32.0);
        assert_eq!(pitch.row_px(1), 32.0);
        assert_eq!(pitch.row_px(2), 12.0);
        // Wrapped text with a value fits its one line of the cell font (Calibri 11: 20 px).
        assert_eq!(pitch.row_px(3), 20.0);
    }

    #[test]
    fn xls_empty_row_retains_stored_height_without_a_normal_floor() {
        let mut wb = Workbook::new();
        let sh = wb.sheet_mut(0).unwrap();
        sh.fit_auto_row_heights_in_print = true;
        sh.fit_print_rows_without_default_floor = true;
        sh.default_row_custom_height = Some(false);
        sh.default_row_height = 40.0;
        sh.rows.insert(1, gridcraft_model::LineInfo { size: Some(12.0), ..Default::default() });
        let pitch = RowPitch::new(&wb, wb.sheet(0).unwrap(), 1.0);
        assert_eq!(pitch.row_px(0), 40.0);
        assert_eq!(pitch.row_px(1), 12.0);
    }

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
