//! XLSB sparklines: the Excel 2010 sparkline group collection of a worksheet's future-record
//! block ([MS-XLSB] 2.1.7.62 SPARKLINEGROUPS), mapped to the same `Sparkline` values the `.xlsx`
//! reader builds from `x14:sparklineGroups`. The type, the series colour, the markers flag and the
//! grouping are read; the group's other colours, axis and empty-cell options keep their defaults.

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS};
use gridcraft_model::{Color, Sheet, Sparkline, SparklineGroup, SparklineKind};

use super::{Bytes, FormulaBytes, invalid, styles};
use crate::IoError;
use crate::ReadReport;
use crate::xls::formula::{Links, Mode, decode_xlsb};

/// BrtBeginSparklineGroup, BrtEndSparklineGroup, BrtSparkline ([MS-XLSB] 2.3).
const BEGIN_GROUP: u16 = 1041;
const END_GROUP: u16 = 1042;
const SPARKLINE: u16 = 1043;
/// BrtBeginSparklines, BrtEndSparklines, BrtBeginSparklineGroups, BrtEndSparklineGroups.
const COLLECTIONS: [u16; 4] = [1056, 1057, 1058, 1059];
/// Most sparklines imported for one sheet.
const MAX_SPARKLINES: usize = 100_000;

/// Whether `id` is a sparkline record, read even inside a future-record block.
pub(super) fn is_record(id: u16) -> bool {
    matches!(id, BEGIN_GROUP | END_GROUP | SPARKLINE) || COLLECTIONS.contains(&id)
}

/// The settings of the open group that the model keeps for each of its sparklines.
#[derive(Clone, Copy)]
struct Group {
    kind: SparklineKind,
    color: Color,
    markers: bool,
    /// Numbered from 1 per sheet, so the group's sparklines are saved as one group again.
    id: u32,
}

/// Sparkline reading state for one worksheet.
#[derive(Default)]
pub(super) struct Sparklines {
    group: Option<Group>,
    groups: u32,
    warned: bool,
}
impl Sparklines {
    /// Handles one sparkline record. A damaged group or sparkline is skipped with at most one
    /// warning per sheet; it never aborts the import.
    pub(super) fn record(&mut self, id: u16, payload: &[u8], links: &Links, sheet: &mut Sheet, report: &mut ReadReport) {
        let ok = match id {
            BEGIN_GROUP => {
                self.groups = self.groups.saturating_add(1);
                self.group = group(payload, self.groups).ok();
                self.group.is_some()
            }
            END_GROUP => {
                // A group of one needs no id, as in the .xlsx reader.
                if let Some(g) = self.group.take() {
                    let mut members = sheet.sparklines.iter_mut().rev().take_while(|sp| sp.group.id == g.id);
                    if let (Some(only), None) = (members.next(), members.next()) {
                        only.group.id = 0;
                    }
                }
                true
            }
            // The sparklines of a group that couldn't be read were already warned about.
            SPARKLINE => match self.group {
                Some(_) if sheet.sparklines.len() >= MAX_SPARKLINES => false,
                Some(g) => match sparkline(payload, links) {
                    Ok((cell, source)) => {
                        let group = SparklineGroup { id: g.id, ..SparklineGroup::default() };
                        sheet.sparklines.push(Sparkline { cell, source, kind: g.kind, color: g.color, markers: g.markers, group });
                        true
                    }
                    Err(_) => false,
                },
                None => true,
            },
            _ => true,
        };
        if !ok && !self.warned {
            self.warned = true;
            report.warn("Some XLSB sparklines could not be read and were skipped.");
        }
    }
}

/// FRTHeader ([MS-XLSB] 2.5.61): the flags, then the references, ranges and formulas they
/// announce. Returns the cell of the first range and the first formula's tokens.
fn frt_header<'a>(b: &mut Bytes<'a>) -> Result<(Option<CellRef>, Option<FormulaBytes<'a>>), IoError> {
    let flags = b.u32()?;
    // FRTRefs and FRTRelID never appear in sparkline records.
    if flags & 0b1001 != 0 {
        return Err(invalid("unexpected sparkline references"));
    }
    let mut cell = None;
    if flags & 0b10 != 0 {
        // FRTSqrefs: csqref, then FRTSqref (flags, UncheckedSqRfX: crfx, UncheckedRfX each).
        let count = b.u32()?;
        for _ in 0..count {
            b.u32()?;
            let crfx = b.u32()?;
            let n = if crfx == u32::MAX { 0 } else { usize::try_from(crfx).map_err(|_| invalid("sparkline range count"))? };
            for _ in 0..n {
                let (r0, _, c0, _) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
                if cell.is_none() && r0 < MAX_ROWS && c0 < MAX_COLS {
                    cell = Some(CellRef::new(r0, c0));
                }
            }
        }
    }
    let mut formula = None;
    if flags & 0b100 != 0 {
        // FRTFormulas: cformula, then FRTFormula (flags, FRTParsedFormula: cce, cb, rgce, rgcb).
        let count = b.u32()?;
        for _ in 0..count {
            b.u32()?;
            let cce = b.u32()?;
            if cce > 16_384 {
                return Err(invalid("sparkline formula too long"));
            }
            let cb = b.u32()?;
            let rgce = b.take(cce as usize)?;
            let rgcb = b.take(usize::try_from(cb).map_err(|_| invalid("sparkline formula data too long"))?)?;
            formula.get_or_insert((rgce, rgcb));
        }
    }
    Ok((cell, formula))
}

/// BrtBeginSparklineGroup ([MS-XLSB] 2.4.228).
fn group(payload: &[u8], id: u32) -> Result<Group, IoError> {
    let mut b = Bytes::new(payload);
    frt_header(&mut b)?;
    let flags = b.u16()?;
    let color = styles::color(&mut b)?;
    // Negative, axis, markers, first, last, high and low colours; manual maximum and minimum
    // and the line weight.
    b.take(7 * 8 + 3 * 8)?;
    let kind = match b.u32()? {
        0 => SparklineKind::Line,
        1 => SparklineKind::Column,
        2 => SparklineKind::WinLoss,
        _ => return Err(invalid("unknown sparkline type")),
    };
    Ok(Group { kind, color, markers: flags & 0x0008 != 0, id })
}

/// BrtSparkline ([MS-XLSB] 2.4.806): the sparkline's cell and the A1 text of its data range.
fn sparkline(payload: &[u8], links: &Links) -> Result<(CellRef, String), IoError> {
    let mut b = Bytes::new(payload);
    let (cell, formula) = frt_header(&mut b)?;
    let cell = cell.ok_or_else(|| invalid("sparkline without a cell"))?;
    let source = match formula {
        Some((rgce, rgcb)) => {
            let e = decode_xlsb(rgce, rgcb, cell, Mode::Cell, links).map_err(|_| invalid("unreadable sparkline data range"))?;
            gridcraft_formula::print(&e)
        }
        None => String::new(),
    };
    Ok((cell, source))
}
