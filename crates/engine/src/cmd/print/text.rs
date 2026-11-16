use super::draw::{Block, Look, rgb_of};
use super::{CellRef, Color, Font, HAlign, MAX_COLS, Page, Point, RangeRef, Sheet, Style, StyleTable, Underline, VAlign, Value};

const PAD_PX: f32 = 2.0;

/// Most lines [`wrap_lines`] returns (it may return one more, the rest of a paragraph).
const MAX_WRAP_LINES: usize = 2000;

/// Word-wraps `text` to `width` points.
///
/// Widths are kept as running sums of glyph widths (the sum `text_width` makes), so each
/// character is measured once: re-measuring the line for every word, or the rest of an
/// over-long word for every piece cut from it, made a long paragraph or word quadratic.
pub(super) fn wrap_lines(text: &str, font: Font, size: f32, width: f32) -> Vec<String> {
    let units_of = |s: &str| -> u64 { s.chars().map(|c| u64::from(font.char_width(c))).sum() };
    let fits = |units: u64| units as f32 * size / 1000.0 <= width;
    let space = units_of(" ");
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        let mut line_units: u64 = 0;
        for word in para.split(' ') {
            let word_units = units_of(word);
            if line.is_empty() {
                line.push_str(word);
                line_units = word_units;
            } else if fits(line_units + space + word_units) {
                line.push(' ');
                line.push_str(word);
                line_units += space + word_units;
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
                line_units = word_units;
            }
            // Break a single over-long word by characters: each piece is the longest that fits
            // (at least one character).
            if !fits(line_units) && line.chars().nth(1).is_some() {
                let mut piece = String::new();
                let mut units: u64 = 0;
                for c in line.chars() {
                    let w = u64::from(font.char_width(c));
                    if !piece.is_empty() && !fits(units + w) {
                        out.push(std::mem::take(&mut piece));
                        units = 0;
                        if out.len() > MAX_WRAP_LINES {
                            return out;
                        }
                    }
                    piece.push(c);
                    units += w;
                }
                line = piece;
                line_units = units;
            }
            if out.len() > MAX_WRAP_LINES {
                return out;
            }
        }
        out.push(line);
    }
    out
}

/// Advance widths (1/2048 em) of Calibri regular's printable ASCII characters (U+0020..=U+007E):
/// published font metrics, measured facts with no font file involved. Excel wraps Calibri text at
/// these widths, which are narrower than the printed stand-in's, capitals and digits most of all.
const CALIBRI_ASCII: [u16; 95] = [
    463, 548, 821, 1038, 1038, 1464, 1397, 452, 621, 621, 1038, 1038, 511, 627, 517, 792, // ' '../
    1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 548, 548, 1038, 1038, 1038, 949, // 0..?
    1757, 1185, 1114, 1092, 1260, 1000, 941, 1292, 1276, 516, 653, 1064, 861, 1751, 1322, 1356, // @..O
    1058, 1378, 1112, 941, 998, 1314, 1162, 1822, 1063, 998, 959, 627, 792, 627, 1038, 1038, // P.._
    585, 981, 1076, 866, 1076, 1019, 625, 964, 1076, 470, 490, 931, 470, 1636, 1076, 1080, // `..o
    1076, 1076, 714, 801, 686, 1076, 925, 1464, 887, 927, 809, 638, 948, 638, 1038, // p..~
];

/// Pixels (at 100 %) of a cell's width that wrapped text with a measured width table does not use:
/// fitted to Excel's wrapped line counts of Calibri 11 text in columns of 49 to 208 px.
const MEASURED_WRAP_MARGIN_PX: f32 = 7.25;

/// How a cell's folded text is measured for wrapping: the printed stand-in's advance widths, or a
/// measured table of the cell font's own widths at the whole-pixel size a 96 dpi screen renders it.
#[derive(Clone, Copy)]
pub(super) struct CellWrap {
    font: Font,
    /// Printed size in points.
    size: f32,
    /// The cell font's own ASCII advances (1/2048 em) and its em in printed points.
    table: Option<(&'static [u16; 95], f32)>,
}

impl CellWrap {
    /// The measure for text in style `st` printed at `scale`.
    pub(super) fn new(st: &Style, scale: f32) -> Self {
        let size = st.font.size.clamp(1.0, 409.0);
        let regular_calibri = st.font.name.trim().eq_ignore_ascii_case("calibri") && !st.font.bold && !st.font.italic;
        let table = regular_calibri.then(|| (&CALIBRI_ASCII, (size * 4.0 / 3.0).round().max(1.0) * 0.75 * scale));
        Self { font: font_for(st), size: size * scale, table }
    }

    /// Width of `c` in printed points.
    fn width(&self, c: char) -> f32 {
        if let Some((table, em)) = self.table {
            let i = (c as usize).wrapping_sub(0x20);
            if let Some(&u) = table.get(i) {
                return f32::from(u) * em / 2048.0;
            }
            return f32::from(self.font.char_width(c)) * em / 1000.0;
        }
        f32::from(self.font.char_width(c)) * self.size / 1000.0
    }

    /// Unscaled pixels of the cell width the text does not wrap into, both sides together.
    pub(super) fn margin_px(&self) -> f32 {
        if self.table.is_some() { MEASURED_WRAP_MARGIN_PX } else { 2.0 * PAD_PX }
    }
}

/// Wraps a cell's folded text to `width` points the way Excel breaks cell text: a line breaks
/// after a run of spaces (which hang past the edge and are not printed at its end) or after a
/// hyphen; a word longer than the line breaks between characters (each piece at least one
/// character); a line feed always breaks. Each character is measured once.
pub(super) fn wrap_cell_lines(text: &str, m: &CellWrap, width: f32) -> Vec<String> {
    let width = f64::from(width);
    let mut out = Vec::new();
    for para in text.split('\n') {
        let chars: Vec<char> = para.chars().collect();
        // prefix[i] = width of chars[..i].
        let mut prefix = Vec::with_capacity(chars.len() + 1);
        let mut sum = 0.0f64;
        prefix.push(sum);
        for &c in &chars {
            sum += f64::from(m.width(c));
            prefix.push(sum);
        }
        let span = |a: usize, b: usize| prefix.get(b).copied().unwrap_or(sum) - prefix.get(a).copied().unwrap_or(sum);
        let piece = |a: usize, b: usize, trim: bool| -> String {
            let s: String = chars.get(a..b).unwrap_or_default().iter().collect();
            if trim { s.trim_end_matches(' ').to_string() } else { s }
        };
        let mut start = 0usize;
        // The last break opportunity on the line: the next line starts after it.
        let mut brk: Option<usize> = None;
        for (i, &c) in chars.iter().enumerate() {
            if c == ' ' {
                brk = Some(i + 1);
                continue;
            }
            if i > start && span(start, i + 1) > width {
                if let Some(b) = brk.filter(|&b| b > start) {
                    out.push(piece(start, b, true));
                    start = b;
                }
                if i > start && span(start, i + 1) > width {
                    out.push(piece(start, i, true));
                    start = i;
                }
                brk = None;
                if out.len() > MAX_WRAP_LINES {
                    return out;
                }
            }
            if c == '-' {
                brk = Some(i + 1);
            }
        }
        out.push(piece(start, chars.len(), false));
        if out.len() > MAX_WRAP_LINES {
            return out;
        }
    }
    out
}

/// Lines the folded (wrapped, justified or distributed) text of cell `c` with style `st` takes
/// when printed at 100 % in its own column: its displayed text wrapped by [`wrap_cell_lines`] to
/// the width [`draw_text`] gives it (explicit line breaks always break). `Some(0)` when the cell
/// prints no value; `None` for rotated or stacked text, whose height the row fit does not model.
pub(super) fn folded_line_count(wb: &gridcraft_model::Workbook, sh: &Sheet, c: CellRef, st: &Style) -> Option<usize> {
    let v = crate::display::displayed_value(sh.value(c));
    if v.is_empty() || (!sh.show_zeros && v.as_f64() == Some(0.0)) {
        return Some(0);
    }
    if st.align.rotation != 0 {
        return None;
    }
    // A number in a format prints on one line (General shortens it to fit, others show #).
    if matches!(v, Value::Number(_)) && !sh.show_formulas {
        return Some(1);
    }
    let mut text = if sh.show_formulas { crate::display::cell_text(wb, sh, c) } else { crate::display::value_text_as(wb, &v, st.num_fmt.as_str()) };
    crate::display::truncate_displayed(&mut text);
    if text.is_empty() {
        return Some(0);
    }
    let k = super::PX_TO_PT;
    let m = CellWrap::new(st, 1.0);
    let size = st.font.size.clamp(1.0, 409.0);
    let indent = f32::from(st.align.indent) * indent_level_pt(wb);
    let avail = (sh.col_width(c.col) * k - m.margin_px() * k - indent).max(0.0);
    Some(wrap_cell_lines(&text, &m, avail.max(size)).len())
}

fn font_for(style: &Style) -> Font {
    Font::styled(Font::family_for(&style.font.name), style.font.bold, style.font.italic)
}

/// Space advance (em) of common Normal style fonts whose printed stand-in has a different space.
const SPACE_EM: [(&str, f32); 3] = [("calibri", 0.2261), ("arial", 0.2778), ("times new roman", 0.25)];

/// Width in points of one indent level: 3 spaces of the workbook's Normal style font at its size
/// (ECMA-376 Part 1, 18.8.1 `alignment/@indent`).
fn indent_level_pt(wb: &gridcraft_model::Workbook) -> f32 {
    let normal = wb.styles.get(gridcraft_model::StyleId::DEFAULT);
    let size = normal.font.size.clamp(1.0, 409.0);
    let name = normal.font.name.trim();
    let em = SPACE_EM
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|&(_, em)| em)
        .unwrap_or_else(|| gridcraft_pdf::text_width(font_for(normal), size, " ") / size);
    3.0 * em * size
}

pub(super) fn text_align(align: HAlign, value: &Value) -> HAlign {
    match align {
        HAlign::General => match value {
            Value::Number(_) => HAlign::Right,
            Value::Bool(_) | Value::Error(_) => HAlign::Center,
            _ => HAlign::Left,
        },
        HAlign::CenterAcross | HAlign::Distributed => HAlign::Center,
        HAlign::Justify | HAlign::Fill => HAlign::Left,
        other => other,
    }
}

/// Is a cell free for text to overflow into?
fn empty_for_overflow(sh: &Sheet, c: CellRef, merges: &[RangeRef]) -> bool {
    sh.value_ref(c).is_none_or(|v| v.is_empty()) && !merges.iter().any(|m| m.contains(c))
}

fn extend_text_bounds(sh: &Sheet, c: CellRef, merges: &[RangeRef], k: f32, need: f32, align: HAlign, bounds: (f32, f32)) -> (f32, f32) {
    let (mut clip_l, mut clip_r) = bounds;
    let grow = |dir: i64, mut remaining: f32| -> f32 {
        let mut ext = 0.0;
        let mut col = c.col as i64 + dir;
        while remaining > 0.0 && col >= 0 && col < MAX_COLS as i64 {
            let nc = CellRef::new(c.row, col as u32);
            if !empty_for_overflow(sh, nc, merges) {
                break;
            }
            let width = sh.col_width(nc.col) * k;
            ext += width;
            remaining -= width;
            col += dir;
            if ext > 20_000.0 {
                break;
            }
        }
        ext
    };
    match align {
        HAlign::Right => clip_l -= grow(-1, need),
        HAlign::Center => {
            clip_l -= grow(-1, need / 2.0);
            clip_r += grow(1, need / 2.0);
        }
        _ => clip_r += grow(1, need),
    }
    (clip_l, clip_r)
}

/// The last column the text of `c` runs into to its right when printed at page scale `scale`
/// (`k` points per sheet pixel), by the same rule [`draw_text`] clips it with: single-line text
/// (not a number) wider than its cell spills over the empty cells to its right, up to the first
/// non-empty one. Wrapped, justified, distributed, shrunk-to-fit, filled, right-aligned, upright
/// and merged cells, and Center Across Selection runs, stay in place (`None`), and so does centred
/// text whose left neighbour (same row) is not empty: Excel's printed range gains no column for it.
/// The text is measured only when the right neighbour is empty, so a sheet of short or packed text
/// costs one lookup per cell.
/// non-empty one. Wrapped, shrunk-to-fit, filled, right-aligned, upright and merged cells, and
/// Center Across Selection runs, stay in place (`None`), and so does centred text whose left
/// neighbour (same row) is not empty: Excel's printed range gains no column for it. The text is
/// measured only when the right neighbour is empty, so a sheet of short or packed text costs one lookup per cell.
/// Greek and Cyrillic use the Latin-width approximation in [`spilling_text`] for pagination.
/// `fonts` remembers the printed font of the style last measured (cells in a run share styles).
pub(super) fn right_overflow_end(
    wb: &gridcraft_model::Workbook,
    sh: &Sheet,
    (c, cell): (CellRef, &gridcraft_model::Cell),
    merges: &[RangeRef],
    (k, scale): (f32, f32),
    fonts: &mut Option<(gridcraft_model::StyleId, Font)>,
) -> Option<u32> {
    let right = spill_candidate(sh, (c, cell))?;
    let (need, h_align) = spilling_text(wb, sh, (c, cell), merges, (k, scale), fonts)?;
    if h_align == HAlign::Center && c.col > 0 && !empty_for_overflow(sh, CellRef::new(c.row, c.col - 1), merges) {
        return None;
    }
    // Columns print wider than they draw on screen (Excel's printed column width, see
    // `layout::print_col_scale`) while text prints at its font size, so the reach is measured
    // against the printed column widths.
    let cs = super::layout::print_col_scale(wb);
    let w = sh.col_width(c.col) * k * cs;
    let over = need - w;
    if over <= 0.0 || merges.iter().any(|m| m.contains(c) || m.contains(right)) {
        return None;
    }
    let mut remaining = if h_align == HAlign::Center { over / 2.0 } else { over };
    // Text running past the cell edge by only a few points does not widen the printed range.
    const MIN_PRINTED_SPILL_PT: f32 = 6.0;
    if remaining < MIN_PRINTED_SPILL_PT {
        return None;
    }
    let (mut col, mut ext, mut last) = (c.col + 1, 0.0f32, None);
    while remaining > 0.0 && col < MAX_COLS && ext <= 20_000.0 {
        let nc = CellRef::new(c.row, col);
        if !empty_for_overflow(sh, nc, merges) {
            break;
        }
        let width = sh.col_width(col) * k * cs;
        ext += width;
        remaining -= width;
        last = Some(col);
        col += 1;
    }
    last
}

/// How far (points from the cell's left edge, at page scale `k`) the text of `c` reaches when
/// its column is printed cut to `cut` points (a column wider than the page): left-aligned,
/// single-line, unrotated text wider than the cut runs on over the empty cells to its right, up
/// to the first non-empty one. Centred text is laid out centred on the cut (not on the whole
/// column), so it reaches `cut / 2 + need / 2`, limited the same way. `None` when the text stays
/// within the cut or cannot run on.
pub(super) fn cut_text_reach(
    wb: &gridcraft_model::Workbook,
    sh: &Sheet,
    (c, cell): (CellRef, &gridcraft_model::Cell),
    merges: &[RangeRef],
    (k, scale): (f32, f32),
    cut: f32,
    fonts: &mut Option<(gridcraft_model::StyleId, Font)>,
) -> Option<f32> {
    let right = spill_candidate(sh, (c, cell))?;
    if wb.styles.get(cell.style).align.rotation != 0 || merges.iter().any(|m| m.contains(c) || m.contains(right)) {
        return None;
    }
    let (need, h_align) = spilling_text(wb, sh, (c, cell), merges, (k, scale), fonts)?;
    let need = match h_align {
        HAlign::Left => need,
        HAlign::Center => cut / 2.0 + need / 2.0,
        _ => return None,
    };
    if need.is_nan() || need <= cut + 0.01 {
        return None;
    }
    let (mut reach, mut col) = (cut, c.col + 1);
    while reach < need && col < MAX_COLS && reach - cut <= 20_000.0 {
        if !empty_for_overflow(sh, CellRef::new(c.row, col), merges) {
            break;
        }
        reach += sh.col_width(col) * k;
        col += 1;
    }
    Some(need.min(reach)).filter(|r| *r > cut + 0.01)
}

/// The cell right of `c` when `c` holds non-numeric text that could run over it: that cell is
/// empty and inside the grid.
fn spill_candidate(sh: &Sheet, (c, cell): (CellRef, &gridcraft_model::Cell)) -> Option<CellRef> {
    let value = &cell.value;
    if value.is_empty() || matches!(value, Value::Number(_)) || c.col + 1 >= MAX_COLS {
        return None;
    }
    let right = CellRef::new(c.row, c.col + 1);
    if sh.value_ref(right).is_some_and(|v| !v.is_empty()) {
        return None;
    }
    Some(right)
}

/// The printed width (points at page scale `k`, padding and indent included) and alignment of
/// the single-line text of `c`, when that text may spill past its cell: not folded (wrapped,
/// justified or distributed, [`gridcraft_model::Alignment::folds`]), shrunk, filled, upright,
/// right-aligned or a Center Across Selection run.
/// the single-line text of `c`, when that text may spill past its cell: not wrapped, shrunk,
/// filled, upright, right-aligned or a Center Across Selection run.
/// For pagination, Greek (U+0370–U+03FF) and Cyrillic (U+0400–U+052F) use the stand-in's
/// lowercase `o` advance, scaled like ASCII by [`latin_width_ratio`]. Other characters keep
/// their existing advances, including full-width CJK; drawing metrics are unchanged.
fn spilling_text(
    wb: &gridcraft_model::Workbook,
    sh: &Sheet,
    (c, cell): (CellRef, &gridcraft_model::Cell),
    merges: &[RangeRef],
    (k, scale): (f32, f32),
    fonts: &mut Option<(gridcraft_model::StyleId, Font)>,
) -> Option<(f32, HAlign)> {
    let value = &cell.value;
    let sid = cell.style;
    let st = wb.styles.get(sid);
    let a = &st.align;
    if a.folds() || a.shrink || a.h == HAlign::Fill || a.rotation.abs() == 90 {
        return None;
    }
    let h_align = text_align(a.h, value);
    if h_align == HAlign::Right {
        return None;
    }
    if a.h == HAlign::CenterAcross && center_across_width(&wb.styles, sh, c, merges) > 0.0 {
        return None;
    }
    // Plain text in a General cell is measured where it lies; anything else is formatted first.
    let code = st.num_fmt.as_str();
    let formatted;
    let text: &str = match value {
        Value::Text(t) if !sh.show_formulas && (code == "General" || code == "@") => crate::display::displayed_prefix(t),
        _ => {
            let v = crate::display::displayed_value(value.clone());
            let mut t = if sh.show_formulas { crate::display::cell_text(wb, sh, c) } else { crate::display::value_text_as(wb, &v, code) };
            crate::display::truncate_displayed(&mut t);
            formatted = t;
            &formatted
        }
    };
    if text.is_empty() {
        return None;
    }
    let size = st.font.size.clamp(1.0, 409.0) * scale;
    let font = match *fonts {
        Some((id, f)) if id == sid => f,
        _ => {
            let f = font_for(st);
            *fonts = Some((sid, f));
            f
        }
    };
    // Line breaks print as spaces (as in `draw_text`).
    let width_of = |ch: char| u64::from(font.char_width(if ch == '\n' || ch == '\r' { ' ' } else { ch }));
    let (latin, other) = text.chars().fold((0u64, 0u64), |(latin, other), ch| {
        if matches!(ch, '\u{0370}'..='\u{03ff}' | '\u{0400}'..='\u{052f}') {
            (latin + u64::from(font.char_width('o')), other)
        } else if ch.is_ascii() {
            (latin + width_of(ch), other)
        } else {
            (latin, other + width_of(ch))
        }
    });
    let tw = (latin as f32 * latin_width_ratio(&st.font.name, font) + other as f32) * size / 1000.0;
    let indent = if a.indent == 0 { 0.0 } else { f32::from(a.indent) * indent_level_pt(wb) };
    Some((tw + 2.0 * PAD_PX * k + indent, h_align))
}

/// Width of `text` at `size` with its ASCII characters scaled by `ratio` ([`latin_width_ratio`]).
fn scaled_width(font: Font, size: f32, text: &str, ratio: f32) -> f32 {
    let (latin, other) = text.chars().fold((0u64, 0u64), |(latin, other), ch| {
        let cw = u64::from(font.char_width(ch));
        if ch.is_ascii() { (latin + cw, other) } else { (latin, other + cw) }
    });
    (latin as f32 * ratio + other as f32) * size / 1000.0
}

/// How much wider (or narrower) the named font's Latin text prints than its stand-in's metrics
/// give: the ratio of their digit advances. Excel prints Calibri text at Calibri's own widths,
/// about 9% narrower than the sans-serif stand-in's, and the cells it runs over are sized by
/// those widths too. 1 for a font whose digit advance is not known.
fn latin_width_ratio(name: &str, font: Font) -> f32 {
    let stand_in = f32::from(font.char_width('0')) / 1000.0;
    match super::layout::digit_em(name) {
        Some(em) if stand_in > 0.0 => (em / stand_in).clamp(0.5, 2.0),
        _ => 1.0,
    }
}

/// Unscaled width of the empty Center Across Selection cells right of `c`, which its text is centred over too.
pub(super) fn center_across_width(styles: &StyleTable, sh: &Sheet, c: CellRef, merges: &[RangeRef]) -> f32 {
    let mut ext = 0.0;
    let mut col = c.col + 1;
    while col < MAX_COLS && ext < 20_000.0 {
        let nc = CellRef::new(c.row, col);
        if !empty_for_overflow(sh, nc, merges) || styles.get(sh.style_id(nc)).align.h != HAlign::CenterAcross {
            break;
        }
        ext += sh.col_width(col);
        col += 1;
    }
    ext
}

pub(super) struct TextPlacement<'a> {
    pub cell: CellRef,
    pub look: &'a Look,
    pub rect: (f32, f32, f32, f32),
    pub merged: bool,
    pub merges: &'a [RangeRef],
    pub limits: (f32, f32),
}

pub(super) fn draw_text(pg: &mut Page, b: &Block, placement: TextPlacement<'_>) {
    let TextPlacement { cell: c, look: l, rect, merged, merges, limits } = placement;
    let (x, y, w, h) = rect;
    let sh = b.sh;
    let wb = b.wb;
    // Only the characters the cell displays are laid out and printed.
    let v = crate::display::displayed_value(sh.value(c));
    if v.is_empty() || l.cf.as_ref().is_some_and(|cf| cf.hide_value) {
        return;
    }
    if !sh.show_zeros && v.as_f64() == Some(0.0) {
        return;
    }
    let st = &l.style;
    let mut size = st.font.size.clamp(1.0, 409.0) * b.scale;
    let font = font_for(st);
    let pad = PAD_PX * b.k;
    // Indent is measured in the Normal style font and is not shrunk by page scaling.
    let indent = f32::from(st.align.indent) * indent_level_pt(wb);
    let avail = (w - 2.0 * pad - indent).max(0.0);
    let code = st.num_fmt.as_str();
    let formatted = crate::display::format(&v, code, wb);
    let mut color = formatted.color.and_then(gridcraft_numfmt::FormatColor::rgb).or_else(|| rgb_of(st.font.color, wb)).unwrap_or([0, 0, 0]);
    if l.cf.as_ref().and_then(|x| x.style.as_ref()).is_some_and(|s| s.font.color != Color::Auto) {
        color = rgb_of(st.font.color, wb).unwrap_or(color);
    }
    let is_num = matches!(v, Value::Number(_));
    // Numbers are measured at the cell font's own Latin widths, as the print extent is, and may
    // use half the cell padding: Excel prints a number in full whenever its digits fit the column.
    let ratio = latin_width_ratio(&st.font.name, font);
    let num_room = (w - indent - pad).max(0.0);
    // Justified and distributed text folds as wrapped text does.
    let wrap = st.align.folds();
    let mut text = if sh.show_formulas {
        crate::display::cell_text(wb, sh, c)
    } else if code == "General"
        && let Value::Number(n) = v
    {
        // Digits counted at the cell font's own width, within the room a number may use.
        let digit = (gridcraft_pdf::text_width(font, size, "0") * ratio).max(0.1);
        let max_chars = (num_room / digit).floor().max(1.0) as usize;
        gridcraft_numfmt::format_general_fit(n, max_chars.min(11)).unwrap_or_else(|| "#".repeat(max_chars.min(255)))
    } else {
        // The look's code, which a conditional format can override (#1163).
        crate::display::value_text_as(wb, &v, code)
    };
    crate::display::truncate_displayed(&mut text);
    if !wrap {
        text = text.replace(['\n', '\r'], " ");
    }
    if text.is_empty() {
        return;
    }
    let mut tw = gridcraft_pdf::text_width(font, size, &text);
    if st.align.shrink && !wrap && tw > avail && tw > 0.0 {
        size = (size * avail / tw).max(1.0);
        tw = gridcraft_pdf::text_width(font, size, &text);
    }
    if is_num && !wrap && tw > avail + 0.01 && !sh.show_formulas && scaled_width(font, size, &text, ratio) > num_room + 0.01 {
        let hash = gridcraft_pdf::text_width(font, size, "#").max(0.1);
        text = "#".repeat(((avail / hash).floor().max(1.0) as usize).min(255));
        tw = gridcraft_pdf::text_width(font, size, &text);
    }
    // A number that fits at its font's widths but whose stand-in glyphs are wider than the
    // padded room stays inside its cell: centred over the padding, else drawn smaller.
    let mut fit_x = None;
    if is_num && !wrap && tw > avail + 0.01 && tw > 0.0 {
        let room = (w - indent).max(0.0);
        if tw > room {
            size = (size * room / tw).max(0.1);
            fit_x = Some(x + indent);
        } else {
            fit_x = Some(x + indent + (room - tw) / 2.0);
        }
    }
    let h_align = text_align(st.align.h, &v);
    let rot = st.align.rotation;
    let rotated = rot != 0 && rot != 255;
    // Rotated text wraps along its own direction: upright (±90°) text to the row height.
    let lines: Vec<String> = match (wrap, rotated) {
        (true, false) => {
            let m = CellWrap::new(st, b.scale);
            let wrap_avail = (w - m.margin_px() * b.k - indent).max(0.0);
            wrap_cell_lines(&text, &m, wrap_avail.max(size))
        }
        (true, true) if rot.abs() == 90 => wrap_cell_lines(&text, &CellWrap::new(st, b.scale), (h - 2.0 * pad).max(size)),
        (true, true) => vec![text.lines().next().unwrap_or_default().to_string()],
        (false, _) => vec![text],
    };
    let line_h = size * 1.2;
    let n = lines.len().max(1) as f32;
    let first_baseline = match st.align.v {
        VAlign::Top => y + pad + size * 0.8,
        VAlign::Center | VAlign::Justify | VAlign::Distributed => y + (h - n * line_h) / 2.0 + line_h * 0.5 + size * 0.33,
        VAlign::Bottom => y + h - pad - size * 0.22 - (n - 1.0) * line_h,
    };
    // Overflow into empty neighbours (single-line text, not merged).
    // Center Across Selection spans the empty Center Across cells to its right; the text stays within that run.
    let spans = st.align.h == HAlign::CenterAcross && !wrap && !merged && rot == 0;
    let across = if spans { center_across_width(&wb.styles, sh, c, merges) * b.k } else { 0.0 };
    let (mut clip_l, mut clip_r) = (x, x + w + across);
    if !wrap && !merged && !is_num && rot.abs() != 90 && across <= 0.0 && tw + 2.0 * pad + indent > w {
        let need = tw + 2.0 * pad + indent - w;
        (clip_l, clip_r) = extend_text_bounds(sh, c, merges, b.k, need, h_align, (clip_l, clip_r));
    }
    let clip_l = clip_l.max(limits.0);
    let clip_r = clip_r.min(limits.1);
    pg.save();
    pg.clip_rect(clip_l, y, (clip_r - clip_l).max(0.0), h);
    if rotated {
        // Rotated text: anchored at the bottom-left (upward) or top-left (downward) of the cell.
        // Wrapped lines stack rightward for upward text and leftward for downward text, so
        // downward text starts its first line far enough in for the last to end at the left pad.
        let deg = (rot as f32).clamp(-90.0, 90.0);
        let angle = -deg.to_radians();
        let last = lines.len().saturating_sub(1) as f32;
        for (i, t) in lines.iter().enumerate() {
            let i = i as f32;
            let (ox, oy) = if deg > 0.0 {
                (x + pad + size * 0.8 * deg.to_radians().sin() + i * line_h, y + h - pad)
            } else {
                (x + pad + (last - i) * line_h, y + pad + size * 0.8)
            };
            pg.text_rotated(Point::new(ox, oy), size, font, color, t, angle);
        }
        pg.restore();
        return;
    }
    if rot == 255 {
        // Vertical stacked text.
        let chars: Vec<char> = lines.concat().chars().take(500).collect();
        let mut yy = y + pad + size * 0.8;
        for ch in chars {
            let s = ch.to_string();
            let cw = gridcraft_pdf::text_width(font, size, &s);
            pg.text(x + (w - cw) / 2.0, yy, size, font, color, &s);
            yy += line_h;
            if yy > y + h + line_h {
                break;
            }
        }
        pg.restore();
        return;
    }
    for (i, line) in lines.iter().enumerate() {
        let lw = gridcraft_pdf::text_width(font, size, line);
        let lx = match (fit_x, h_align) {
            (Some(fx), _) => fx,
            (None, HAlign::Right) => x + w - pad - lw - indent,
            (None, HAlign::Center) => x + (w + across - lw) / 2.0,
            (None, _) => x + pad + indent,
        };
        let ly = first_baseline + i as f32 * line_h;
        if ly - size > y + h || ly < y - line_h {
            continue;
        }
        pg.text(lx, ly, size, font, color, line);
        if st.font.underline != Underline::None {
            let uw = (size * 0.06).max(0.2);
            pg.stroke_line(lx, ly + size * 0.12, lx + lw, ly + size * 0.12, uw, color);
            if matches!(st.font.underline, Underline::Double | Underline::DoubleAccounting) {
                pg.stroke_line(lx, ly + size * 0.24, lx + lw, ly + size * 0.24, uw, color);
            }
        }
        if st.font.strike {
            pg.stroke_line(lx, ly - size * 0.3, lx + lw, ly - size * 0.3, (size * 0.06).max(0.2), color);
        }
    }
    pg.restore();
}
