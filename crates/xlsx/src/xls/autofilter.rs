//! Worksheet AutoFilter from [MS-XLS]: AutoFilterInfo (2.4.8), AutoFilter (2.4.6) with its
//! AFDOper conditions (2.5.5) and the `_FilterDatabase` built-in name (2.4.150) that gives the
//! filtered range.
//!
//! Excel 2007+ also writes AutoFilter12 (2.4.7) for a column that filters on more than two
//! values, by cell or font colour, by icon, or with a dynamic filter. Its colour (DXFN12NoCB,
//! 2.5.98) or icon (AF12CellIcon, 2.5.2) is in the record itself; a value list's AF12Criteria
//! (2.5.3) and AF12DateInfo (2.5.4) items each follow in their own ContinueFrt12 (2.4.62).
//! AutoFilter12 is the full criterion: it replaces the AutoFilter record Excel writes beside it
//! as an approximation for older versions. Date filters are skipped, as the xlsx reader does.

use gridcraft_core::{CellRef, RangeRef, number_to_text};
use gridcraft_formula::Expr;
use gridcraft_model::{AutoFilter, Color, FilterCriterion};

use super::{Cursor, Links, Mode, SheetState, decode};
use crate::{IoError, ReadReport};

/// Conditions the model can't hold: Boolean or error values, unknown filter or icon types.
const UNSUPPORTED: &str = "Some .xls AutoFilter criteria (Boolean or error values, or unknown filter types) were not imported.";
/// A damaged AutoFilter record: skipped, the other columns keep theirs.
const MALFORMED: &str = "Some .xls AutoFilter criteria could not be read and were skipped.";
/// AutoFilterInfo without the name Excel must write beside it.
const NO_RANGE: &str = "An .xls sheet has an AutoFilter but no _FilterDatabase range; its filter was skipped.";
/// Dynamic date filters and date groupings, as the xlsx reader words it.
const DATES: &str = "date filters in AutoFilters are not supported";

/// BIFF8 columns at most: the number of criteria one sheet can hold.
const MAX_CRITERIA: usize = 256;

/// AutoFilter12's fields before `rgb`: frtRefHeader (12 bytes) up to guidSview (16 bytes).
const FILTER12_HEADER: usize = 60;
/// ContinueFrt12's frtRefHeader, before its data.
const FRT_REF_HEADER: usize = 12;

/// Why one column's criterion was not imported.
#[derive(Clone, Copy)]
enum Skipped {
    /// A condition the model has no field for (a Boolean or error value, or an unknown type).
    Unsupported,
    /// A record shorter than its contents.
    Malformed,
    /// A dynamic date filter or date grouping.
    Date,
}

impl From<IoError> for Skipped {
    fn from(_: IoError) -> Self {
        Skipped::Malformed
    }
}

impl Skipped {
    fn warn(self, report: &mut ReadReport) {
        report.warn(match self {
            Skipped::Unsupported => UNSUPPORTED,
            Skipped::Malformed => MALFORMED,
            Skipped::Date => DATES,
        });
    }
}

/// An AutoFilter12 value list waiting for the ContinueFrt12 records that carry its items.
struct ValueList {
    entry: u32,
    /// AF12Criteria still to come, then AF12DateInfo.
    criteria: u32,
    dates: u32,
    has_dates: bool,
    values: Vec<String>,
    blanks: bool,
    /// The first item that could not be read: the column keeps no AutoFilter12 criterion.
    error: Option<Skipped>,
}

impl ValueList {
    /// One AF12Criteria (2.5.3): an AFDOper, then its string when `vt` is 0x06.
    fn item(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), Skipped> {
        let mut oper = Oper::read(c)?;
        if oper.vt == 0x06 {
            let cch = oper.cch();
            oper.text = Some(string(c, cch)?);
        }
        // "Equals empty" is the list's (Blanks) item.
        if oper.vt == 0x0C {
            self.blanks = true;
            return Ok(());
        }
        match oper.comparison()? {
            (op, value) if op == "=" && oper.vt != 0x0E => self.values.push(value),
            _ => return Err(Skipped::Unsupported),
        }
        Ok(())
    }

    /// The next item from `c`, which is past any header.
    fn next(&mut self, c: &mut Cursor<'_, '_>) {
        let result = if self.criteria > 0 {
            self.criteria -= 1;
            self.item(c)
        } else if self.dates > 0 {
            // AF12DateInfo: a date grouping, which the model has no field for.
            self.dates -= 1;
            Ok(())
        } else {
            return;
        };
        if let Err(e) = result {
            self.error.get_or_insert(e);
        }
    }

    fn outcome(self) -> (u32, Result<Option<FilterCriterion>, Skipped>) {
        let result = match self.error {
            Some(e) => Err(e),
            None if self.criteria > 0 || self.dates > 0 => Err(Skipped::Malformed),
            // Only date groupings: a date filter.
            None if self.values.is_empty() && !self.blanks && self.has_dates => Err(Skipped::Date),
            None if self.values.is_empty() && !self.blanks => Ok(None),
            None => Ok(Some(FilterCriterion::Values { values: self.values, blanks: self.blanks })),
        };
        (self.entry, result)
    }
}

/// What a sheet substream's AutoFilter records said, until the substream ends.
#[derive(Default)]
pub(super) struct State {
    /// AutoFilterInfo was read: the sheet has an AutoFilter.
    present: bool,
    /// From AutoFilter records.
    criteria: Vec<(u32, FilterCriterion)>,
    /// From AutoFilter12 records, which replace those for the same column.
    criteria12: Vec<(u32, FilterCriterion)>,
    /// Columns whose AutoFilter12 was read (with or without a criterion the model holds).
    columns12: Vec<u32>,
    pending: Option<ValueList>,
}

impl State {
    /// AutoFilterInfo (2.4.8): cEntries is the number of drop-down columns; the columns
    /// themselves carry the criteria.
    pub(super) fn info(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>, report: &mut ReadReport) {
        match c.u16() {
            Ok(_entries) => st.autofilter.present = true,
            Err(_) => report.warn(MALFORMED),
        }
    }

    /// AutoFilter (2.4.6): one column's criterion.
    pub(super) fn criterion(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>, report: &mut ReadReport) {
        match parse(c) {
            Ok(Some((entry, crit))) if st.autofilter.criteria.len() < MAX_CRITERIA => st.autofilter.criteria.push((entry, crit)),
            Ok(_) => {}
            Err(e) => e.warn(report),
        }
    }

    /// Called for every record: a value list's items are the ContinueFrt12 records right after
    /// its AutoFilter12.
    pub(super) fn next_record(&mut self, id: u16, report: &mut ReadReport) {
        if id != 0x087F
            && let Some(list) = self.pending.take()
        {
            let (entry, result) = list.outcome();
            self.settle(entry, result, report);
        }
    }

    /// AutoFilter12 (2.4.7): one column's Excel 2007 criterion.
    pub(super) fn filter12(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>, report: &mut ReadReport) {
        let table = st.g.styles.color_table();
        match parse12(c, &table) {
            Ok((entry, Filter12::Done(result))) => st.autofilter.settle(entry, result, report),
            Ok((_, Filter12::List(list))) => st.autofilter.pending = Some(list),
            Err(e) => e.warn(report),
        }
    }

    /// ContinueFrt12 (2.4.62): after AutoFilter12, one AF12Criteria or AF12DateInfo.
    pub(super) fn continue_frt12(c: &mut Cursor<'_, '_>, st: &mut SheetState<'_>) {
        let Some(list) = st.autofilter.pending.as_mut() else { return };
        if c.skip(FRT_REF_HEADER).is_err() {
            list.error.get_or_insert(Skipped::Malformed);
            return;
        }
        list.next(c);
    }

    /// Keeps a read AutoFilter12 outcome. A column that could not be read keeps the
    /// AutoFilter record's approximation, if there is one.
    fn settle(&mut self, entry: u32, result: Result<Option<FilterCriterion>, Skipped>, report: &mut ReadReport) {
        match result {
            Ok(crit) => {
                if self.columns12.len() >= MAX_CRITERIA {
                    return;
                }
                self.columns12.push(entry);
                if let Some(crit) = crit {
                    self.criteria12.retain(|(e, _)| *e != entry);
                    self.criteria12.push((entry, crit));
                }
            }
            Err(e) => e.warn(report),
        }
    }

    /// The sheet's AutoFilter, once its substream has ended.
    pub(super) fn finish(mut self, range: Option<RangeRef>, report: &mut ReadReport) -> Option<AutoFilter> {
        self.next_record(0, report);
        if !self.present {
            // The name without AutoFilterInfo is what an advanced filter leaves behind.
            return None;
        }
        let Some(range) = range else {
            report.warn(NO_RANGE);
            return None;
        };
        let columns12 = self.columns12;
        let criteria = self
            .criteria
            .into_iter()
            .filter(|(entry, _)| !columns12.contains(entry))
            .chain(self.criteria12)
            .filter(|(entry, _)| *entry < range.width())
            .collect();
        Some(AutoFilter { range, criteria })
    }
}

/// What an AutoFilter12 record gives: its criterion, or a value list whose items follow.
enum Filter12 {
    Done(Result<Option<FilterCriterion>, Skipped>),
    List(ValueList),
}

/// AutoFilter12 (2.4.7): `iEntry`, `ft` (0 criteria, 1 cell colour, 2 font colour, 3 icon),
/// `cft` (the dynamic filter type when nonzero) and the item counts, then `rgb` for `ft` 1-3.
fn parse12(c: &mut Cursor<'_, '_>, table: &[u32; 64]) -> Result<(u32, Filter12), Skipped> {
    if c.remaining() < FILTER12_HEADER {
        return Err(Skipped::Malformed);
    }
    c.skip(FRT_REF_HEADER)?;
    let entry = u32::from(c.u16()?);
    let _hide_arrow = c.u32()?;
    let ft = c.u32()?;
    let cft = c.u32()?;
    let criteria = c.u32()?;
    let dates = c.u32()?;
    // fWorksheetAutoFilter and unused bits, unused2, idList, guidSview.
    c.skip(2 + 4 + 4 + 16)?;
    let done = |r| Ok((entry, Filter12::Done(r)));
    match ft {
        0 => match cft {
            0 => {
                let mut list = ValueList { entry, criteria, dates, has_dates: dates > 0, values: vec![], blanks: false, error: None };
                // Each item belongs in its own ContinueFrt12; read any written in the record itself.
                while c.remaining() > 0 && (list.criteria > 0 || list.dates > 0) {
                    list.next(c);
                }
                Ok((entry, Filter12::List(list)))
            }
            1 => done(Ok(Some(FilterCriterion::AboveAverage(true)))),
            2 => done(Ok(Some(FilterCriterion::AboveAverage(false)))),
            0x08..=0x27 => done(Err(Skipped::Date)),
            _ => done(Err(Skipped::Unsupported)),
        },
        1 => done(Ok(Some(FilterCriterion::FillColor(dxf_color(c, false, table)?)))),
        2 => done(Ok(Some(FilterCriterion::FontColor(dxf_color(c, true, table)?)))),
        3 => {
            // AF12CellIcon (2.5.2): iIconSet from KPISets, iIcon (0xFFFFFFFF: no icon).
            let (set, icon) = (c.u32()?, c.u32()?);
            let set = crate::xlsb::icon_set(set).ok_or(Skipped::Unsupported)?;
            done(Ok(Some(FilterCriterion::Icon { set: set.to_string(), id: (icon != u32::MAX).then_some(icon) })))
        }
        _ => done(Err(Skipped::Unsupported)),
    }
}

/// The fill (`font` false) or font colour of a DXFN12NoCB (2.5.98): a DXFN (2.5.70), then an
/// optional XFExtNoFRT (2.5.283) whose FullColorExt, when present, is the exact colour.
fn dxf_color(c: &mut Cursor<'_, '_>, font: bool, table: &[u32; 64]) -> Result<Color, Skipped> {
    let indexed = |icv: u32| usize::try_from(icv).ok().and_then(|i| table.get(i)).map_or(Color::Auto, |&rgb| Color::Rgb(rgb));
    let flags = c.u32()?;
    let flags2 = c.u16()?;
    let has = |bit: u32| flags & (1 << bit) != 0;
    let mut color = Color::Auto;
    if has(25) {
        if flags2 & 1 != 0 {
            // DXFNumUsr: cb, then an XLUnicodeString.
            c.u16()?;
            let cch = usize::from(c.u16()?);
            let high = c.u8()? & 1 != 0;
            c.skip(if high { cch.saturating_mul(2) } else { cch })?;
        } else {
            c.skip(2)?;
        }
    }
    if has(26) {
        // DXFFntD (2.5.84), 118 bytes: icvFore is at offset 80.
        c.skip(80)?;
        let icv = c.u32()?;
        c.skip(34)?;
        if font {
            color = indexed(icv);
        }
    }
    if has(27) {
        c.skip(8)?;
    }
    if has(28) {
        c.skip(8)?;
    }
    if has(29) {
        // DXFPat: fls, then icvForeground (7 bits) and icvBackground (7 bits). A solid
        // differential fill's colour is its background colour.
        c.u16()?;
        let icvs = c.u16()?;
        let (fg, bg) = (u32::from(icvs & 0x7F), u32::from((icvs >> 7) & 0x7F));
        if !font {
            let bg = if has(18) { Color::Auto } else { indexed(bg) };
            let fg = if has(17) { Color::Auto } else { indexed(fg) };
            color = if bg != Color::Auto { bg } else { fg };
        }
    }
    if has(30) {
        c.skip(2)?;
    }
    // XFExtNoFRT: three reserved words and cexts, then ExtProps (2.5.108): extType, cb (the
    // whole ExtProp) and data. 0x04/0x05 are the fill's fore/background, 0x0D the text colour.
    if c.remaining() >= 8 {
        c.skip(6)?;
        let cexts = c.u16()?;
        let mut fill: [Option<Color>; 2] = [None, None];
        for _ in 0..cexts {
            let ext_type = c.u16()?;
            let cb = usize::from(c.u16()?);
            let data = cb.checked_sub(4).ok_or(Skipped::Malformed)?;
            let slot = match ext_type {
                0x0D if font => Some(0),
                0x05 if !font => Some(0),
                0x04 if !font => Some(1),
                _ => None,
            };
            match slot {
                Some(slot) if data >= 8 => {
                    let ext = full_color(c, table)?;
                    c.skip(data - 8)?;
                    if let Some(s) = fill.get_mut(slot) {
                        *s = ext;
                    }
                }
                _ => c.skip(data)?,
            }
        }
        if let Some(ext) = fill.into_iter().flatten().find(|c| *c != Color::Auto) {
            color = ext;
        }
    }
    Ok(color)
}

/// FullColorExt (2.5.155): xclrType (0 automatic, 1 indexed, 2 RGB, 3 theme, 4 not set),
/// nTintShade and xclrValue; `None` when no colour is set.
fn full_color(c: &mut Cursor<'_, '_>, table: &[u32; 64]) -> Result<Option<Color>, Skipped> {
    let ty = c.u16()?;
    let tint = c.u16()? as i16;
    let value = c.u32()?;
    Ok(match ty {
        1 => usize::try_from(value).ok().and_then(|i| table.get(i)).map(|&rgb| Color::Rgb(rgb)),
        2 => {
            // LongRGBA: red, green, blue, reserved.
            let [r, g, b, _] = value.to_le_bytes();
            Some(Color::rgb(r, g, b))
        }
        3 => u8::try_from(value).ok().filter(|v| *v <= 11).map(|v| Color::Theme(v, (i32::from(tint) * 1000 / 32767) as i16)),
        _ => None,
    })
}

/// The range of a sheet's `_FilterDatabase` Lbl ([MS-XLS] 2.4.150): its rgce is a single
/// PtgArea3d (2.5.198.28) or PtgRef3d on that sheet.
pub(super) fn filter_database(rgce: &[u8], rgcb: &[u8], links: &Links) -> Option<RangeRef> {
    match decode(rgce, rgcb, CellRef::new(0, 0), Mode::Name, links).ok()? {
        Expr::Ref(r) => Some(r.range()),
        _ => None,
    }
}

/// An AFDOper ([MS-XLS] 2.5.5): the comparison type, its sign and eight value bytes.
struct Oper {
    vt: u8,
    sign: u8,
    value: [u8; 8],
    /// The AFDOperStr's string, which follows the AutoFilter record's two AFDOpers.
    text: Option<String>,
}

impl Oper {
    fn read(c: &mut Cursor<'_, '_>) -> Result<Self, IoError> {
        let vt = c.u8()?;
        let sign = c.u8()?;
        let mut value = [0u8; 8];
        for b in value.iter_mut() {
            *b = c.u8()?;
        }
        Ok(Oper { vt, sign, value, text: None })
    }

    /// The character count of the AFDOperStr (2.5.8): cch follows four unused bytes.
    fn cch(&self) -> usize {
        usize::from(self.value.get(4).copied().unwrap_or(0))
    }

    /// One condition as the operator and value the xlsx reader produces for a `customFilter`.
    fn comparison(&self) -> Result<(String, String), Skipped> {
        // All non-blanks: the xlsx reader's `notEqual` with a space.
        if self.vt == 0x0E {
            return Ok(("<>".into(), " ".into()));
        }
        let op = match self.sign {
            1 => "<",
            2 => "=",
            3 => "<=",
            4 => ">",
            5 => "<>",
            6 => ">=",
            _ => return Err(Skipped::Unsupported),
        };
        let value = match self.vt {
            // An RK number (2.5.217).
            0x02 => {
                let mut bytes = [0u8; 4];
                for (b, s) in bytes.iter_mut().zip(self.value) {
                    *b = s;
                }
                number_to_text(super::rk(u32::from_le_bytes(bytes)).as_f64().unwrap_or(0.0))
            }
            // An Xnum (2.5.342).
            0x04 => number_to_text(f64::from_le_bytes(self.value)),
            0x06 => self.text.clone().ok_or(Skipped::Malformed)?,
            0x0C => String::new(),
            _ => return Err(Skipped::Unsupported),
        };
        Ok((op.to_string(), value))
    }
}

/// AutoFilter (2.4.6): the column offset within the range and its criterion.
fn parse(c: &mut Cursor<'_, '_>) -> Result<Option<(u32, FilterCriterion)>, Skipped> {
    let entry = u32::from(c.u16()?);
    let flags = c.u16()?;
    let mut first = Oper::read(c)?;
    let mut second = Oper::read(c)?;
    // str1 and str2 are the strings of the conditions with vt = 0x06, in order.
    for oper in [&mut first, &mut second] {
        if oper.vt == 0x06 {
            let cch = oper.cch();
            oper.text = Some(string(c, cch)?);
        }
    }
    // fTopN; then fTop (0 selects the bottom items), fPercent and the 9-bit wTopN.
    if flags & 0x0010 != 0 {
        let count = u32::from(flags >> 7);
        return Ok(Some((entry, FilterCriterion::Top10 { bottom: flags & 0x0020 == 0, percent: flags & 0x0040 != 0, count })));
    }
    // A single "all blanks" condition is the xlsx reader's `<filters blank="1"/>`.
    if first.vt == 0x0C && second.vt == 0 {
        return Ok(Some((entry, FilterCriterion::Values { values: vec![], blanks: true })));
    }
    let and = flags & 0x0003 == 0 && second.vt != 0;
    if first.vt == 0 {
        return Ok(None);
    }
    let a = first.comparison()?;
    let b = if second.vt == 0 { None } else { Some(second.comparison()?) };
    Ok(Some((entry, FilterCriterion::Custom { a, b, and })))
}

/// XLUnicodeStringNoCch ([MS-XLS] 2.5.296): the compression flag, then `cch` characters.
fn string(c: &mut Cursor<'_, '_>, cch: usize) -> Result<String, IoError> {
    let high = c.u8()? & 1 != 0;
    c.chars(cch, high)
}
