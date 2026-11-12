use super::header_footer::HF_FIXED_SIDE_MARGIN;
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
    /// Per column band, how far into its first column (sheet pixels) the page starts: above 0
    /// for the continuation pieces of a column wider than the page, which is printed in pieces
    /// across pages. Missing entries are 0.
    pub col_offsets: Vec<f32>,
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
}

impl SheetLayout {
    pub(super) fn k(&self) -> f32 {
        PX_TO_PT * self.scale
    }
    /// How far into its first column (sheet pixels) column band `ci` starts (0 unless it
    /// continues a column wider than the page).
    pub(crate) fn col_offset(&self, ci: usize) -> f32 {
        self.col_offsets.get(ci).copied().unwrap_or(0.0)
    }
    /// Column band `ci` is a piece of a column wider than the page: it continues one, or the
    /// next band continues it.
    pub(crate) fn col_split(&self, ci: usize) -> bool {
        self.col_offset(ci) > 0.0
            || (self.col_offset(ci + 1) > 0.0 && self.col_bands.get(ci).zip(self.col_bands.get(ci + 1)).is_some_and(|(a, b)| a.1 == b.0))
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

/// The range a sheet prints: its print area, else A1 to the end of the used range grown to cover
/// visible drawing objects. `None` when there is nothing to print.
pub fn print_range(sh: &Sheet) -> Option<RangeRef> {
    print_ranges(sh).into_iter().next()
}

/// Every range a sheet prints, in print order: each range of its print area (Excel starts each
/// one on a new page), else the one range [`print_range`] describes. Empty when there is
/// nothing to print.
pub fn print_ranges(sh: &Sheet) -> Vec<RangeRef> {
    let areas = sh.print.print_areas();
    if !areas.is_empty() {
        return areas.into_iter().map(|a| trim_full(sh, a)).collect();
    }
    used_print_range(sh).into_iter().collect()
}

/// A1 to the end of the used range grown to cover visible drawing objects.
fn used_print_range(sh: &Sheet) -> Option<RangeRef> {
    let mut r = sh.used_range();
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
/// line, when positive) is printed in pieces of that room, each a band of its own (Excel prints
/// such a column across pages). Returns the bands and, per band, the offset into its first line
/// where it starts. The line after the last piece joins its page when it fits in what is left.
pub(super) fn bands_split_wide(
    start: u32,
    end: u32,
    size: impl Fn(u32) -> f32,
    avail: impl Fn(u32) -> f32,
    breaks: &[u32],
    cap: usize,
) -> (Vec<(u32, u32)>, Vec<f32>) {
    let mut out = Vec::new();
    let mut offs = Vec::new();
    let mut cur = start;
    let mut cur_off = 0.0f32;
    let mut used = 0.0f32;
    let mut room = avail(start).max(1.0);
    let mut line = start;
    loop {
        let sz = size(line);
        let manual = line > cur && breaks.binary_search(&line).is_ok();
        if line > cur && (manual || used + sz > room + 0.01) {
            out.push((cur, line - 1));
            offs.push(cur_off);
            if out.len() >= cap {
                return (out, offs);
            }
            cur = line;
            cur_off = 0.0;
            used = 0.0;
            room = avail(line).max(1.0);
        }
        // A line on an empty page that is wider than the page: every piece but the last is a
        // page of its own; the last stays open for the lines after it.
        let empty_room = avail(line);
        if line == cur && used == 0.0 && sz.is_finite() && empty_room > 0.0 && sz > empty_room + 0.01 {
            let mut off = 0.0f32;
            while sz - off > room + 0.01 {
                out.push((line, line));
                offs.push(off);
                if out.len() >= cap {
                    return (out, offs);
                }
                off += room;
            }
            cur_off = off;
            used = sz - off;
        } else {
            used += sz;
        }
        if line >= end {
            break;
        }
        line += 1;
    }
    out.push((cur, end));
    offs.push(cur_off);
    (out, offs)
}

/// [`split_bands`] for printing: the same bands, except that a column wider than the page is
/// split into pieces across pages ([`bands_split_wide`]) unless the sheet is fitted to a number
/// of pages wide. Returns the row bands, the column bands and each column band's offset.
pub(crate) fn split_bands_pieces(sh: &Sheet, lay: &SheetLayout, cap: usize) -> (Bands, Bands, Vec<f32>) {
    let (rows, cols) = split_bands(sh, lay, cap);
    let k = lay.k();
    let aw = lay.avail().0 / k;
    let title_w = |start: u32| -> f32 {
        match lay.title_cols {
            Some((a, b)) if a < start => (a..=b.min(start - 1)).map(|c| sh.col_width(c)).sum(),
            _ => 0.0,
        }
    };
    let room = |start: u32| aw - lay.heading_w - title_w(start);
    // Fit To a page count wide picks the scale so the columns fit: no pieces (as before).
    // `manual_col_breaks` is off exactly when the sheet is fitted to a page count wide.
    let wide = |&(a, b): &(u32, u32)| (a..=b).any(|c| room(c) > 0.0 && sh.col_width(c) > room(c) + 0.01);
    if !lay.manual_col_breaks || !cols.iter().any(wide) {
        let offs = vec![0.0; cols.len()];
        return (rows, cols, offs);
    }
    let mut cb: Vec<u32> = lay.settings.col_breaks.clone();
    cb.sort_unstable();
    let area = lay.area;
    let (cols, offs) = bands_split_wide(area.start.col, area.end.col, |c| sh.col_width(c), room, &cb, cap);
    (rows, cols, offs)
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
        col_offsets: vec![],
        pages: vec![(0, 0)],
        title_rows: None,
        title_cols: None,
        heading_w: 0.0,
        heading_h: 0.0,
        manual_breaks: false,
        manual_row_breaks: false,
        manual_col_breaks: false,
        chart_sheet: Some(ci),
    })
}

/// Paginates one sheet.
pub fn layout_sheet(wb: &Workbook, si: usize, opts: &PrintOptions, range_override: Option<RangeRef>) -> Option<SheetLayout> {
    let sh = wb.sheet(si)?;
    let st = sh.print.clone();
    let (pw, ph, margins) = page_geometry(&st);
    let area = match range_override {
        Some(r) => trim_full(sh, r),
        None => print_range(sh).unwrap_or(RangeRef::cell(CellRef::new(0, 0))),
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
        col_offsets: vec![],
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
    };
    // Without a print area the pages below and to the right of everything on the sheet are left
    // out (Excel prints each band of columns down to its last cell); a print area prints its grid.
    let trim_empty = range_override.is_none() && sh.print.print_areas().is_empty();
    let paginate = |lay: &mut SheetLayout| {
        (lay.row_bands, lay.col_bands, lay.col_offsets) = split_bands_pieces(sh, lay, MAX_PAGES);
        let reach = if trim_empty { printed_reach(sh, &lay.row_bands, &lay.col_bands) } else { None };
        lay.pages = match reach {
            Some(reach) => printed_page_sequence(&reach, lay.settings.page_order, MAX_PAGES),
            None => page_sequence(lay.row_bands.len(), lay.col_bands.len(), lay.settings.page_order, MAX_PAGES),
        };
    };
    match fit {
        Some((fw, fh)) => {
            let total_w: f32 = (area.start.col..=area.end.col).map(|c| sh.col_width(c)).sum::<f32>() + heading_w;
            let total_h: f32 = if area.height() > 200_000 {
                f32::INFINITY
            } else {
                (area.start.row..=area.end.row).map(|r| sh.row_height(r)).sum::<f32>() + heading_h
            };
            let (aw, ah) = lay.avail();
            let sw = if fw > 0 { aw * fw as f32 / (total_w * PX_TO_PT).max(1.0) } else { f32::INFINITY };
            let sh_ = if fh > 0 { ah * fh as f32 / (total_h * PX_TO_PT).max(1.0) } else { f32::INFINITY };
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
/// cell, a spill range, a merged range whose anchor cell is stored, or a printing drawing
/// object). The counts never grow from left to right. `None` when nothing lies on any page (the
/// caller then prints the full grid).
fn printed_reach(sh: &Sheet, row_bands: &[(u32, u32)], col_bands: &[(u32, u32)]) -> Option<Vec<usize>> {
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
    for (c, _) in sh.cells.iter() {
        mark(c.row, c.col, c.col);
    }
    for r in sh.spill_ranges.values() {
        mark(r.end.row, r.start.col, r.end.col);
    }
    for m in sh.merges.iter().filter(|m| sh.cells.get(m.start).is_some()) {
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

/// [`page_sequence`] over only the printed pages: `reach[c]` row bands of column band `c`
/// (non-increasing in `c`). Over-then-down prints the same pages in a different sequence.
fn printed_page_sequence(reach: &[usize], order: PageOrder, cap: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    match order {
        PageOrder::DownThenOver => {
            for (c, &n) in reach.iter().enumerate() {
                out.extend((0..n).map(|r| (r, c)).take(cap.saturating_sub(out.len())));
            }
        }
        PageOrder::OverThenDown => {
            let rows = reach.first().copied().unwrap_or(0);
            for r in 0..rows {
                if out.len() >= cap {
                    break;
                }
                let cols = reach.partition_point(|&n| n > r);
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
pub(crate) fn split_bands(sh: &Sheet, lay: &SheetLayout, cap: usize) -> (Bands, Bands) {
    let k = lay.k();
    let (aw, ah) = lay.avail();
    let (aw, ah) = (aw / k, ah / k);
    let st = &lay.settings;
    let mut rb: Vec<u32> = if lay.manual_row_breaks { st.row_breaks.clone() } else { vec![] };
    let mut cb: Vec<u32> = if lay.manual_col_breaks { st.col_breaks.clone() } else { vec![] };
    rb.sort_unstable();
    cb.sort_unstable();
    let (title_rows, title_cols) = (lay.title_rows, lay.title_cols);
    let title_h = |start: u32| -> f32 {
        match title_rows {
            Some((a, b)) if a < start => (a..=b.min(start - 1)).map(|r| sh.row_height(r)).sum(),
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
    let rows = bands(area.start.row, area.end.row, |r| sh.row_height(r), |start| (ah - lay.heading_h - title_h(start)).max(1.0), &rb, cap);
    let cols = bands(area.start.col, area.end.col, |c| sh.col_width(c), |start| (aw - lay.heading_w - title_w(start)).max(1.0), &cb, cap);
    (rows, cols)
}

/// Lays out every sheet a print command covers. Printing the whole workbook also prints each
/// visible chart sheet as one page, in tab order.
pub fn layouts(wb: &Workbook, opts: &PrintOptions) -> Vec<SheetLayout> {
    let mut out = Vec::new();
    let mut pages = 0usize;
    // Tabs in order: `Ok(sheet)` for a worksheet, `Err(chart sheet)` for a chart sheet.
    let mut tabs: Vec<std::result::Result<usize, usize>> = Vec::new();
    if opts.all_sheets {
        let n = wb.sheets.len();
        for i in 0..=n {
            // A chart sheet sits before the worksheet `before` counts up to; past the end, last.
            let charts = wb.chart_sheets.iter().enumerate().filter(|(_, c)| c.visibility == Visibility::Visible);
            tabs.extend(charts.filter(|(_, c)| if i == n { c.before >= n } else { c.before == i }).map(|(ci, _)| Err(ci)));
            if wb.sheet(i).is_some_and(|s| s.visibility == Visibility::Visible) {
                tabs.push(Ok(i));
            }
        }
    } else {
        tabs.push(Ok(wb.active_sheet()));
    }
    for tab in tabs {
        if pages >= MAX_PAGES {
            break;
        }
        let lay = match tab {
            Ok(si) => {
                let ovr = if si == wb.active_sheet() { opts.range } else { None };
                if ovr.is_some() {
                    layout_sheet(wb, si, opts, ovr).into_iter().collect()
                } else {
                    let ranges = wb.sheet(si).map(print_ranges).unwrap_or_default();
                    if opts.all_sheets && ranges.is_empty() {
                        continue;
                    }
                    sheet_layouts(wb, si, opts, &ranges)
                }
            }
            Err(ci) => layout_chart_sheet(wb, ci).into_iter().collect(),
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
    fn a_wide_line_is_split_and_the_next_joins_its_last_piece() {
        // Room 100: line 0 is 250 wide (pieces at 0, 100, 200; 50 left on the last), line 1
        // (40) fits after it, line 2 (40) does not.
        let w = [250.0f32, 40.0, 40.0];
        let size = |c: u32| w.get(c as usize).copied().unwrap_or(0.0);
        let (b, o) = bands_split_wide(0, 2, size, |_| 100.0, &[], 100);
        assert_eq!(b, [(0, 0), (0, 0), (0, 1), (2, 2)]);
        assert_eq!(o, [0.0, 100.0, 200.0, 0.0]);
        // A manual break after the wide line still applies; no room means no split.
        let (b, _) = bands_split_wide(0, 2, size, |_| 100.0, &[1], 100);
        assert_eq!(b, [(0, 0), (0, 0), (0, 0), (1, 2)]);
        let (b, o) = bands_split_wide(0, 2, size, |_| 0.0, &[], 100);
        assert_eq!(b, bands(0, 2, size, |_| 1.0, &[], 100));
        assert!(o.iter().all(|x| *x == 0.0));
    }
}
