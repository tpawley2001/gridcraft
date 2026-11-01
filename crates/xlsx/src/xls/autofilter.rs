//! Worksheet AutoFilter from [MS-XLS]: AutoFilterInfo (2.4.8), AutoFilter (2.4.6) with its
//! AFDOper conditions (2.5.5) and the `_FilterDatabase` built-in name (2.4.150) that gives the
//! filtered range.

use gridcraft_core::{CellRef, RangeRef, number_to_text};
use gridcraft_formula::Expr;
use gridcraft_model::{AutoFilter, FilterCriterion};

use super::{Cursor, Links, Mode, SheetState, decode};
use crate::{IoError, ReadReport};

/// Excel 2007+ filters (value lists, colour, dynamic) and conditions the model can't hold.
const UNSUPPORTED: &str = "Some .xls AutoFilter criteria (Excel 2007 value lists or colour filters) were not imported.";
/// A damaged AutoFilter record: skipped, the other columns keep theirs.
const MALFORMED: &str = "Some .xls AutoFilter criteria could not be read and were skipped.";
/// AutoFilterInfo without the name Excel must write beside it.
const NO_RANGE: &str = "An .xls sheet has an AutoFilter but no _FilterDatabase range; its filter was skipped.";

/// BIFF8 columns at most: the number of criteria one sheet can hold.
const MAX_CRITERIA: usize = 256;

/// Why one column's criterion was not imported.
enum Skipped {
    /// A condition the model has no field for (a Boolean or error value, or an unknown type).
    Unsupported,
    /// A record shorter than its contents.
    Malformed,
}

impl From<IoError> for Skipped {
    fn from(_: IoError) -> Self {
        Skipped::Malformed
    }
}

/// What a sheet substream's AutoFilter records said, until the substream ends.
#[derive(Default)]
pub(super) struct State {
    /// AutoFilterInfo was read: the sheet has an AutoFilter.
    present: bool,
    criteria: Vec<(u32, FilterCriterion)>,
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
            Err(Skipped::Unsupported) => report.warn(UNSUPPORTED),
            Err(Skipped::Malformed) => report.warn(MALFORMED),
        }
    }

    /// AutoFilter12 (2.4.7): value lists, colour and dynamic filters, which the model has no
    /// field for; that column keeps no criterion.
    pub(super) fn filter12(report: &mut ReadReport) {
        report.warn(UNSUPPORTED);
    }

    /// The sheet's AutoFilter, once its substream has ended.
    pub(super) fn finish(self, range: Option<RangeRef>, report: &mut ReadReport) -> Option<AutoFilter> {
        if !self.present {
            // The name without AutoFilterInfo is what an advanced filter leaves behind.
            return None;
        }
        let Some(range) = range else {
            report.warn(NO_RANGE);
            return None;
        };
        let criteria = self.criteria.into_iter().filter(|(entry, _)| *entry < range.width()).collect();
        Some(AutoFilter { range, criteria })
    }
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
