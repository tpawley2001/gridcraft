//! Conditional formatting in `.xls` sheets: a CondFmt record ([MS-XLS] 2.4.56) names the ranges,
//! and the CF records ([MS-XLS] 2.4.42) that follow it hold one rule each, with the format it
//! applies as a DXFN ([MS-XLS] 2.5.70). Only cell-value and formula rules exist in BIFF8; the
//! Excel 2007 kinds (CondFmt12 / CF12 / CFEx) are read in [`cf12`].

mod cf12;

use std::collections::HashMap;

use gridcraft_core::color::INDEXED;
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{
    BorderLine, BorderStyle, CfOperator, CfRule, Color, CondFormat, Fill, NumFmt, PatternType, Sheet, Style, Underline, VertAlign,
};

use super::formula::{Links, Mode, decode};
use crate::ReadReport;
use crate::tables::builtin_format;

/// Most ranges one CondFmt applies to.
const MAX_RANGES: usize = 8192;
/// Most rules kept per sheet, and most ranges kept over all of them.
const MAX_RULES: usize = 16_384;
const MAX_KEPT_RANGES: usize = 1_000_000;

/// The CondFmt / CondFmt12 records read so far in a sheet.
struct Group {
    ranges: Vec<RangeRef>,
    /// Index in `Sheet::cond_formats` of each classic CF record that followed (`None`: skipped).
    rules: Vec<Option<usize>>,
}

/// Conditional formatting state while a sheet's records are read.
#[derive(Default)]
pub(super) struct CondState {
    /// The `nID` and ranges of the header whose rule records are being read.
    current: Option<(u16, Vec<RangeRef>)>,
    groups: HashMap<u16, Group>,
    kept_ranges: usize,
    /// Whether a classic rule was skipped (warned once per sheet).
    skipped: bool,
    /// Why Excel 2007+ rules were skipped or read in part (each reason once).
    skipped12: Vec<&'static str>,
    capped: bool,
}

impl CondState {
    /// Called for every record: a header's rules are the records right after it.
    pub(super) fn next_record(&mut self, id: u16) {
        if !matches!(id, 0x01B1 | 0x087A | 0x087B) {
            self.current = None;
        }
    }

    /// CondFmt (0x01B0), or CondFmt12 (0x0879) after its 12-byte header.
    pub(super) fn header(&mut self, bytes: &[u8], frt: bool) {
        let body = if frt { bytes.get(12..).unwrap_or(&[]) } else { bytes };
        let id = body.get(2..4).and_then(|b| b.try_into().ok()).map_or(0, |b| u16::from_le_bytes(b) >> 1);
        self.current = cond_fmt(body).map(|ranges| (id, ranges));
        match &self.current {
            Some((id, ranges)) => {
                self.groups.insert(*id, Group { ranges: ranges.clone(), rules: vec![] });
            }
            None if frt => self.skip12("damaged records"),
            None => self.skipped = true,
        }
    }

    fn skip12(&mut self, why: &'static str) {
        if !self.skipped12.contains(&why) {
            self.skipped12.push(why);
        }
    }

    /// Adds a rule unless the sheet already holds too many; returns its index.
    fn push(&mut self, sheet: &mut Sheet, ranges: &[RangeRef], rule: CfRule, priority: Option<u32>, stop_if_true: bool) -> Option<usize> {
        if sheet.cond_formats.len() >= MAX_RULES || self.kept_ranges.saturating_add(ranges.len()) > MAX_KEPT_RANGES {
            self.capped = true;
            return None;
        }
        self.kept_ranges += ranges.len();
        let priority = priority.unwrap_or_else(|| u32::try_from(sheet.cond_formats.len()).unwrap_or(u32::MAX).saturating_add(1));
        sheet.cond_formats.push(CondFormat { ranges: ranges.to_vec(), rule, priority, stop_if_true });
        Some(sheet.cond_formats.len() - 1)
    }

    /// CF (0x01B1).
    pub(super) fn cf(&mut self, bytes: &[u8], links: &Links, sheet: &mut Sheet) {
        let Some((id, ranges)) = self.current.take() else { return };
        let base = ranges.first().map_or(CellRef::new(0, 0), |r| r.start);
        let index = match cf(bytes, base, links) {
            Ok(rule) => self.push(sheet, &ranges, rule, None, false),
            Err(_) => {
                self.skipped = true;
                None
            }
        };
        if let Some(g) = self.groups.get_mut(&id) {
            g.rules.push(index);
        }
        self.current = Some((id, ranges));
    }

    /// CF12 (0x087A).
    pub(super) fn cf12(&mut self, bytes: &[u8], links: &Links, sheet: &mut Sheet) {
        let Some((id, ranges)) = self.current.take() else { return };
        let base = ranges.first().map_or(CellRef::new(0, 0), |r| r.start);
        let mut notes = vec![];
        match cf12::cf12(bytes, base, links, &mut notes) {
            Ok(r) => {
                self.push(sheet, &ranges, r.rule, Some(r.priority), r.stop_if_true);
            }
            Err("truncated") => self.skip12("damaged records"),
            Err(why) => self.skip12(why),
        }
        for why in notes {
            self.skip12(why);
        }
        self.current = Some((id, ranges));
    }

    /// CFEx (0x087B): extends a classic CF rule, or announces the CF12 rule that follows.
    pub(super) fn cf_ex(&mut self, bytes: &[u8], sheet: &mut Sheet) {
        let Some(ex) = cf12::cf_ex(bytes) else {
            self.skip12("damaged records");
            return;
        };
        let Some(group) = self.groups.get(&ex.id) else {
            self.skip12("extensions of a missing rule");
            return;
        };
        let Some(ext) = ex.ext else {
            self.current = Some((ex.id, group.ranges.clone()));
            return;
        };
        let Some(cf) = group.rules.get(ext.index).copied().flatten().and_then(|i| sheet.cond_formats.get_mut(i)) else { return };
        cf.priority = ext.priority;
        cf.stop_if_true = ext.stop_if_true;
        if let CfRule::Expression { formula, style } = &cf.rule {
            let style = Box::new(ext.style.unwrap_or_else(|| (**style).clone()));
            if let Some(rule) = cf12::template_rule(ext.template, &ext.params, Some(formula), style) {
                cf.rule = rule;
            }
        }
    }

    /// Warns about what was skipped in this sheet.
    pub(super) fn finish(&self, report: &mut ReadReport) {
        if self.skipped {
            report.warn("Some conditional formatting rules in this .xls sheet could not be read and were skipped.");
        }
        for why in &self.skipped12 {
            report.warn(format!("Some Excel 2007+ conditional formats in this .xls sheet were skipped or read in part ({why})."));
        }
        if self.capped {
            report.warn(format!("Conditional formats past {MAX_RULES} rules in this .xls sheet were skipped."));
        }
    }
}

/// A little-endian reader over a record's bytes that fails softly at the end.
struct Bytes<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.b.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).and_then(|s| s.first().copied())
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).and_then(|s| s.try_into().ok()).map(u16::from_le_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes)
    }
    fn f64(&mut self) -> Option<f64> {
        self.take(8).and_then(|s| s.try_into().ok()).map(f64::from_le_bytes)
    }
}

/// The ranges of a CondFmt record (its `sqref`); `None` if it is malformed or names none.
fn cond_fmt(bytes: &[u8]) -> Option<Vec<RangeRef>> {
    let mut t = Bytes { b: bytes, pos: 0 };
    // ccf, fToughRecalc/nID and the bounding range are not needed.
    t.take(12)?;
    let n = usize::from(t.u16()?);
    let mut ranges = vec![];
    for _ in 0..n.min(MAX_RANGES) {
        let Some((r0, r1, c0, c1)) = (|| Some((t.u16()?, t.u16()?, t.u16()?, t.u16()?)))() else { break };
        if r1 < r0 || c1 < c0 || c1 >= 256 {
            continue;
        }
        ranges.push(RangeRef::new(CellRef::new(u32::from(r0), u32::from(c0)), CellRef::new(u32::from(r1), u32::from(c1))));
    }
    (!ranges.is_empty()).then_some(ranges)
}

/// One CF record as a rule, its formulas relative to `base` (the first range's top-left cell).
/// The error says why the rule was skipped.
fn cf(bytes: &[u8], base: CellRef, links: &Links) -> Result<CfRule, &'static str> {
    const TRUNC: &str = "truncated";
    let mut t = Bytes { b: bytes, pos: 0 };
    let (ct, cp) = (t.u8().ok_or(TRUNC)?, t.u8().ok_or(TRUNC)?);
    let (cce1, cce2) = (usize::from(t.u16().ok_or(TRUNC)?), usize::from(t.u16().ok_or(TRUNC)?));
    let style = Box::new(dxfn(&mut t).ok_or(TRUNC)?);
    let rgce1 = t.take(cce1).ok_or(TRUNC)?;
    let rgce2 = t.take(cce2).ok_or(TRUNC)?;
    let text = |rgce: &[u8]| -> Result<String, &'static str> {
        if rgce.is_empty() {
            return Err("missing formula");
        }
        decode(rgce, &[], base, Mode::Cell, links).map(|e| gridcraft_formula::print(&e)).map_err(|_| "unsupported formula")
    };
    cf_rule(ct, cp, rgce1, rgce2, style, &text)
}

/// A cell-value (`ct` 1) or formula (`ct` 2) rule; `text` prints a formula.
fn cf_rule(
    ct: u8,
    cp: u8,
    rgce1: &[u8],
    rgce2: &[u8],
    style: Box<Style>,
    text: &dyn Fn(&[u8]) -> Result<String, &'static str>,
) -> Result<CfRule, &'static str> {
    match (ct, cp) {
        (1, 1..=8) => {
            let op = match cp {
                1 => CfOperator::Between,
                2 => CfOperator::NotBetween,
                3 => CfOperator::Equal,
                4 => CfOperator::NotEqual,
                5 => CfOperator::Greater,
                6 => CfOperator::Less,
                7 => CfOperator::GreaterOrEqual,
                _ => CfOperator::LessOrEqual,
            };
            let a = text(rgce1)?;
            let b = if cp <= 2 { Some(text(rgce2)?) } else { None };
            Ok(CfRule::CellIs { op, a, b, style })
        }
        (2, _) => Ok(CfRule::Expression { formula: text(rgce1)?, style }),
        _ => Err("unknown rule type"),
    }
}

/// UTF-16LE text.
fn utf16(raw: &[u8]) -> String {
    let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|p| u16::from_le_bytes(*p)).collect();
    String::from_utf16_lossy(&units)
}

/// A palette index as a colour: the BIFF8 default palette; system colours and "unset" are automatic.
fn icv(i: u32) -> Color {
    usize::try_from(i).ok().and_then(|i| INDEXED.get(i)).map_or(Color::Auto, |&c| Color::Rgb(c))
}

/// DXFN: the differential format. `None` when it is shorter than its flags claim.
fn dxfn(t: &mut Bytes<'_>) -> Option<Style> {
    let flags = t.u32()?;
    let flags2 = t.u16()?;
    let has = |bit: u32| flags & (1 << bit) != 0;
    let mut s = Style::default();
    if has(25) {
        let code = if flags2 & 1 != 0 {
            // DXFNumUsr: cb, then an XLUnicodeString.
            t.u16()?;
            let cch = usize::from(t.u16()?);
            let high = t.u8()? & 1 != 0;
            if high { Some(utf16(t.take(cch.checked_mul(2)?)?)) } else { Some(t.take(cch)?.iter().map(|&b| char::from(b)).collect()) }
        } else {
            t.u8()?;
            builtin_format(u32::from(t.u8()?)).map(str::to_string)
        };
        // ifmtNinch: the number format is not specified.
        if let Some(code) = code.filter(|_| !has(19)) {
            s.num_fmt = NumFmt::new(&code);
        }
    }
    if has(26) {
        font(t.take(118)?, &mut s);
    }
    if has(27) {
        t.take(8)?;
    }
    if has(28) {
        let (a, b) = (t.u32()?, t.u32()?);
        let line = |dg: u32, ic: u32| BorderLine { style: border_style(dg & 0xF), color: icv(ic & 0x7F) };
        let sides = [
            (10, &mut s.borders.left, line(a, a >> 16)),
            (11, &mut s.borders.right, line(a >> 4, a >> 23)),
            (12, &mut s.borders.top, line(a >> 8, b)),
            (13, &mut s.borders.bottom, line(a >> 12, b >> 7)),
        ];
        for (ninch, side, l) in sides {
            if !has(ninch) {
                *side = l;
            }
        }
    }
    if has(29) {
        let (p, c) = (t.u16()?, t.u16()?);
        let (fg, bg) =
            (if has(17) { Color::Auto } else { icv(u32::from(c & 0x7F)) }, if has(18) { Color::Auto } else { icv(u32::from((c >> 7) & 0x7F)) });
        let pattern = if has(16) { None } else { Some(pattern(p >> 10)) };
        s.fill = match pattern {
            // In a differential format a solid fill's colour is its background colour.
            Some(PatternType::Solid) | None => {
                let c = if bg != Color::Auto { bg } else { fg };
                if c == Color::Auto { Fill::default() } else { Fill::solid(c) }
            }
            Some(PatternType::None) => Fill::default(),
            Some(pattern) => Fill { pattern, fg, bg },
        };
    }
    if has(30) {
        let p = t.u16()?;
        if !has(8) {
            s.protection.locked = p & 1 != 0;
        }
        if !has(9) {
            s.protection.hidden = p & 2 != 0;
        }
        s.dxf_stated.protection = s.protection == gridcraft_model::style::Protection::default();
    }
    Some(s)
}

/// DXFFntD ([MS-XLS] 2.5.84), exactly 118 bytes.
fn font(b: &[u8], s: &mut Style) {
    let mut t = Bytes { b, pos: 0 };
    let field = |at: usize, n: usize| -> u32 { b.get(at..at + n).map_or(0, |s| s.iter().rev().fold(0, |v, &x| (v << 8) | u32::from(x))) };
    let cch = t.u8().map_or(0, usize::from);
    if (1..=32).contains(&cch) {
        let high = t.u8().is_some_and(|f| f & 1 != 0);
        let name = if high { t.take(cch * 2).map(utf16) } else { t.take(cch).map(|r| r.iter().map(|&c| char::from(c)).collect()) };
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            s.dxf_stated.name = name == s.font.name;
            s.font.name = name;
        }
    }
    let (height, ts, bls, sss, uls) = (field(64, 4) as i32, field(68, 4), field(72, 2), field(74, 2), field(76, 1));
    let (fore, ts_ninch, sss_ninch, uls_ninch, bls_ninch) = (field(80, 4), field(88, 4), field(92, 4), field(96, 4), field(100, 4));
    if height > 0 && height <= 409 * 20 {
        let size = height as f32 / 20.0;
        s.dxf_stated.size = size == s.font.size;
        s.font.size = size;
    }
    if ts_ninch & 0x02 == 0 {
        s.font.italic = ts & 0x02 != 0;
        s.dxf_stated.italic = !s.font.italic;
    }
    if ts_ninch & 0x80 == 0 {
        s.font.strike = ts & 0x80 != 0;
        s.dxf_stated.strike = !s.font.strike;
    }
    if bls_ninch == 0 && bls != 0 {
        s.font.bold = bls >= 700;
        s.dxf_stated.bold = !s.font.bold;
    }
    if sss_ninch == 0 {
        s.font.vert = match sss {
            1 => VertAlign::Superscript,
            2 => VertAlign::Subscript,
            _ => VertAlign::Baseline,
        };
    }
    if uls_ninch == 0 {
        s.font.underline = match uls {
            0x01 => Underline::Single,
            0x02 => Underline::Double,
            0x21 => Underline::SingleAccounting,
            0x22 => Underline::DoubleAccounting,
            _ => Underline::None,
        };
        s.dxf_stated.underline = s.font.underline == Underline::None;
    }
    s.font.color = icv(fore);
}

/// A BIFF8 border line style (`dg*`).
fn border_style(dg: u32) -> BorderStyle {
    match dg {
        1 => BorderStyle::Thin,
        2 => BorderStyle::Medium,
        3 => BorderStyle::Dashed,
        4 => BorderStyle::Dotted,
        5 => BorderStyle::Thick,
        6 => BorderStyle::Double,
        7 => BorderStyle::Hair,
        8 => BorderStyle::MediumDashed,
        9 => BorderStyle::DashDot,
        10 => BorderStyle::MediumDashDot,
        11 => BorderStyle::DashDotDot,
        12 => BorderStyle::MediumDashDotDot,
        13 => BorderStyle::SlantDashDot,
        _ => BorderStyle::None,
    }
}

/// A BIFF8 fill pattern (`fls`).
fn pattern(fls: u16) -> PatternType {
    match fls {
        1 => PatternType::Solid,
        2 => PatternType::Gray50,
        3 => PatternType::Gray75,
        4 => PatternType::Gray25,
        5 => PatternType::DarkHorizontal,
        6 => PatternType::DarkVertical,
        7 => PatternType::DarkDown,
        8 => PatternType::DarkUp,
        9 => PatternType::DarkGrid,
        10 => PatternType::DarkTrellis,
        11 => PatternType::LightHorizontal,
        12 => PatternType::LightVertical,
        13 => PatternType::LightDown,
        14 => PatternType::LightUp,
        15 => PatternType::LightGrid,
        16 => PatternType::LightTrellis,
        17 => PatternType::Gray125,
        18 => PatternType::Gray0625,
        _ => PatternType::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A formula rule with bold, a single underline, a solid yellow fill and a thin red bottom border.
    #[test]
    fn dxfn_bold_fill_and_border() {
        let flags: u32 = 0x003F_FFFF & !(1 << 13 | 1 << 16 | 1 << 18) | 1 << 26 | 1 << 28 | 1 << 29;
        let mut b = vec![2, 0, 2, 0, 0, 0];
        b.extend(flags.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        let mut font = vec![0u8; 118];
        font[64..68].copy_from_slice(&(-1i32).to_le_bytes());
        font[72..74].copy_from_slice(&700u16.to_le_bytes());
        font[76] = 1;
        font[80..84].copy_from_slice(&(-1i32).to_le_bytes());
        font[88..92].copy_from_slice(&0x9Au32.to_le_bytes());
        font[92..96].copy_from_slice(&1u32.to_le_bytes());
        b.extend(font);
        b.extend((1u32 << 12 | 0x0A << 23).to_le_bytes()); // bottom thin; right colour (ignored)
        b.extend((0x0Au32 << 7).to_le_bytes()); // icvBottom red
        b.extend((1u16 << 10).to_le_bytes()); // solid
        b.extend((0x0Du16 << 7).to_le_bytes()); // icvBackground yellow
        b.extend([0x1D, 0x01]);
        let rule = cf(&b, CellRef::new(0, 0), &Links::default()).unwrap();
        let CfRule::Expression { formula, style } = rule else { panic!("expected a formula rule") };
        assert_eq!(formula, "TRUE");
        assert!(style.font.bold && !style.font.italic);
        assert_eq!(style.font.underline, Underline::Single);
        assert_eq!(style.font.color, Color::Auto);
        assert_eq!(style.fill, Fill::solid(Color::Rgb(0xFFFF00)));
        assert_eq!(style.borders.bottom, BorderLine { style: BorderStyle::Thin, color: Color::Rgb(0xFF0000) });
        assert!(style.borders.right.is_none());
        // Cut anywhere inside, the record is refused rather than misread.
        for n in 0..b.len() - 2 {
            assert!(cf(&b[..n], CellRef::new(0, 0), &Links::default()).is_err());
        }
    }
}
