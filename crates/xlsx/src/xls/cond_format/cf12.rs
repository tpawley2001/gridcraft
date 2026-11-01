//! Excel 2007+ conditional formats in `.xls` sheets: CF12 ([MS-XLS] 2.4.42) holds one rule of
//! any kind (colour scale, data bar, icon set, top N, templates such as "contains text"), and
//! CFEx ([MS-XLS] 2.4.43) adds a priority, stop-if-true and a template to a classic CF rule.

use gridcraft_core::CellRef;
use gridcraft_model::{CfRule, CfValueKind, Color, Style};

use super::{Bytes, cf_rule, dxfn, icv};
use crate::xls::formula::{Links, Mode, decode};

const TRUNC: &str = "truncated";

/// One CF12 record as a rule.
pub(in crate::xls) struct Rule12 {
    pub rule: CfRule,
    pub priority: u32,
    pub stop_if_true: bool,
}

/// What a CFEx record adds to a classic CF rule.
pub(in crate::xls) struct ExRule {
    /// Zero-based index of the CF record after its CondFmt.
    pub index: usize,
    pub template: u16,
    pub priority: u32,
    pub stop_if_true: bool,
    pub style: Option<Style>,
    pub params: Vec<u8>,
}

/// A CFEx record: the CondFmt `nID` it extends and, for a classic CF rule, the extension.
pub(in crate::xls) struct CfEx {
    pub id: u16,
    pub ext: Option<ExRule>,
}

/// Icon set names by `iIconSet` ([MS-XLS] 2.5.20).
const ICON_SETS: [&str; 17] = [
    "3Arrows",
    "3ArrowsGray",
    "3Flags",
    "3TrafficLights1",
    "3Signs",
    "3TrafficLights2",
    "3Symbols",
    "3Symbols2",
    "4Arrows",
    "4ArrowsGray",
    "4RedToBlack",
    "4Rating",
    "4TrafficLights",
    "5Arrows",
    "5ArrowsGray",
    "5Rating",
    "5Quarters",
];

/// DXFN12: a size, then a DXFN (and extensions, skipped); an empty one is two reserved bytes.
fn dxfn12(t: &mut Bytes<'_>) -> Option<Style> {
    let cb = usize::try_from(t.u32()?).ok()?;
    if cb == 0 {
        t.take(2)?;
        return Some(Style::default());
    }
    dxfn(&mut Bytes { b: t.take(cb)?, pos: 0 })
}

/// A CF12 record (after its 12-byte header) as a rule, its formulas relative to `base`.
/// `notes` collects what was read only in part (each reason once).
pub(in crate::xls) fn cf12(bytes: &[u8], base: CellRef, links: &Links, notes: &mut Vec<&'static str>) -> Result<Rule12, &'static str> {
    let mut t = Bytes { b: bytes, pos: 0 };
    t.take(12).ok_or(TRUNC)?;
    let (ct, cp) = (t.u8().ok_or(TRUNC)?, t.u8().ok_or(TRUNC)?);
    let (cce1, cce2) = (usize::from(t.u16().ok_or(TRUNC)?), usize::from(t.u16().ok_or(TRUNC)?));
    let style = dxfn12(&mut t).ok_or(TRUNC)?;
    let rgce1 = t.take(cce1).ok_or(TRUNC)?;
    let rgce2 = t.take(cce2).ok_or(TRUNC)?;
    // fmlaActive: an extra condition for scales, bars and icons; not modelled.
    let active = usize::from(t.u16().ok_or(TRUNC)?);
    t.take(active).ok_or(TRUNC)?;
    let stop_if_true = t.u8().ok_or(TRUNC)? & 0x02 != 0;
    let priority = u32::from(t.u16().ok_or(TRUNC)?);
    let template = t.u16().ok_or(TRUNC)?;
    let cb = usize::from(t.u8().ok_or(TRUNC)?);
    let params = t.take(cb).ok_or(TRUNC)?;
    let text = |rgce: &[u8]| -> Result<String, &'static str> {
        if rgce.is_empty() {
            return Err("missing formula");
        }
        decode(rgce, &[], base, Mode::Cell, links).map(|e| gridcraft_formula::print(&e)).map_err(|_| "unsupported formula")
    };
    let rule = match ct {
        1 => cf_rule(1, cp, rgce1, rgce2, Box::new(style), &text)?,
        2 | 5 if template == 5 => {
            let (top, percent, rank) = if ct == 5 { filter(&mut t).ok_or(TRUNC)? } else { filter_params(params).ok_or(TRUNC)? };
            CfRule::Top10 { bottom: !top, percent, rank, style: Box::new(style) }
        }
        2 | 5 => {
            let formula = text(rgce1);
            match template_rule(template, params, formula.as_deref().ok(), Box::new(style.clone())) {
                Some(rule) => rule,
                None if ct == 2 => CfRule::Expression { formula: formula?, style: Box::new(style) },
                None => return Err("filter rules other than top/bottom N"),
            }
        }
        3 => gradient(&mut t, base, links, notes)?,
        4 => databar(&mut t, base, links, notes)?,
        6 => multistate(&mut t, base, links)?,
        _ => return Err("unknown rule type"),
    };
    Ok(Rule12 { rule, priority, stop_if_true })
}

/// A CFEx record; `None` if it is malformed.
pub(in crate::xls) fn cf_ex(bytes: &[u8]) -> Option<CfEx> {
    let mut t = Bytes { b: bytes, pos: 0 };
    t.take(12)?;
    let is_cf12 = t.u32()? != 0;
    let id = t.u16()?;
    if is_cf12 {
        return Some(CfEx { id, ext: None });
    }
    let index = usize::from(t.u16()?);
    let _cp = t.u8()?;
    let template = u16::from(t.u8()?);
    let priority = u32::from(t.u16()?);
    let stop_if_true = t.u8()? & 0x02 != 0;
    let style = if t.u8()? != 0 { Some(dxfn12(&mut t)?) } else { None };
    let cb = usize::from(t.u8()?);
    let params = t.take(cb)?.to_vec();
    Some(CfEx { id, ext: Some(ExRule { index, template, priority, stop_if_true, style, params }) })
}

/// The rule a template (`icfTemplate`) makes of a formula rule, for the kinds that have their
/// own rule type; `None` for plain formulas and templates without one.
pub(in crate::xls) fn template_rule(template: u16, params: &[u8], formula: Option<&str>, style: Box<Style>) -> Option<CfRule> {
    let param = params.get(..2).and_then(|p| p.try_into().ok()).map_or(0, u16::from_le_bytes);
    let period = |p: &str| Some(CfRule::TimePeriod { period: p.to_string(), style: style.clone() });
    let average = |below: bool, equal: bool| Some(CfRule::AboveAverage { below, equal, std_dev: param.min(3) as u8, style: style.clone() });
    match template {
        0x05 => filter_params(params).map(|(top, percent, rank)| CfRule::Top10 { bottom: !top, percent, rank, style }),
        0x07 => Some(CfRule::Unique { style }),
        0x08 => {
            let text = first_text(formula?)?;
            Some(match param {
                0 => CfRule::ContainsText { text, style },
                1 => CfRule::NotContainsText { text, style },
                2 => CfRule::BeginsWith { text, style },
                3 => CfRule::EndsWith { text, style },
                _ => return None,
            })
        }
        0x09 => Some(CfRule::Blanks { style }),
        0x0A => Some(CfRule::NoBlanks { style }),
        0x0B => Some(CfRule::Errors { style }),
        0x0C => Some(CfRule::NoErrors { style }),
        0x0F => period("today"),
        0x10 => period("tomorrow"),
        0x11 => period("yesterday"),
        0x12 => period("last7Days"),
        0x13 => period("lastMonth"),
        0x14 => period("nextMonth"),
        0x15 => period("thisWeek"),
        0x16 => period("nextWeek"),
        0x17 => period("lastWeek"),
        0x18 => period("thisMonth"),
        0x19 => average(false, false),
        0x1A => average(true, false),
        0x1B => Some(CfRule::Duplicate { style }),
        0x1D => average(false, true),
        0x1E => average(true, true),
        _ => None,
    }
}

/// The first text constant in a printed formula (`SEARCH("abc",A1)` → `abc`).
fn first_text(formula: &str) -> Option<String> {
    let (_, rest) = formula.split_once('"')?;
    let mut out = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '"' {
            out.push(c);
        } else if chars.peek() == Some(&'"') {
            chars.next();
            out.push('"');
        } else {
            return Some(out);
        }
    }
    None
}

/// CFFilter: top/bottom, percent, rank.
fn filter(t: &mut Bytes<'_>) -> Option<(bool, bool, u32)> {
    t.u16()?;
    t.u8()?;
    let flags = t.u8()?;
    Some((flags & 1 != 0, flags & 2 != 0, u32::from(t.u16()?)))
}

/// CFExFilterParams: top/bottom, percent, rank.
fn filter_params(p: &[u8]) -> Option<(bool, bool, u32)> {
    let flags = *p.first()?;
    let rank = u16::from_le_bytes(p.get(1..3)?.try_into().ok()?);
    Some((flags & 1 != 0, flags & 2 != 0, u32::from(rank)))
}

/// CFVO: a threshold.
fn cfvo(t: &mut Bytes<'_>, base: CellRef, links: &Links) -> Result<CfValueKind, &'static str> {
    let ty = t.u8().ok_or(TRUNC)?;
    let cce = usize::from(t.u16().ok_or(TRUNC)?);
    let rgce = t.take(cce).ok_or(TRUNC)?;
    if matches!(ty, 2 | 3) {
        return Ok(if ty == 2 { CfValueKind::Min } else { CfValueKind::Max });
    }
    if cce > 0 {
        let e = decode(rgce, &[], base, Mode::Cell, links).map_err(|_| "unsupported formula")?;
        return Ok(CfValueKind::Formula(gridcraft_formula::print(&e)));
    }
    let n = t.f64().ok_or(TRUNC)?;
    if !n.is_finite() {
        return Err("unreadable threshold");
    }
    match ty {
        1 => Ok(CfValueKind::Number(n)),
        4 => Ok(CfValueKind::Percent(n)),
        5 => Ok(CfValueKind::Percentile(n)),
        7 => Ok(CfValueKind::Formula(gridcraft_formula::print(&gridcraft_formula::Expr::Number(n)))),
        _ => Err("unknown threshold type"),
    }
}

/// CFColor: palette index, RGB or theme colour; automatic and unset colours stay automatic.
fn cf_color(t: &mut Bytes<'_>, notes: &mut Vec<&'static str>) -> Option<Color> {
    let (ty, value, tint) = (t.u32()?, t.u32()?, t.f64()?);
    Some(match ty {
        1 => icv(value),
        2 => {
            let [r, g, b, _] = value.to_le_bytes();
            Color::rgb(r, g, b)
        }
        3 if value <= 11 => {
            let tint = if tint.is_finite() { (tint * 1000.0).round().clamp(-1000.0, 1000.0) as i16 } else { 0 };
            Color::Theme(value as u8, tint)
        }
        3 => {
            note(notes, "colours outside the theme");
            Color::Auto
        }
        _ => Color::Auto,
    })
}

fn note(notes: &mut Vec<&'static str>, why: &'static str) {
    if !notes.contains(&why) {
        notes.push(why);
    }
}

/// CFGradient: a colour scale of 2 or 3 stops.
fn gradient(t: &mut Bytes<'_>, base: CellRef, links: &Links, notes: &mut Vec<&'static str>) -> Result<CfRule, &'static str> {
    t.take(3).ok_or(TRUNC)?;
    let (n_interp, n_curve) = (t.u8().ok_or(TRUNC)?, t.u8().ok_or(TRUNC)?);
    t.u8().ok_or(TRUNC)?;
    if !(2..=3).contains(&n_interp) || n_curve != n_interp {
        return Err("colour scales with an unusual number of stops");
    }
    let mut values = vec![];
    for _ in 0..n_interp {
        values.push(cfvo(t, base, links)?);
        t.f64().ok_or(TRUNC)?;
    }
    let mut colors = vec![];
    for _ in 0..n_curve {
        t.f64().ok_or(TRUNC)?;
        colors.push(cf_color(t, notes).ok_or(TRUNC)?);
    }
    Ok(CfRule::ColorScale { stops: values.into_iter().zip(colors).collect() })
}

/// CFDatabar.
fn databar(t: &mut Bytes<'_>, base: CellRef, links: &Links, notes: &mut Vec<&'static str>) -> Result<CfRule, &'static str> {
    t.take(3).ok_or(TRUNC)?;
    let flags = t.u8().ok_or(TRUNC)?;
    // iPercentMin / iPercentMax: bar lengths, not modelled.
    t.take(2).ok_or(TRUNC)?;
    let color = cf_color(t, notes).ok_or(TRUNC)?;
    let min = cfvo(t, base, links)?;
    let max = cfvo(t, base, links)?;
    Ok(CfRule::DataBar { min, max, color, gradient: true, show_value: flags & 0x02 != 0 })
}

/// CFMultistate: an icon set.
fn multistate(t: &mut Bytes<'_>, base: CellRef, links: &Links) -> Result<CfRule, &'static str> {
    t.take(3).ok_or(TRUNC)?;
    let states = t.u8().ok_or(TRUNC)?;
    let set = *ICON_SETS.get(usize::from(t.u8().ok_or(TRUNC)?)).ok_or("unknown icon sets")?;
    let flags = t.u8().ok_or(TRUNC)?;
    if !(3..=5).contains(&states) {
        return Err("unknown icon sets");
    }
    let mut thresholds = vec![];
    for _ in 0..states {
        thresholds.push(cfvo(t, base, links)?);
        // fEqual (>= rather than >) is not modelled; then 4 unused bytes.
        t.take(5).ok_or(TRUNC)?;
    }
    Ok(CfRule::IconSet { set: set.to_string(), thresholds, reverse: flags & 0x04 != 0, show_value: flags & 0x01 == 0 })
}
