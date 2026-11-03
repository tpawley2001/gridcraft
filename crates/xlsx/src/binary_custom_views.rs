//! Custom Views translated to the existing XLSX preservation slots.
//!
//! Layout sources: [MS-XLS] UserBView, UserSViewBegin, Pane, Selection, TabId;
//! [MS-XLSB] BrtUserBookView, BrtBeginUserShView, BrtPane, BrtSel.
//! GUID fields use [MS-DTYP] 2.3.4.2 (the first three components are little endian).
use crate::ReadReport;
use gridcraft_core::CellRef;
use gridcraft_model::{Preserved, PreservedXml};
use std::sync::Arc;

pub(crate) const LIMIT: usize = 1024;
const WARNING: &str = "Some Custom Views were damaged or exceeded the import limit and were dropped.";
pub(crate) fn warn(report: &mut ReadReport) {
    report.warn(WARNING);
}

struct Data<'a>(&'a [u8]);
impl<'a> Data<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let out = self.0.get(..n)?;
        self.0 = self.0.get(n..)?;
        Some(out)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(self.u32()? as i32)
    }
    fn f64(&mut self) -> Option<f64> {
        let v = f64::from_le_bytes(self.take(8)?.try_into().ok()?);
        (v.is_finite() && v >= 0.0).then_some(v)
    }
    fn name(&mut self, binary: bool) -> Option<String> {
        Some(if binary {
            let n = usize::try_from(self.u32()?).ok()?;
            if !(1..=255).contains(&n) {
                return None;
            }
            let units = self.take(n.checked_mul(2)?)?.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect::<Vec<_>>();
            String::from_utf16(&units).ok()?
        } else {
            let n = usize::from(self.u16()?);
            if !(1..=255).contains(&n) {
                return None;
            }
            let flags = self.u8()?;
            // XLUnicodeString has neither formatting runs nor phonetic data.
            if flags & !1 != 0 {
                return None;
            }
            if flags & 1 != 0 {
                let units = self.take(n.checked_mul(2)?)?.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect::<Vec<_>>();
                String::from_utf16(&units).ok()?
            } else {
                self.take(n)?.iter().map(|&b| char::from(b)).collect()
            }
        })
    }
    fn guid(&mut self) -> Option<String> {
        let a = self.u32()?;
        let b = self.u16()?;
        let c = self.u16()?;
        let d = self.u16()?.swap_bytes();
        let mut tail = String::new();
        for v in self.take(6)? {
            tail.push_str(&format!("{v:02X}"));
        }
        Some(format!("{{{a:08X}-{b:04X}-{c:04X}-{d:04X}-{tail}}}"))
    }
}
fn attr(xml: &mut String, key: &str, value: impl std::fmt::Display) {
    xml.push_str(&format!(" {key}=\"{}\"", crate::xml::esc_attr(&value.to_string())));
}
fn bit(xml: &mut String, key: &str, flags: u32, shift: u32) {
    attr(xml, key, u8::from(flags & (1 << shift) != 0));
}
fn enum_value(value: u32, values: &[&str]) -> Option<String> {
    values.get(usize::try_from(value).ok()?).map(|v| (*v).to_owned())
}
fn cell(row: u32, col: u32, binary: bool) -> Option<String> {
    (row < if binary { 1_048_576 } else { 65536 } && col < if binary { 16384 } else { 256 }).then(|| CellRef::new(row, col).to_string())
}
fn keep(into: &mut Option<Arc<Preserved>>, slot: &str, body: String) {
    if body.is_empty() {
        return;
    }
    let xml = format!("<{slot}>{body}</{slot}>");
    crate::passthrough::merge_preserved(
        into,
        Some(Arc::new(Preserved { elements: vec![PreservedXml { slot: slot.into(), xml }], ..Preserved::default() })),
    );
}
/// Returns the source sheet identifier and an unfinished start tag.
pub(crate) fn book(bytes: &[u8], binary: bool) -> Option<(u32, String)> {
    let mut d = Data(bytes);
    let (id, guid, x, y, width, height, ratio, f, personal, minimized, interval);
    if binary {
        x = i64::from(d.i32()?);
        let right = i64::from(d.i32()?);
        y = i64::from(d.i32()?);
        let bottom = i64::from(d.i32()?);
        width = right.checked_sub(x)?;
        height = bottom.checked_sub(y)?;
        id = d.u32()?;
        ratio = d.u32()?;
        guid = d.guid()?;
        interval = d.u16()?;
        f = d.u32()?;
        personal = (f >> 16) & 1;
        minimized = f & 1;
    } else {
        d.take(4)?;
        let tab = u32::from(d.u16()?);
        d.take(2)?;
        guid = d.guid()?;
        // BIFF8 coordinates are pixels; SpreadsheetML window coordinates are twips.
        x = i64::from(d.i32()?) * 15;
        y = i64::from(d.i32()?) * 15;
        width = i64::from(d.i32()?) * 15;
        height = i64::from(d.i32()?) * 15;
        ratio = u32::from(d.u16()?);
        f = u32::from(d.u16()?);
        d.take(2)?;
        let more = d.u16()?;
        personal = u32::from(more & 1);
        minimized = u32::from((more >> 1) & 1);
        interval = d.u16()?;
        id = if f & (1 << 12) != 0 { 0 } else { tab };
    }
    let maximized = f & (1 << if binary { 4 } else { 7 }) != 0;
    if i32::try_from(x).is_err()
        || i32::try_from(y).is_err()
        || u32::try_from(width).is_err()
        || u32::try_from(height).is_err()
        || ratio > 1000
        || (maximized && minimized != 0)
    {
        return None;
    }
    let name = d.name(binary)?;
    let mut xml = "<customWorkbookView".to_owned();
    for (k, v) in [("name", name), ("guid", guid)] {
        attr(&mut xml, k, v);
    }
    for (k, v) in [("xWindow", x), ("yWindow", y), ("windowWidth", width), ("windowHeight", height)] {
        attr(&mut xml, k, v);
    }
    attr(&mut xml, "tabRatio", ratio);
    let shifts = if binary { [1, 2, 3, 5, 6, 4, 11, 12, 13, 14, 15] } else { [4, 5, 6, 0, 1, 7, 10, 11, 13, 14, 15] };
    for (key, shift) in [
        "showHorizontalScroll",
        "showVerticalScroll",
        "showSheetTabs",
        "showFormulaBar",
        "showStatusbar",
        "maximized",
        "includePrintSettings",
        "includeHiddenRowCol",
        "autoUpdate",
        "changesSavedWin",
        "onlySync",
    ]
    .into_iter()
    .zip(shifts)
    {
        bit(&mut xml, key, f, shift);
    }
    let comments = if binary {
        enum_value((f >> 7) & 3, &["commNone", "commIndAndComment", "commIndicator"])?
    } else {
        enum_value((f >> 2) & 3, &["commNone", "commIndicator", "commIndAndComment"])?
    };
    attr(&mut xml, "showComments", comments);
    attr(&mut xml, "showObjects", enum_value((f >> if binary { 9 } else { 8 }) & 3, &["all", "placeholders", "none"])?);
    attr(&mut xml, "personalView", personal);
    attr(&mut xml, "minimized", minimized);
    if f & (1 << 13) != 0 {
        if !(5..=1440).contains(&interval) {
            return None;
        }
        attr(&mut xml, "mergeInterval", interval);
    }
    Some((id, xml))
}
// The XLSX writer only retains activeSheetId when the sheet owns that file id.
// Pin the translated model ids so later saves (and sheet deletion) use the existing rule.
pub(crate) fn pin_sheet_ids(wb: &mut gridcraft_model::Workbook) {
    for (index, sheet) in wb.sheets.iter_mut().enumerate() {
        Arc::make_mut(sheet).file_id = u32::try_from(index.saturating_add(1)).unwrap_or(1);
    }
}
pub(crate) fn finish_book(views: &[(u32, String)], into: &mut Option<Arc<Preserved>>, map: impl Fn(u32) -> usize) {
    let mut body = String::new();
    for (id, xml) in views {
        body.push_str(xml);
        attr(&mut body, "activeSheetId", map(*id).saturating_add(1));
        body.push_str("/>");
    }
    keep(into, "customWorkbookViews", body);
}

fn sheet(bytes: &[u8], binary: bool) -> Option<(String, u32)> {
    let mut d = Data(bytes);
    let guid = d.guid()?;
    d.take(4)?;
    let scale = d.u32()?;
    let color = if binary {
        d.u32()?
    } else {
        let v = u32::from(d.u16()?);
        d.take(6)?;
        v
    };
    let f = d.u32()?;
    let (row, col) = if binary {
        let r = d.u32()?;
        d.u32()?;
        let c = d.u32()?;
        d.u32()?;
        (r, c)
    } else {
        let r = u32::from(d.u16()?);
        d.u16()?;
        let c = u32::from(d.u16()?);
        d.u16()?;
        d.take(20)?;
        (r, c)
    };
    if !(10..=400).contains(&scale) || color > 64 {
        return None;
    }
    let mut xml = "<customSheetView".to_owned();
    attr(&mut xml, "guid", guid);
    attr(&mut xml, "scale", scale);
    attr(&mut xml, "colorId", color);
    for (key, shift) in [
        ("showPageBreaks", 0),
        ("showFormulas", 1),
        ("showGridLines", 2),
        ("showRowCol", 3),
        ("outlineSymbols", 4),
        ("fitToPage", 10),
        ("printArea", 11),
        ("filter", 13),
        ("showAutoFilter", 14),
        ("hiddenColumns", 21),
        ("filterUnique", 25),
        ("showRuler", 29),
    ] {
        bit(&mut xml, key, f, shift);
    }
    attr(&mut xml, "zeroValues", u8::from((f & 32 != 0) == binary));
    let hidden = (f >> 19) & 3;
    if hidden > 1 {
        return None;
    }
    attr(&mut xml, "hiddenRows", if binary { hidden } else { 1 - hidden });
    attr(&mut xml, "state", if binary { enum_value((f >> 22) & 3, &["visible", "hidden", "veryHidden"])? } else { "visible".into() });
    let view = (f >> 26) & 3;
    attr(&mut xml, "view", enum_value(view, &["normal", "pageBreakPreview", "pageLayout"])?);
    attr(&mut xml, "topLeftCell", cell(row, col, binary)?);
    xml.push('>');
    Some((xml, f))
}
fn pane_name(n: u32) -> Option<String> {
    enum_value(n, &["bottomRight", "topRight", "bottomLeft", "topLeft"])
}
fn pane(bytes: &[u8], binary: bool, view_flags: u32) -> Option<String> {
    let mut d = Data(bytes);
    let (x, y, row, col, active, f) = if binary {
        (d.f64()?, d.f64()?, d.u32()?, d.u32()?, d.u32()?, u32::from(d.u8()?))
    } else {
        let x = f64::from(d.u16()?);
        let y = f64::from(d.u16()?);
        let r = u32::from(d.u16()?);
        let c = u32::from(d.u16()?);
        let a = u32::from(d.u8()?);
        d.take(1)?;
        (x, y, r, c, a, ((view_flags >> 15) & 1) | (((view_flags >> 16) & 1) << 1))
    };
    let mut xml = "<pane".to_owned();
    attr(&mut xml, "xSplit", x);
    attr(&mut xml, "ySplit", y);
    attr(&mut xml, "topLeftCell", cell(row, col, binary)?);
    attr(&mut xml, "activePane", pane_name(active)?);
    attr(
        &mut xml,
        "state",
        if f & 2 != 0 {
            "frozen"
        } else if f & 1 != 0 {
            "frozenSplit"
        } else {
            "split"
        },
    );
    xml.push_str("/>");
    Some(xml)
}
fn selection(bytes: &[u8], binary: bool) -> Option<String> {
    let mut d = Data(bytes);
    let (pane, row, col, active, count) = if binary {
        (d.u32()?, d.u32()?, d.u32()?, d.u32()?, d.u32()?)
    } else {
        (u32::from(d.u8()?), u32::from(d.u16()?), u32::from(d.u16()?), u32::from(d.u16()?), u32::from(d.u16()?))
    };
    if count == 0 || count > if binary { 32767 } else { 1369 } || active >= count {
        return None;
    }
    let mut ranges = Vec::new();
    for _ in 0..count {
        let (r1, r2, c1, c2) = if binary {
            (d.u32()?, d.u32()?, d.u32()?, d.u32()?)
        } else {
            (u32::from(d.u16()?), u32::from(d.u16()?), u32::from(d.u8()?), u32::from(d.u8()?))
        };
        if r1 > r2 || c1 > c2 {
            return None;
        }
        let first = cell(r1, c1, binary)?;
        let last = cell(r2, c2, binary)?;
        ranges.push(if first == last { first } else { format!("{first}:{last}") });
    }
    let mut xml = "<selection".to_owned();
    attr(&mut xml, "pane", pane_name(pane)?);
    attr(&mut xml, "activeCell", cell(row, col, binary)?);
    attr(&mut xml, "activeCellId", active);
    attr(&mut xml, "sqref", ranges.join(" "));
    xml.push_str("/>");
    Some(xml)
}
#[derive(Default)]
pub(crate) struct SheetViews {
    current: Option<(String, u32)>,
    inside: bool,
    body: String,
    count: usize,
    pane: String,
    selections: String,
}
impl SheetViews {
    /// Consumes the entire block, including unsupported per-view print settings.
    pub(crate) fn record(&mut self, id: u16, bytes: &[u8], binary: bool, report: &mut ReadReport) -> bool {
        let (begin, end, pane_id, sel_id) = if binary { (423, 424, 151, 152) } else { (0x1aa, 0x1ab, 0x41, 0x1d) };
        if id == begin {
            if self.inside {
                warn(report);
            }
            self.inside = true;
            self.count = self.count.saturating_add(1);
            self.pane.clear();
            self.selections.clear();
            self.current = if self.count <= LIMIT { sheet(bytes, binary) } else { None };
            if self.current.is_none() {
                warn(report);
            }
            return true;
        }
        if !self.inside {
            return false;
        }
        if id == end {
            if bytes.len() != if binary { 0 } else { 2 } {
                self.current = None;
                warn(report);
            }
            if let Some((mut xml, _)) = self.current.take() {
                xml.push_str(&self.pane);
                xml.push_str(&self.selections);
                xml.push_str("</customSheetView>");
                if self.body.len().saturating_add(xml.len()) <= 4 * 1024 * 1024 {
                    self.body.push_str(&xml);
                } else {
                    warn(report);
                }
            }
            self.inside = false;
        } else if (id == pane_id || id == sel_id)
            && let Some((_, flags)) = self.current.as_ref()
        {
            let child = if self.selections.len().saturating_add(self.body.len()) > 4 * 1024 * 1024 {
                None
            } else if id == pane_id {
                pane(bytes, binary, *flags)
            } else {
                selection(bytes, binary)
            };
            match child {
                Some(xml) if id == pane_id => self.pane = xml,
                Some(xml) => self.selections.push_str(&xml),
                None => {
                    self.current = None;
                    warn(report);
                }
            }
        }
        true
    }
    pub(crate) fn finish(self, into: &mut Option<Arc<Preserved>>, report: &mut ReadReport) {
        if self.inside {
            warn(report);
        }
        keep(into, "customSheetViews", self.body);
    }
}
