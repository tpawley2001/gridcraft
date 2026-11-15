use super::header_footer::HF_FIXED_SIDE_MARGIN;
use super::metrics::{DOT_PT, RowPitch, page_dots};
use super::{
    Anchor, CellRef, Json, MAX_COLS, MAX_PAGES, MAX_ROWS, Orientation, PX_TO_PT, PageOrder, PrintSettings, RangeRef, Result, Sheet, Visibility,
    Workbook, bad, bool_param, split_sheet, str_param,
};

/// Cells in a page block above which only stored cells are visited (not every grid position).
pub(super) const DENSE_BLOCK: u64 = 60_000;

// ---------------------------------------------------------------- paper

/// Paper size in points (portrait) for a paper name; unknown names fall back to Letter.
pub fn paper_size(name: &str) -> (f32, f32) {
    known_paper(name).unwrap_or((612.0, 792.0))
}

/// Paper sizes in points, portrait. `B4`/`B5`/`B6` are the **JIS** sizes Japanese Excel and its
/// e-mail attachments use (257×364, 182×257 and 128×182 mm); the ISO B series stays reachable as
/// `B4 (ISO)`/`B5 (ISO)`/`B6 (ISO)`. The Japanese postcards and envelopes are the JIS ones too.
pub(super) fn known_paper(name: &str) -> Option<(f32, f32)> {
    // Spaces, hyphens, underscores and parentheses don't matter: `B5 (JIS)`, `b5-jis` and `b5`
    // are one name.
    let n = name.trim().to_ascii_lowercase().replace([' ', '-', '_', '(', ')'], "");
    Some(match n.as_str() {
        // Letter Small and Note are Letter-sized (ECMA-376 Part 1 §18.3.1.63).
        "letter" | "usletter" | "lettersmall" | "note" => (612.0, 792.0),
        "legal" | "uslegal" => (612.0, 1008.0),
        "a4" | "a4small" => (595.28, 841.89),
        "a2" => (1190.55, 1683.78),
        "folio" => (612.0, 936.0),
        "quarto" => (609.45, 779.53),
        "10x14" => (720.0, 1008.0),
        "csheet" => (1224.0, 1584.0),
        "dsheet" => (1584.0, 2448.0),
        "esheet" => (2448.0, 3168.0),
        "envelopec4" => (649.13, 918.43),
        "envelopec5" => (459.21, 649.13),
        "envelopec6" => (323.15, 459.21),
        "a3" => (841.89, 1190.55),
        "a5" => (419.53, 595.28),
        "a6" => (297.64, 419.53),
        "tabloid" | "11x17" => (792.0, 1224.0),
        "ledger" => (1224.0, 792.0),
        "executive" => (522.0, 756.0),
        "b4" | "b4jis" => (728.50, 1031.81),
        "b5" | "b5jis" => (515.91, 728.50),
        "b6" | "b6jis" => (362.83, 515.91),
        "b4iso" => (708.66, 1000.63),
        "b5iso" => (498.90, 708.66),
        "b6iso" => (354.33, 498.90),
        "japanesepostcard" | "postcard" => (283.46, 419.53),
        "japanesedoublepostcard" | "doublepostcard" => (419.53, 566.93),
        "japaneseenvelope#3" | "chou#3" => (340.16, 666.14),
        // 長形4号 is 90×205 mm; 90×225 mm is 長形40号, a different envelope, so it stays out.
        "japaneseenvelope#4" | "chou#4" => (255.12, 581.10),
        "japaneseenvelopekaku#2" | "kaku#2" => (680.31, 941.10),
        "japaneseenvelopekaku#3" | "kaku#3" => (612.28, 785.20),
        _ => return None,
    })
}

// ---------------------------------------------------------------- layout

/// The pagination of one sheet.
#[derive(Clone, Debug)]
pub struct SheetLayout {
    pub sheet: usize,
    pub settings: PrintSettings,
    pub page_w: f32,
    pub page_h: f32,
    /// Left, right, top, bottom, header, footer margins in points.
    pub margins: [f32; 6],
    pub scale: f32,
    pub area: RangeRef,
    pub row_bands: Vec<(u32, u32)>,
    pub col_bands: Vec<(u32, u32)>,
    /// Per column band, how it prints a column wider than the page ([`ColCut`]): such a column
    /// is cut to the page's width, and text running on past the cut continues on following
    /// pages. Missing entries are not cut.
    pub col_cuts: Vec<ColCut>,
    /// Pages in print order: (row band, column band).
    pub pages: Vec<(usize, usize)>,
    pub title_rows: Option<(u32, u32)>,
    pub title_cols: Option<(u32, u32)>,
    /// Heading sizes in sheet pixels (0 when headings are off).
    pub heading_w: f32,
    pub heading_h: f32,
    /// Manual page breaks apply (they don't when the sheet is fitted to pages).
    pub manual_breaks: bool,
    /// Manual row breaks split pages: no Fit To, or Fit To with an automatic page count tall
    /// (Excel still breaks at them, after scaling to the requested width).
    pub manual_row_breaks: bool,
    /// Manual column breaks split pages: no Fit To, or Fit To with an automatic page count wide.
    pub manual_col_breaks: bool,
    /// The page is a chart sheet's (an index into [`Workbook::chart_sheets`]): one page holding
    /// its chart scaled to the printable area. `sheet` is then the worksheet before it.
    pub chart_sheet: Option<usize>,
    /// The pages are a macro sheet's (an index into [`Workbook::macro_sheets`]), laid out like a
    /// worksheet's. `sheet` is then the worksheet before it; read the printed sheet through
    /// [`SheetLayout::sheet_in`].
    pub macro_sheet: Option<usize>,
}

impl SheetLayout {
    /// The sheet whose cells these pages print: the macro sheet's detached sheet for a macro
    /// sheet's layout, otherwise the workbook's sheet `sheet`.
    pub(crate) fn sheet_in<'a>(&self, wb: &'a Workbook) -> Option<&'a Sheet> {
        match self.macro_sheet {
            Some(mi) => wb.macro_sheets.get(mi).map(|m| &m.sheet),
            None => wb.sheet(self.sheet),
        }
    }
    pub(super) fn k(&self) -> f32 {
        PX_TO_PT * self.scale
    }
    /// How column band `ci` prints a column wider than the page (not cut when it holds none).
    pub(crate) fn col_cut(&self, ci: usize) -> ColCut {
        self.col_cuts.get(ci).copied().unwrap_or_default()
    }
    /// Per column band, the column cut to the page ([`ColCut`]) whose text runs on across that
    /// band's pages, as Excel prints it: (the cut column, how many printable widths the band's
    /// page lies past the cut column's own page, the cut width in sheet pixels). Bands after the
    /// nearest cut column carry it; the cut column's own bands and those before any cut have none.
    pub(crate) fn cut_run_ons(&self) -> Vec<Option<(u32, usize, f32)>> {
        let mut last: Option<(u32, usize, f32)> = None;
        let mut out = Vec::with_capacity(self.col_bands.len());
        for (ci, &(c0, c1)) in self.col_bands.iter().enumerate() {
            let cut = self.col_cut(ci);
            if cut.width > 0.0 {
                if cut.offset <= 0.0 && c0 == c1 {
                    last = Some((c0, ci, cut.width));
                }
                out.push(None);
            } else {
                out.push(last.map(|(col, first, width)| (col, ci.saturating_sub(first), width)));
            }
        }
        out
    }
    pub(crate) fn avail(&self) -> (f32, f32) {
        let [l, r, t, b, ..] = self.margins;
        ((self.page_w - l - r).max(36.0), (self.page_h - t - b).max(36.0))
    }
    /// Left and right margins, in points, that header and footer sections line up with: the
    /// page's side margins with Align with Page Margins on, otherwise 0.7 inch on each side, at
    /// most a quarter of the page width.
    pub(crate) fn hf_sides(&self) -> (f32, f32) {
        if self.settings.hf_align_with_margins {
            (self.margins[0], self.margins[1])
        } else {
            let side = HF_FIXED_SIDE_MARGIN.min(self.page_w / 4.0);
            (side, side)
        }
    }
    /// Scale of header and footer text: the print scale with Scale with Document on, otherwise 1.
    pub(crate) fn hf_text_scale(&self) -> f32 {
        if self.settings.hf_scale_with_doc { self.scale } else { 1.0 }
    }
}

/// Options shared by the print commands.
#[derive(Clone, Debug, Default)]
pub struct PrintOptions {
    pub all_sheets: bool,
    pub range: Option<RangeRef>,
    pub fit_to_page: Option<bool>,
}

impl PrintOptions {
    pub(super) fn from_params(p: &Json, cmd: &str) -> Result<PrintOptions> {
        let all_sheets = match str_param(p, "sheets") {
            None | Some("active") => false,
            Some("all") | Some("workbook") => true,
            Some(other) => return Err(bad(cmd, format!("`sheets` must be active or all, not `{other}`"))),
        };
        let range = match str_param(p, "range").map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) => Some(RangeRef::parse(split_sheet(r).1).ok_or_else(|| bad(cmd, format!("not a range: `{r}`")))?),
            None => None,
        };
        Ok(PrintOptions { all_sheets, range, fit_to_page: bool_param(p, "fitToPage") })
    }
}

/// The range a sheet prints: its print area, else A1 to the end of what prints on the sheet
/// ([`print_ranges`]). `None` when there is nothing to print.
pub fn print_range(wb: &Workbook, sh: &Sheet) -> Option<RangeRef> {
    print_ranges(wb, sh).into_iter().next()
}

/// Every range a sheet prints, in print order: each range of its print area (Excel starts each
/// one on a new page), else A1 to the end of the cells that put something on paper
/// (formatting alone counts only as a fill or a border), merged ranges, spill ranges and visible drawing objects. Empty when there is
/// nothing to print.
pub fn print_ranges(wb: &Workbook, sh: &Sheet) -> Vec<RangeRef> {
    let areas = sh.print.print_areas();
    if !areas.is_empty() {
        return areas.into_iter().map(|a| trim_full(sh, a)).collect();
    }
    used_print_range(wb, sh).into_iter().collect()
}

/// Which stored cells put something on paper, as Excel decides where a sheet's printed range
/// ends: a cell with a value or a formula (even one showing nothing), or an empty cell whose
/// style shows (a fill or a border side). A cell carrying only a font, number format,
/// alignment or protection prints nothing, so columns or rows formatted far past the data add
/// no pages; nor does any cell in a hidden row. Styles are shared, so whether a style shows is
/// worked out once per style id.
struct OnPaper<'a> {
    wb: &'a Workbook,
    sh: &'a Sheet,
    /// Per style id: not yet looked at, or whether it shows on paper.
    styles: Vec<Option<bool>>,
    hidden_row: Option<(u32, bool)>,
    /// Every stored cell counts: nothing on the sheet puts anything on paper by the rule above
    /// (no value or formula, no fill or border, no merge, spill range, table or printing drawing
    /// object). Such a sheet prints its block of stored cells, as Excel does (a sheet of empty
    /// cells formatted only with number formats and fonts prints all of them).
    all: bool,
}

impl<'a> OnPaper<'a> {
    fn new(wb: &'a Workbook, sh: &'a Sheet) -> Self {
        let mut p = OnPaper { wb, sh, styles: vec![None; wb.styles.len()], hidden_row: None, all: false };
        let other = !sh.merges.is_empty() || !sh.spill_ranges.is_empty() || !sh.tables.is_empty() || has_printing_objects(sh);
        // Stops at the first cell that counts, so a sheet with data pays almost nothing here.
        p.all = !other && !sh.cells.iter().any(|(_, cell)| cell.has_content() || p.style_shows(cell));
        p
    }

    /// Whether the style of an empty cell shows on paper, looked up once per style id.
    fn style_shows(&mut self, cell: &gridcraft_model::Cell) -> bool {
        let slot = self.styles.get_mut(cell.style.0 as usize);
        match slot.as_deref().copied().flatten() {
            Some(shows) => shows,
            None => {
                let shows = style_shows_on_paper(self.wb.styles.get(cell.style));
                if let Some(slot) = slot {
                    *slot = Some(shows);
                }
                shows
            }
        }
    }

    /// Whether the stored cell `c` counts as something on a page.
    fn counts(&mut self, c: CellRef, cell: &gridcraft_model::Cell) -> bool {
        if self.all {
            return true;
        }
        let hidden = match self.hidden_row {
            Some((r, h)) if r == c.row => h,
            _ => {
                let h = self.sh.is_row_hidden(c.row);
                self.hidden_row = Some((c.row, h));
                h
            }
        };
        if hidden {
            return false;
        }
        cell.has_content() || self.style_shows(cell)
    }
}

/// Whether the sheet holds a drawing object that prints.
fn has_printing_objects(sh: &Sheet) -> bool {
    sh.charts.iter().any(|o| !o.hidden && !o.no_print)
        || sh.images.iter().any(|o| !o.hidden && !o.no_print)
        || sh.shapes.iter().any(|o| !o.hidden && !o.no_print)
        || sh.controls.iter().any(|o| o.print)
        || sh.embedded_objects.iter().any(|o| o.print)
}

/// The part of a merged range that counts toward the printed range: a merge over whole columns
/// does not reach down (it would end at the grid's last row), nor one over whole rows right.
/// Each still counts in its other direction.
fn merge_extent(m: &RangeRef) -> RangeRef {
    let mut end = m.end;
    if m.is_full_cols() {
        end.row = m.start.row;
    }
    if m.is_full_rows() {
        end.col = m.start.col;
    }
    RangeRef::new(m.start, end)
}

/// A style that marks an empty cell on paper: a fill or any border side.
fn style_shows_on_paper(st: &gridcraft_model::Style) -> bool {
    let b = &st.borders;
    st.fill.pattern != gridcraft_model::PatternType::None || [b.left, b.right, b.top, b.bottom, b.diag_down, b.diag_up].iter().any(|l| !l.is_none())
}

/// Visit missing-cell intervals that inherit row fill ink. Row metadata cannot create a
/// footprint: only the stored cells bound both axes. Subtract explicit cells (including
/// unfilled overrides) and hidden columns without walking the row/column Cartesian product.
fn implicit_row_fill(wb: &Workbook, sh: &Sheet, mut visit: impl FnMut(u32, u32, u32)) {
    let Some(footprint) = sh.cells.used_range() else { return };
    let (r0, r1) = (footprint.start.row, footprint.end.row.min(MAX_ROWS - 1));
    let (c0, c1) = (footprint.start.col, footprint.end.col.min(MAX_COLS - 1));
    if r0 > r1 || c0 > c1 {
        return;
    }
    // At most MAX_COLS columns are examined once, regardless of the number of row styles.
    let mut visible = Vec::new();
    let mut start = c0;
    for col in c0..=c1 {
        if sh.is_col_hidden(col) {
            if start < col {
                visible.push((start, col - 1));
            }
            start = col + 1;
        }
    }
    if start <= c1 {
        visible.push((start, c1));
    }
    for (&row, info) in sh.rows.range(r0..=r1) {
        let Some(style) = info.style else { continue };
        let st = wb.styles.get(style);
        if sh.is_row_hidden(row) || st.fill.pattern == gridcraft_model::PatternType::None || !style_shows_on_paper(st) {
            continue;
        }
        let mut gap = |from: u32, to: u32| {
            let first = visible.partition_point(|&(_, end)| end < from);
            for &(a, b) in visible.iter().skip(first).take_while(|&&(a, _)| a <= to) {
                visit(row, from.max(a), to.min(b));
            }
        };
        // Sheet::style_id gives an explicit cell priority over the row, and the row
        // priority over a column style. Only missing cells therefore inherit this fill.
        let mut from = c0;
        for (col, _) in sh.cells.row(row, c0, c1) {
            if from < col {
                gap(from, col - 1);
            }
            from = col + 1;
        }
        if from <= c1 {
            gap(from, c1);
        }
    }
}

/// A1 to the end of the cells that put something on paper, merged ranges, spill ranges and
/// visible drawing objects; text overflowing past that reaches the columns it spills over, but
/// no further than the last stored column.
fn used_print_range(wb: &Workbook, sh: &Sheet) -> Option<RangeRef> {
    let mut on_paper = OnPaper::new(wb, sh);
    let mut end: Option<CellRef> = None;
    let grow_to = |e: &mut Option<CellRef>, row: u32, col: u32| {
        *e = Some(e.map_or(CellRef::new(row, col), |x| CellRef::new(x.row.max(row), x.col.max(col))));
    };
    // The last stored column: text may spill over formatting-only cells up to it.
    let limit = sh.cells.used_range().map_or(0, |u| u.end.col);
    // Counted cells left of that column holding a value that can spill (numbers never do).
    let mut spill_from = Vec::new();
    for (c, cell) in sh.cells.iter() {
        if on_paper.counts(c, cell) {
            grow_to(&mut end, c.row, c.col);
            if c.col < limit && !cell.value.is_empty() && !matches!(cell.value, gridcraft_core::Value::Number(_)) {
                spill_from.push(c);
            }
        }
    }
    implicit_row_fill(wb, sh, |row, _, end_col| grow_to(&mut end, row, end_col));
    for r in sh.spill_ranges.values() {
        grow_to(&mut end, r.end.row, r.end.col);
    }
    for m in sh.merges.iter().map(merge_extent) {
        grow_to(&mut end, m.end.row, m.end.col);
    }
    // Text spilling over the empty cells right of the last counted column keeps printing, up to
    // the last stored column (as far as the range reached before formatting-only cells stopped
    // counting).
    if let Some(e) = end.filter(|e| e.col < limit) {
        let s = (sh.print.scale.clamp(10, 400) as f32) / 100.0;
        let mut fonts = None;
        let mut reach = e.col;
        for c in spill_from {
            if reach >= limit {
                break;
            }
            let Some(cell) = sh.cells.get(c) else { continue };
            if let Some(x) = super::text::right_overflow_end(wb, sh, (c, cell), &sh.merges, (PX_TO_PT * s, s), &mut fonts) {
                reach = reach.max(x.min(limit));
            }
        }
        grow_to(&mut end, e.row, reach);
    }
    let mut r = end.map(|e| RangeRef::new(CellRef::new(0, 0), e));
    let mut grow = |a: &Anchor| {
        let x = sh.col_left(a.cell.col) + a.dx as f64;
        let y = sh.row_top(a.cell.row) + a.dy as f64;
        let end = CellRef::new(sh.row_at(y + a.height.max(0.0) as f64), sh.col_at(x + a.width.max(0.0) as f64));
        let ar = RangeRef::new(a.cell, end);
        r = Some(r.map_or(ar, |u| u.union(&ar)));
    };
    for c in sh.charts.iter().filter(|o| !o.hidden && !o.no_print) {
        grow(&c.anchor);
    }
    for i in sh.images.iter().filter(|o| !o.hidden && !o.no_print) {
        grow(&i.anchor);
    }
    for s in sh.shapes.iter().filter(|o| !o.hidden && !o.no_print) {
        grow(&s.anchor);
    }
    // Form controls whose "Print object" setting is on print like any other drawing object.
    for c in sh.controls.iter().filter(|o| o.print) {
        grow(&c.anchor);
    }
    // Embedded (OLE) objects and ActiveX controls aren't drawn, but they print and take room.
    for o in sh.embedded_objects.iter().filter(|o| o.print) {
        grow(&o.anchor);
    }
    // Without a print area Excel prints from A1: leading empty rows and columns print as blank
    // space, and pages that fall wholly inside them print as blank pages.
    r.map(|r| RangeRef::new(CellRef::new(0, 0), r.end))
}

/// Whole rows/columns are trimmed to the used range.
fn trim_full(sh: &Sheet, r: RangeRef) -> RangeRef {
    if !(r.is_full_cols() || r.is_full_rows()) {
        return r;
    }
    match sh.used_range() {
        Some(u) => r.intersection(&RangeRef::new(CellRef::new(0, 0), u.end)).unwrap_or(RangeRef::cell(r.start)),
        None => RangeRef::cell(r.start),
    }
}

/// Splits `start..=end` into bands whose sizes fit `avail(band_start)`; `breaks` force a new
/// band before a line. Stops after `cap` bands.
pub(super) fn bands(start: u32, end: u32, size: impl Fn(u32) -> f32, avail: impl Fn(u32) -> f32, breaks: &[u32], cap: usize) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut cur = start;
    let mut used = 0.0f32;
    let mut room = avail(start);
    let mut line = start;
    loop {
        let sz = size(line);
        let manual = line > cur && breaks.binary_search(&line).is_ok();
        if line > cur && (manual || used + sz > room + 0.01) {
            out.push((cur, line - 1));
            if out.len() >= cap {
                return out;
            }
            cur = line;
            used = 0.0;
            room = avail(line);
        }
        used += sz;
        if line >= end {
            break;
        }
        line += 1;
    }
    out.push((cur, end));
    out
}

/// [`bands`] for columns, except that a line wider than an empty page's room (`avail` at that
/// line, when positive) is cut to that room and printed alone on its page, as Excel prints such
/// a column; `more(line, room)` continuation pages follow it, for text running on past the cut.
/// The line after it starts a new page. Returns the bands and, per band, its [`ColCut`].
pub(super) fn bands_cut_wide(
    start: u32,
    end: u32,
    size: impl Fn(u32) -> f32,
    avail: impl Fn(u32) -> f32,
    breaks: &[u32],
    cap: usize,
    more: impl Fn(u32, f32) -> usize,
) -> (Vec<(u32, u32)>, Vec<ColCut>) {
    let mut out = Vec::new();
    let mut cuts = Vec::new();
    let mut cur = start;
    let mut used = 0.0f32;
    let mut room = avail(start).max(1.0);
    let mut line = start;
    loop {
        let sz = size(line);
        let empty_room = avail(line);
        let wide = sz.is_finite() && empty_room > 0.0 && sz > empty_room + 0.01;
        let manual = line > cur && breaks.binary_search(&line).is_ok();
        if line > cur && (manual || wide || used + sz > room + 0.01) {
            out.push((cur, line - 1));
            cuts.push(ColCut::default());
            if out.len() >= cap {
                return (out, cuts);
            }
            cur = line;
            used = 0.0;
            room = avail(line).max(1.0);
        }
        if wide {
            // The cut line's page, then one page per further printable width its text reaches.
            for i in 0..=more(line, empty_room) {
                out.push((line, line));
                cuts.push(ColCut { offset: i as f32 * empty_room, width: empty_room });
                if out.len() >= cap {
                    return (out, cuts);
                }
            }
            if line >= end {
                return (out, cuts);
            }
            line += 1;
            cur = line;
            used = 0.0;
            room = avail(line).max(1.0);
            continue;
        }
        used += sz;
        if line >= end {
            break;
        }
        line += 1;
    }
    out.push((cur, end));
    cuts.push(ColCut::default());
    (out, cuts)
}

/// How a band of columns prints a column wider than the page (sheet pixels): `width` is the
/// width the column is cut to (0 when the band is not cut) and `offset` how far into the column's
/// text a continuation page starts (0 on the cut column's own page).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColCut {
    pub offset: f32,
    pub width: f32,
}

/// [`split_bands`] for printing: the same bands, except that a column wider than the page is
/// cut to the page's width, followed by pages for the text running on past the cut
/// ([`bands_cut_wide`]), unless the sheet is fitted to a number of pages wide. Returns the row
/// bands, the column bands and each column band's [`ColCut`].
pub(crate) fn split_bands_pieces(wb: &Workbook, sh: &Sheet, lay: &SheetLayout, cap: usize) -> (Bands, Bands, Vec<ColCut>) {
    let (rows, cols) = split_bands(wb, sh, lay, cap);
    let k = lay.k();
    let aw = lay.avail().0 / k;
    let title_w = |start: u32| -> f32 {
        match lay.title_cols {
            Some((a, b)) if a < start => (a..=b.min(start - 1)).map(|c| sh.col_width(c)).sum(),
            _ => 0.0,
        }
    };
    let room = |start: u32| aw - lay.heading_w - title_w(start);
    // Fit To a page count wide picks the scale so the columns fit: no cut (as before).
    // `manual_col_breaks` is off exactly when the sheet is fitted to a page count wide.
    let wide = |&(a, b): &(u32, u32)| (a..=b).any(|c| room(c) > 0.0 && sh.col_width(c) > room(c) + 0.01);
    if !lay.manual_col_breaks || !cols.iter().any(wide) {
        let cuts = vec![ColCut::default(); cols.len()];
        return (rows, cols, cuts);
    }
    let mut cb: Vec<u32> = lay.settings.col_breaks.clone();
    cb.sort_unstable();
    let area = lay.area;
    // Continuation pages: as many printable widths as the longest text of the column (in the
    // printed rows) runs on past the cut and past the columns printed after it.
    let more = |col: u32, cut: f32| -> usize {
        let cut_pt = cut * k;
        if cut_pt.is_nan() || cut_pt <= 0.0 {
            return 0;
        }
        let column = RangeRef::new(CellRef::new(area.start.row, col), CellRef::new(area.end.row, col));
        let mut fonts = None;
        let reach = sh
            .cells
            .iter_range(column)
            .filter(|(c, _)| sh.row_height(c.row) > 0.0)
            .filter_map(|cell| super::text::cut_text_reach(wb, sh, cell, &sh.merges, (k, lay.scale), cut_pt, &mut fonts))
            .fold(0.0f32, f32::max);
        // The columns printed after the cut one carry the text on across their own pages (as
        // Excel prints it): only text reaching past the last of them needs pages of its own.
        let rest: f32 = (col.saturating_add(1)..=area.end.col).map(|c| sh.col_width(c) * k).sum();
        // Saturating float-to-int conversion: a hostile reach only meets the page cap, and a
        // negative count is 0.
        ((reach - cut_pt - rest) / cut_pt).ceil().max(0.0) as usize
    };
    let (cols, cuts) = bands_cut_wide(area.start.col, area.end.col, |c| sh.col_width(c), room, &cb, cap, more);
    (rows, cols, cuts)
}

fn title_span(t: Option<(u32, u32)>, max: u32) -> Option<(u32, u32)> {
    let (a, b) = t?;
    let (a, b) = (a.min(b), a.max(b));
    if a >= max {
        return None;
    }
    // Repeated titles are capped (Excel needs them to fit on a page anyway).
    Some((a, b.min(max - 1).min(a.saturating_add(499))))
}

/// Page width and height and the margins (left, right, top, bottom, header, footer), in points,
/// of a page setup.
fn page_geometry(st: &PrintSettings) -> (f32, f32, [f32; 6]) {
    let (mut pw, mut ph) = paper_size(&st.paper);
    if st.orientation == Orientation::Landscape {
        std::mem::swap(&mut pw, &mut ph);
    }
    let inch = |i: usize| st.margins.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.5).clamp(0.0, 20.0) * 72.0;
    let mut margins = [inch(0), inch(1), inch(2), inch(3), inch(4), inch(5)];
    // Keep at least an inch of printable space.
    for (a, b, total) in [(0usize, 1usize, pw), (2, 3, ph)] {
        let sum = margins[a] + margins[b];
        if total - sum < 72.0 {
            let f = ((total - 72.0).max(0.0)) / sum.max(1.0);
            margins[a] *= f;
            margins[b] *= f;
        }
    }
    (pw, ph, margins)
}

/// The one page of a chart sheet: its own page setup, the chart scaled to fill the printable
/// area (Excel's default for chart sheets).
fn layout_chart_sheet(wb: &Workbook, ci: usize) -> Option<SheetLayout> {
    let cs = wb.chart_sheets.get(ci)?;
    let (page_w, page_h, margins) = page_geometry(&cs.print);
    Some(SheetLayout {
        sheet: cs.before.min(wb.sheets.len().saturating_sub(1)),
        settings: cs.print.clone(),
        page_w,
        page_h,
        margins,
        scale: 1.0,
        area: RangeRef::cell(CellRef::new(0, 0)),
        row_bands: vec![(0, 0)],
        col_bands: vec![(0, 0)],
        col_cuts: vec![],
        pages: vec![(0, 0)],
        title_rows: None,
        title_cols: None,
        heading_w: 0.0,
        heading_h: 0.0,
        manual_breaks: false,
        manual_row_breaks: false,
        manual_col_breaks: false,
        chart_sheet: Some(ci),
        macro_sheet: None,
    })
}

/// Paginates one sheet.
pub fn layout_sheet(wb: &Workbook, si: usize, opts: &PrintOptions, range_override: Option<RangeRef>) -> Option<SheetLayout> {
    layout_of(wb, wb.sheet(si)?, (si, None), opts, range_override)
}

/// Paginates a visible macro sheet's used range over its own page setup, like a worksheet's.
fn layout_macro_sheet(wb: &Workbook, mi: usize, opts: &PrintOptions) -> Option<SheetLayout> {
    let ms = wb.macro_sheets.get(mi)?;
    let si = ms.before.min(wb.sheets.len().saturating_sub(1));
    layout_of(wb, &ms.sheet, (si, Some(mi)), opts, None)
}

/// Paginates `sh`: the workbook's sheet `si`, or macro sheet `mi` when `at` is `(si, Some(mi))`.
fn layout_of(wb: &Workbook, sh: &Sheet, at: (usize, Option<usize>), opts: &PrintOptions, range_override: Option<RangeRef>) -> Option<SheetLayout> {
    let (si, macro_sheet) = at;
    let st = sh.print.clone();
    let (pw, ph, margins) = page_geometry(&st);
    let area = match range_override {
        Some(r) => trim_full(sh, r),
        None => print_range(wb, sh).unwrap_or(RangeRef::cell(CellRef::new(0, 0))),
    };
    // A print area from a file or JSON can name coordinates past the grid; pagination walks every
    // line of the area (zero-height rows never fill a band), so it is held to the grid first.
    let area = RangeRef::new(
        CellRef::new(area.start.row.min(MAX_ROWS - 1), area.start.col.min(MAX_COLS - 1)),
        CellRef::new(area.end.row.min(MAX_ROWS - 1), area.end.col.min(MAX_COLS - 1)),
    );
    let title_rows = title_span(st.title_rows, MAX_ROWS);
    let title_cols = title_span(st.title_cols, MAX_COLS);
    let digits = format!("{}", area.end.row + 1).len() as f32;
    let (heading_w, heading_h) = if st.headings { (10.0 + 7.0 * digits, sh.default_row_height.clamp(10.0, 60.0)) } else { (0.0, 0.0) };
    let fit = match opts.fit_to_page {
        Some(true) => Some((1u16, 1u16)),
        Some(false) => None,
        None => st.fit_to.filter(|(w, h)| *w > 0 || *h > 0),
    };
    let mut lay = SheetLayout {
        sheet: si,
        settings: st.clone(),
        page_w: pw,
        page_h: ph,
        margins,
        scale: (st.scale.clamp(10, 400) as f32) / 100.0,
        area,
        row_bands: vec![],
        col_bands: vec![],
        col_cuts: vec![],
        pages: vec![],
        title_rows,
        title_cols,
        heading_w,
        heading_h,
        manual_breaks: fit.is_none(),
        // An axis with a page count ignores its manual breaks; an automatic (0) axis keeps them.
        manual_row_breaks: fit.is_none_or(|(_, h)| h == 0),
        manual_col_breaks: fit.is_none_or(|(w, _)| w == 0),
        chart_sheet: None,
        macro_sheet,
    };
    // Without a print area the pages below and to the right of everything on the sheet are left
    // out (Excel prints each band of columns down to its last cell); a print area prints its grid.
    let trim_empty = range_override.is_none() && sh.print.print_areas().is_empty();
    let paginate = |lay: &mut SheetLayout| {
        (lay.row_bands, lay.col_bands, lay.col_cuts) = split_bands_pieces(wb, sh, lay, MAX_PAGES);
    };
    match fit {
        Some((fw, fh)) => {
            let total_w: f32 = (area.start.col..=area.end.col).map(|c| sh.col_width(c)).sum::<f32>() + heading_w;
            // The sheet's printed height at 100 %, in points: rows at their printed pitch.
            let total_h_pt: f32 = if area.height() > 200_000 {
                f32::INFINITY
            } else {
                let pitch = RowPitch::new(wb, sh, 1.0);
                let head = if heading_h > 0.0 { pitch.dots_px(heading_h) as f32 * DOT_PT } else { 0.0 };
                pitch.span_pt((area.start.row, area.end.row)) + head
            };
            let (aw, ah) = lay.avail();
            let sw = if fw > 0 { aw * fw as f32 / (total_w * PX_TO_PT).max(1.0) } else { f32::INFINITY };
            let sh_ = if fh > 0 { ah * fh as f32 / total_h_pt.max(1.0) } else { f32::INFINITY };
            lay.scale = sw.min(sh_).min(1.0).clamp(0.1, 1.0);
            for _ in 0..40 {
                paginate(&mut lay);
                let ok_w = fw == 0 || lay.col_bands.len() <= fw as usize;
                let ok_h = fh == 0 || lay.row_bands.len() <= fh as usize;
                if (ok_w && ok_h) || lay.scale <= 0.1 {
                    break;
                }
                lay.scale = (lay.scale * 0.97).max(0.1);
            }
        }
        None => paginate(&mut lay),
    }
    // The printed pages, once the bands (and Fit To's scale) are settled: `printed_reach` visits
    // every stored cell and object, so it does not run on each of Fit To's tries.
    let reach = if trim_empty { printed_reach(wb, sh, &lay) } else { None };
    lay.pages = match reach {
        Some(reach) => printed_page_sequence(&reach, lay.settings.page_order, MAX_PAGES),
        None => page_sequence(lay.row_bands.len(), lay.col_bands.len(), lay.settings.page_order, MAX_PAGES),
    };
    Some(lay)
}

/// The pages of a sheet `rows` pages tall and `cols` pages wide as (row band, column band), in
/// print order, at most `cap` of them: down each column of pages, then over to the next (Excel's
/// default), or over each row of pages, then down to the next.
pub(crate) fn page_sequence(rows: usize, cols: usize, order: PageOrder, cap: usize) -> Vec<(usize, usize)> {
    let (outer, inner) = match order {
        PageOrder::DownThenOver => (cols, rows),
        PageOrder::OverThenDown => (rows, cols),
    };
    (0..outer)
        .flat_map(|o| {
            (0..inner).map(move |i| match order {
                PageOrder::DownThenOver => (i, o),
                PageOrder::OverThenDown => (o, i),
            })
        })
        .take(cap)
        .collect()
}

/// Per column band, the number of row bands printed in it: page (r, c) prints when some page
/// at or below and at or to the right of it (r2 >= r, c2 >= c) has something on it (a stored
/// cell that puts something on paper ([`OnPaper`]), a spill range, a merged range, a printing drawing
/// object, part of a table's range, or text overflowing into it from a cell in an earlier column,
/// as the drawing lets it). The counts never grow from left to right. `None` when nothing lies on
/// any page (the caller then prints the full grid).
fn printed_reach(wb: &Workbook, sh: &Sheet, lay: &SheetLayout) -> Option<Vec<usize>> {
    let (row_bands, col_bands) = (&lay.row_bands, &lay.col_bands);
    // The lowest occupied row band of each column band.
    let mut last: Vec<Option<usize>> = vec![None; col_bands.len()];
    // Marks the rectangle ending in row r1 over columns c0..=c1 (only its lowest row matters). Bands stop at a page cap, so what lies
    // past the last band counts on the last band (everything before it prints).
    let mut mark = |r1: u32, c0: u32, c1: u32| {
        let (Some(first), Some(end), Some(col_end)) = (row_bands.first(), row_bands.last(), col_bands.last()) else { return };
        if r1 < first.0 {
            return;
        }
        // The band holding the rectangle's lowest row on the pages.
        let rb = row_bands.partition_point(|b| b.1 < r1.min(end.1));
        if row_bands.get(rb).is_none_or(|b| b.0 > r1) {
            return;
        }
        let from = col_bands.partition_point(|b| b.1 < c0.min(col_end.1));
        for (ci, b) in col_bands.iter().enumerate().skip(from) {
            if b.0 > c1 {
                break;
            }
            if let Some(slot) = last.get_mut(ci) {
                *slot = Some(slot.map_or(rb, |v| v.max(rb)));
            }
        }
    };
    // Text overflows rightward only (overflow to the left lands on bands the cell's own band
    // already keeps), and only matters when a later band of columns exists.
    let last_band_start = col_bands.last().map_or(0, |b| b.0);
    let page_scale = (lay.k(), lay.scale);
    let mut fonts = None;
    let mut on_paper = OnPaper::new(wb, sh);
    for (c, cell) in sh.cells.iter() {
        if !on_paper.counts(c, cell) {
            continue;
        }
        let end = if c.col < last_band_start { super::text::right_overflow_end(wb, sh, (c, cell), &sh.merges, page_scale, &mut fonts) } else { None };
        mark(c.row, c.col, end.map_or(c.col, |e| e.max(c.col)));
    }
    implicit_row_fill(wb, sh, &mut mark);
    for t in &sh.tables {
        mark(t.range.end.row, t.range.start.col, t.range.end.col);
    }
    for r in sh.spill_ranges.values() {
        mark(r.end.row, r.start.col, r.end.col);
    }
    for m in sh.merges.iter().map(merge_extent) {
        mark(m.end.row, m.start.col, m.end.col);
    }
    let mut object = |a: &Anchor| {
        let x = sh.col_left(a.cell.col) + a.dx as f64;
        let y = sh.row_top(a.cell.row) + a.dy as f64;
        let (r1, c1) = (sh.row_at(y + a.height.max(0.0) as f64), sh.col_at(x + a.width.max(0.0) as f64));
        mark(r1.max(a.cell.row), a.cell.col, c1.max(a.cell.col));
    };
    for c in sh.charts.iter().filter(|o| !o.hidden && !o.no_print) {
        object(&c.anchor);
    }
    for i in sh.images.iter().filter(|o| !o.hidden && !o.no_print) {
        object(&i.anchor);
    }
    for s in sh.shapes.iter().filter(|o| !o.hidden && !o.no_print) {
        object(&s.anchor);
    }
    for c in sh.controls.iter().filter(|o| o.print) {
        object(&c.anchor);
    }
    for o in sh.embedded_objects.iter().filter(|o| o.print) {
        object(&o.anchor);
    }
    // Sweep from the right: a band prints down to the lowest occupied band at or right of it.
    let mut reach = vec![0usize; last.len()];
    let mut below: Option<usize> = None;
    for (ci, l) in last.iter().enumerate().rev() {
        below = match (below, *l) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        if let Some(slot) = reach.get_mut(ci) {
            *slot = below.map_or(0, |v| v + 1);
        }
    }
    below.map(|_| reach)
}

/// [`page_sequence`] cut after the last page that holds anything, as Excel prints a sheet:
/// blank pages before that page are kept. `reach[c]` is how many row bands column band `c` or
/// any band to its right reaches (non-increasing in `c`). Down-then-over prints every row band
/// of each column band before the last and `reach[last]` of the last; over-then-down prints every
/// column band of each row band before the last and, on the last, the bands that reach it. The
/// two orders can therefore print different sets of pages.
fn printed_page_sequence(reach: &[usize], order: PageOrder, cap: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let rows = reach.first().copied().unwrap_or(0);
    let bands = reach.len();
    match order {
        PageOrder::DownThenOver => {
            for (c, &n) in reach.iter().enumerate() {
                if out.len() >= cap {
                    break;
                }
                let n = if c + 1 < bands { rows } else { n };
                out.extend((0..n).map(|r| (r, c)).take(cap.saturating_sub(out.len())));
            }
        }
        PageOrder::OverThenDown => {
            for r in 0..rows {
                if out.len() >= cap {
                    break;
                }
                let cols = if r + 1 < rows { bands } else { reach.partition_point(|&n| n > r) };
                out.extend((0..cols).map(|c| (r, c)).take(cap.saturating_sub(out.len())));
            }
        }
    }
    out.truncate(cap);
    out
}

/// Inclusive-start/end index spans of the pages along one axis.
pub(crate) type Bands = Vec<(u32, u32)>;

/// Splits a layout's area into row and column bands that fit its pages (at most `cap` of each),
/// honouring manual breaks (per axis, `lay.manual_row_breaks` / `lay.manual_col_breaks`), print headings and repeated titles. The
/// one pagination rule shared by printing, PDF export, Page Break Preview and Page Layout view.
/// Rows are cut on their printed height in printer dots ([`RowPitch`]), columns on their width
/// in sheet pixels.
pub(crate) fn split_bands(wb: &Workbook, sh: &Sheet, lay: &SheetLayout, cap: usize) -> (Bands, Bands) {
    let k = lay.k();
    let (aw, ah) = lay.avail();
    let aw = aw / k;
    let pitch = RowPitch::new(wb, sh, lay.scale);
    let page = page_dots(ah) as f32;
    let head = if lay.heading_h > 0.0 { pitch.dots_px(lay.heading_h) as f32 } else { 0.0 };
    let st = &lay.settings;
    let mut rb: Vec<u32> = if lay.manual_row_breaks { st.row_breaks.clone() } else { vec![] };
    let mut cb: Vec<u32> = if lay.manual_col_breaks { st.col_breaks.clone() } else { vec![] };
    rb.sort_unstable();
    cb.sort_unstable();
    let (title_rows, title_cols) = (lay.title_rows, lay.title_cols);
    let title_h = |start: u32| -> f32 {
        match title_rows {
            Some((a, b)) if a < start => (a..=b.min(start - 1)).map(|r| pitch.dots(r) as f32).sum(),
            _ => 0.0,
        }
    };
    let title_w = |start: u32| -> f32 {
        match title_cols {
            Some((a, b)) if a < start => (a..=b.min(start - 1)).map(|c| sh.col_width(c)).sum(),
            _ => 0.0,
        }
    };
    let area = lay.area;
    let rows = bands(area.start.row, area.end.row, |r| pitch.dots(r) as f32, |start| (page - head - title_h(start)).max(1.0), &rb, cap);
    let cols = bands(area.start.col, area.end.col, |c| sh.col_width(c), |start| (aw - lay.heading_w - title_w(start)).max(1.0), &cb, cap);
    (rows, cols)
}

/// A tab printing covers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tab {
    /// A macro sheet (an index into [`Workbook::macro_sheets`]).
    Macro(usize),
    /// A chart sheet (an index into [`Workbook::chart_sheets`]).
    Chart(usize),
    /// A worksheet (an index into [`Workbook::sheets`]).
    Sheet(usize),
}

/// The visible chart and macro sheets in tab order, each with the number of worksheets before it
/// (at most `n`, the worksheet count: past the end means last).
fn other_tabs(wb: &Workbook, n: usize) -> Vec<(usize, Tab)> {
    // Sort key: (worksheets before, chart sheets before, macro before chart, index). A macro sheet
    // with `k` chart sheets before it sits just before chart sheet `k`.
    let charts = wb
        .chart_sheets
        .iter()
        .enumerate()
        .filter(|(_, c)| c.visibility == Visibility::Visible)
        .map(|(ci, c)| ((c.before.min(n), ci, 1), Tab::Chart(ci)));
    let macros = wb
        .macro_sheets
        .iter()
        .enumerate()
        .filter(|(_, m)| m.visibility == Visibility::Visible)
        .map(|(mi, m)| ((m.before.min(n), m.charts_before, 0), Tab::Macro(mi)));
    let mut all: Vec<((usize, usize, u8), Tab)> = charts.chain(macros).collect();
    all.sort();
    all.into_iter().map(|((before, ..), tab)| (before, tab)).collect()
}

/// Lays out every sheet a print command covers. Printing the whole workbook also prints each
/// visible chart sheet as one page and each visible macro sheet like a worksheet, in tab order.
pub fn layouts(wb: &Workbook, opts: &PrintOptions) -> Vec<SheetLayout> {
    let mut out = Vec::new();
    let mut pages = 0usize;
    let mut tabs: Vec<Tab> = Vec::new();
    if opts.all_sheets {
        let n = wb.sheets.len();
        let others = other_tabs(wb, n);
        for i in 0..=n {
            // A chart or macro sheet sits before the worksheet `before` counts up to.
            tabs.extend(others.iter().filter(|(before, _)| *before == i).map(|(_, tab)| *tab));
            if wb.sheet(i).is_some_and(|s| s.visibility == Visibility::Visible) {
                tabs.push(Tab::Sheet(i));
            }
        }
    } else {
        tabs.push(Tab::Sheet(wb.active_sheet()));
    }
    for tab in tabs {
        if pages >= MAX_PAGES {
            break;
        }
        let lay = match tab {
            Tab::Sheet(si) => {
                let ovr = if si == wb.active_sheet() { opts.range } else { None };
                if ovr.is_some() {
                    layout_sheet(wb, si, opts, ovr).into_iter().collect()
                } else {
                    let ranges = wb.sheet(si).map(|sh| print_ranges(wb, sh)).unwrap_or_default();
                    if opts.all_sheets && ranges.is_empty() {
                        continue;
                    }
                    sheet_layouts(wb, si, opts, &ranges)
                }
            }
            Tab::Chart(ci) => layout_chart_sheet(wb, ci).into_iter().collect(),
            // A macro sheet without cells prints nothing, like an empty worksheet.
            Tab::Macro(mi) => match wb.macro_sheets.get(mi) {
                Some(m) if !print_ranges(wb, &m.sheet).is_empty() => layout_macro_sheet(wb, mi, opts).into_iter().collect(),
                _ => continue,
            },
        };
        for mut l in lay {
            if pages >= MAX_PAGES {
                break;
            }
            l.pages.truncate(MAX_PAGES - pages);
            pages += l.pages.len();
            out.push(l);
        }
    }
    if out.is_empty() && opts.all_sheets {
        out.extend(layout_sheet(wb, wb.active_sheet(), opts, None));
    }
    out
}

/// One layout per range of a sheet's print area, in order (a print area of several ranges
/// prints each range starting on a new page); the sheet's default layout when `ranges` is empty.
fn sheet_layouts(wb: &Workbook, si: usize, opts: &PrintOptions, ranges: &[RangeRef]) -> Vec<SheetLayout> {
    match ranges {
        [] | [_] => layout_sheet(wb, si, opts, None).into_iter().collect(),
        _ => ranges.iter().filter_map(|r| layout_sheet(wb, si, opts, Some(*r))).collect(),
    }
}

/// Segments of a page along one axis: repeated titles (before the band) then the band.
pub(super) fn segments(titles: Option<(u32, u32)>, band: (u32, u32)) -> Vec<(u32, u32)> {
    let mut v = Vec::new();
    if let Some((a, b)) = titles
        && a < band.0
    {
        v.push((a, b.min(band.0 - 1)));
    }
    v.push(band);
    v
}

pub(super) fn seg_size(sh: &Sheet, seg: (u32, u32), rows: bool) -> f32 {
    if rows { (seg.0..=seg.1).map(|r| sh.row_height(r)).sum() } else { (seg.0..=seg.1).map(|c| sh.col_width(c)).sum() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_print_area_past_the_grid_is_held_to_the_grid() {
        let mut wb = Workbook::new();
        let sh = wb.sheet_mut(0).unwrap();
        sh.default_row_height = 0.0;
        sh.print.print_area = Some(RangeRef { start: CellRef::new(0, 0), end: CellRef::new(u32::MAX, u32::MAX) });
        let lay = layout_sheet(&wb, 0, &PrintOptions::default(), None).unwrap();
        assert_eq!((lay.area.end.row, lay.area.end.col), (MAX_ROWS - 1, MAX_COLS - 1));
        assert!(lay.pages.len() <= MAX_PAGES);
    }

    #[test]
    fn a_wide_line_is_cut_to_the_room_and_the_next_starts_a_new_band() {
        // Room 100: line 0 is 250 wide, cut to 100; its text needs two more pages. Line 1 (40)
        // starts a new band and line 2 (40) joins it.
        let w = [250.0f32, 40.0, 40.0];
        let size = |c: u32| w.get(c as usize).copied().unwrap_or(0.0);
        let cut = |offset: f32| ColCut { offset, width: 100.0 };
        let (b, o) = bands_cut_wide(0, 2, size, |_| 100.0, &[], 100, |_, _| 2);
        assert_eq!(b, [(0, 0), (0, 0), (0, 0), (1, 2)]);
        assert_eq!(o, [cut(0.0), cut(100.0), cut(200.0), ColCut::default()]);
        // Without long text the cut line takes one band; a manual break still applies.
        let (b, o) = bands_cut_wide(0, 2, size, |_| 100.0, &[2], 100, |_, _| 0);
        assert_eq!(b, [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(o, [cut(0.0), ColCut::default(), ColCut::default()]);
        // A wide line after a narrow one moves to a band of its own.
        let (b, _) = bands_cut_wide(0, 2, |c| [40.0f32, 250.0, 40.0].get(c as usize).copied().unwrap_or(0.0), |_| 100.0, &[], 100, |_, _| 0);
        assert_eq!(b, [(0, 0), (1, 1), (2, 2)]);
        // No room means no cut; the page cap holds for continuation pages too.
        let (b, o) = bands_cut_wide(0, 2, size, |_| 0.0, &[], 100, |_, _| 0);
        assert_eq!(b, bands(0, 2, size, |_| 1.0, &[], 100));
        assert!(o.iter().all(|x| *x == ColCut::default()));
        let (b, _) = bands_cut_wide(0, 2, size, |_| 100.0, &[], 3, |_, _| usize::MAX);
        assert_eq!(b.len(), 3);
    }
}
