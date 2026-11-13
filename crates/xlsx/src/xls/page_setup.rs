//! Page setup records of a worksheet substream ([MS-XLS] 2.4): Setup, the four margins,
//! Header/Footer, HeaderFooter, PrintGrid/PrintRowCol/HCenter/VCenter, the page break lists and
//! SheetExt (the tab colour). Each maps onto the print settings the `.xlsx` reader fills from
//! `<pageSetup>`, `<pageMargins>`, `<headerFooter>`, `<printOptions>`, `<rowBreaks>`/`<colBreaks>`
//! and `<tabColor>`. A record too short for its fields is skipped with a warning.

use gridcraft_core::{MAX_COLS, MAX_ROWS};
use gridcraft_model::{Color, Orientation, PrintSettings};

use super::formula::Biff5;
use super::records::{Cursor, Record};
use crate::ReadReport;
use crate::tables::{INDEXED, paper_name};

pub(super) const SETUP: u16 = 0x00A1;
pub(super) const LEFT_MARGIN: u16 = 0x0026;
pub(super) const RIGHT_MARGIN: u16 = 0x0027;
pub(super) const TOP_MARGIN: u16 = 0x0028;
pub(super) const BOTTOM_MARGIN: u16 = 0x0029;
pub(super) const HEADER: u16 = 0x0014;
pub(super) const FOOTER: u16 = 0x0015;
pub(super) const HEADER_FOOTER: u16 = 0x089C;
pub(super) const PRINT_GRID: u16 = 0x002B;
pub(super) const PRINT_ROW_COL: u16 = 0x002A;
pub(super) const HCENTER: u16 = 0x0083;
pub(super) const VCENTER: u16 = 0x0084;
pub(super) const HORIZONTAL_BREAKS: u16 = 0x001B;
pub(super) const VERTICAL_BREAKS: u16 = 0x001A;
pub(super) const SHEET_EXT: u16 = 0x0862;

/// Excel keeps header and footer texts to 255 characters (the `.xlsx` reader caps them the same).
const MAX_HF_CHARS: usize = 255;
/// Page breaks kept per direction at most (Excel allows 1,026).
const MAX_BREAKS: usize = 4096;

/// What the page setup records leave for the end of the sheet: Setup's fit-to-page counts
/// apply only when WsBool's `fFitToPage` is set, and the two records can come in either order.
#[derive(Default)]
pub(super) struct PageState {
    pub fit_to_page: bool,
    pub fit: Option<(u16, u16)>,
    /// Which of LeftMargin, RightMargin, TopMargin, BottomMargin the substream has.
    margins: [bool; 4],
}

/// The margins (left, right, top, bottom, inches) of a worksheet substream without the margin
/// records: Excel's old defaults, as Excel prints such a file and writes it back.
const DEFAULT_MARGINS: [f32; 4] = [0.75, 0.75, 1.0, 1.0];

impl PageState {
    pub(super) fn finish(&self, pr: &mut PrintSettings) {
        for ((seen, default), m) in self.margins.iter().zip(DEFAULT_MARGINS).zip(pr.margins.iter_mut()) {
            if !seen {
                *m = default;
            }
        }
        if self.fit_to_page
            && let Some(fit) = self.fit
        {
            pr.fit_to = Some(fit);
        }
    }
}

/// Reads one page setup record into the sheet; `false` when `id` isn't one of them. `biff5` is
/// the workbook's BIFF5 tables when the file is Excel 5.0/95 (its Header/Footer text is 8-bit).
pub(super) fn read(
    rec: &Record<'_>,
    pr: &mut PrintSettings,
    tab: &mut Option<Color>,
    page: &mut PageState,
    biff5: Option<&Biff5>,
    report: &mut ReadReport,
) -> bool {
    let mut c = Cursor::new(&rec.parts);
    let ok = match rec.id {
        SETUP => setup(&mut c, pr, page),
        LEFT_MARGIN | RIGHT_MARGIN | TOP_MARGIN | BOTTOM_MARGIN => {
            if let Some(seen) = page.margins.get_mut(margin_slot(rec.id)) {
                *seen = true;
            }
            margin(&mut c, rec.id, pr)
        }
        HEADER => header_footer_text(&mut c, biff5).map(|t| pr.header = t),
        FOOTER => header_footer_text(&mut c, biff5).map(|t| pr.footer = t),
        HEADER_FOOTER => header_footer(&mut c, pr),
        PRINT_GRID => flag(&mut c).map(|on| pr.gridlines = on),
        PRINT_ROW_COL => flag(&mut c).map(|on| pr.headings = on),
        HCENTER => flag(&mut c).map(|on| pr.center_h = on),
        VCENTER => flag(&mut c).map(|on| pr.center_v = on),
        HORIZONTAL_BREAKS => breaks(&mut c, MAX_ROWS, biff5.is_some()).map(|b| pr.row_breaks = b),
        VERTICAL_BREAKS => breaks(&mut c, MAX_COLS, biff5.is_some()).map(|b| pr.col_breaks = b),
        SHEET_EXT => sheet_ext(&mut c).map(|color| *tab = color),
        _ => return false,
    };
    if ok.is_none() {
        report.warn(format!("Some .xls page setup records ({}) were damaged and skipped.", name(rec.id)));
    }
    true
}

fn name(id: u16) -> &'static str {
    match id {
        SETUP => "page setup",
        LEFT_MARGIN | RIGHT_MARGIN | TOP_MARGIN | BOTTOM_MARGIN => "margins",
        HEADER | FOOTER | HEADER_FOOTER => "headers and footers",
        PRINT_GRID | PRINT_ROW_COL | HCENTER | VCENTER => "print options",
        HORIZONTAL_BREAKS | VERTICAL_BREAKS => "page breaks",
        _ => "sheet tab colour",
    }
}

/// Setup (2.4.257).
fn setup(c: &mut Cursor<'_, '_>, pr: &mut PrintSettings, page: &mut PageState) -> Option<()> {
    let paper = c.u16().ok()?;
    let scale = c.u16().ok()?;
    let _page_start = c.u16().ok()?;
    let fit_w = c.u16().ok()?;
    let fit_h = c.u16().ok()?;
    let flags = c.u16().ok()?;
    let _res = c.u16().ok()?;
    let _vres = c.u16().ok()?;
    let header = c.f64().ok()?;
    let footer = c.f64().ok()?;
    let on = |bit: u16| flags & bit != 0;
    // fNoPls: paper size, scale, resolution, orientation and copies are undefined.
    if !on(0x0004) {
        if let Some(p) = paper_name(u32::from(paper)) {
            pr.paper = p;
        }
        // Like `<pageSetup scale="0">`, a zero scale is unstated.
        if scale != 0 {
            pr.scale = scale.clamp(10, 400);
        }
        // fPortrait clear is landscape, unless fNoOrient says the orientation is the default.
        // Portrait is set explicitly: a chart sheet starts landscape.
        pr.orientation = if !on(0x0002) && !on(0x0040) { Orientation::Landscape } else { Orientation::Portrait };
    }
    page.fit = Some((fit_w.min(32767), fit_h.min(32767)));
    for (slot, v) in [(4, header), (5, footer)] {
        if let Some(m) = pr.margins.get_mut(slot) {
            *m = inches(v);
        }
    }
    Some(())
}

fn inches(v: f64) -> f32 {
    if v.is_finite() { v.clamp(0.0, 50.0) as f32 } else { 0.0 }
}

/// LeftMargin, RightMargin, TopMargin, BottomMargin: one Xnum in inches.
fn margin(c: &mut Cursor<'_, '_>, id: u16, pr: &mut PrintSettings) -> Option<()> {
    let v = c.f64().ok()?;
    *pr.margins.get_mut(margin_slot(id))? = inches(v);
    Some(())
}

/// The print settings' margin slot of a margin record.
fn margin_slot(id: u16) -> usize {
    match id {
        LEFT_MARGIN => 0,
        RIGHT_MARGIN => 1,
        TOP_MARGIN => 2,
        _ => 3,
    }
}

/// Header / Footer (2.4.136, 2.4.124): empty for none, else an XLUnicodeString; in BIFF5 a
/// 1-byte count and that many bytes in the workbook code page.
fn header_footer_text(c: &mut Cursor<'_, '_>, biff5: Option<&Biff5>) -> Option<String> {
    if c.remaining() == 0 {
        return Some(String::new());
    }
    let text = match biff5 {
        Some(b5) => {
            let cch = usize::from(c.u8().ok()?);
            b5.text(&c.bytes(cch).ok()?)
        }
        None => c.string().ok()?,
    };
    Some(text.chars().take(MAX_HF_CHARS).collect())
}

/// HeaderFooter (2.4.137): FrtHeader, guidSView, flags, four counts, then the even and first
/// page texts (each omitted when its count is zero).
fn header_footer(c: &mut Cursor<'_, '_>, pr: &mut PrintSettings) -> Option<()> {
    c.skip(12).ok()?;
    let mut guid = [0u8; 16];
    for b in &mut guid {
        *b = c.u8().ok()?;
    }
    // A non-zero GUID belongs to a custom view, not to the sheet itself.
    if guid.iter().any(|&b| b != 0) {
        return Some(());
    }
    let flags = c.u16().ok()?;
    let counts = [c.u16().ok()?, c.u16().ok()?, c.u16().ok()?, c.u16().ok()?];
    let mut texts: [String; 4] = Default::default();
    for (cch, text) in counts.iter().zip(texts.iter_mut()) {
        if *cch > 0 {
            *text = c.string().ok()?.chars().take(MAX_HF_CHARS).collect();
        }
    }
    let [even_header, even_footer, first_header, first_footer] = texts;
    let on = |bit: u16| flags & bit != 0;
    pr.different_odd_even = on(0x0001);
    pr.different_first = on(0x0002);
    pr.hf_scale_with_doc = on(0x0004);
    pr.hf_align_with_margins = on(0x0008);
    pr.even_header = even_header;
    pr.even_footer = even_footer;
    pr.first_header = first_header;
    pr.first_footer = first_footer;
    Some(())
}

/// PrintGrid, PrintRowCol, HCenter, VCenter: a 16-bit Boolean (bit 0).
fn flag(c: &mut Cursor<'_, '_>) -> Option<bool> {
    Some(c.u16().ok()? & 1 != 0)
}

/// HorizontalPageBreaks / VerticalPageBreaks (2.4.142, 2.4.350): a count, then 6-byte entries
/// whose first field is the first row (column) after the break, the `.xlsx` `<brk id>`. Excel
/// 5.0/95 (`biff5`) writes 2-byte entries holding only that row (column).
fn breaks(c: &mut Cursor<'_, '_>, limit: u32, biff5: bool) -> Option<Vec<u32>> {
    let n = usize::from(c.u16().ok()?);
    let entry = if biff5 { 2 } else { 6 };
    if c.remaining() < n.saturating_mul(entry) {
        return None;
    }
    let mut out = Vec::with_capacity(n.min(MAX_BREAKS));
    for _ in 0..n {
        let at = u32::from(c.u16().ok()?);
        c.skip(entry - 2).ok()?;
        if at > 0 && at < limit && out.len() < MAX_BREAKS && !out.contains(&at) {
            out.push(at);
        }
    }
    Some(out)
}

/// SheetExt (2.4.264): FrtHeader, cb, icvPlain, then (cb > 0x14) SheetExtOptional, whose
/// CFColor tab colour wins when it is an RGB colour. `Some(None)` is "no tab colour".
fn sheet_ext(c: &mut Cursor<'_, '_>) -> Option<Option<Color>> {
    c.skip(12).ok()?;
    let cb = c.u32().ok()?;
    let plain = c.u32().ok()? & 0x7F;
    if cb > 0x14 && c.remaining() >= 12 {
        // SheetExtOptional: icvPlain12 and its flags (4 bytes), then the CFColor.
        c.skip(4).ok()?;
        let kind = c.u32().ok()?;
        if kind == 2 {
            let (r, g, b) = (c.u8().ok()?, c.u8().ok()?, c.u8().ok()?);
            return Some(Some(Color::Rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))));
        }
    }
    // 0x7F is the default (no colour); system colours past the palette are none either.
    Some(usize::try_from(plain).ok().and_then(|i| INDEXED.get(i)).map(|&rgb| Color::Rgb(rgb)))
}
