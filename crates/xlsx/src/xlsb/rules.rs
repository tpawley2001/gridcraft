//! Data validation and conditional formatting in XLSB worksheets.
//!
//! Data validation: BrtDVal ([MS-XLSB] 2.4.353) records inside BrtBeginDVals / BrtEndDVals.
//! Conditional formatting: BrtBeginConditionalFormatting ([MS-XLSB] 2.4.34) holds the ranges, each
//! BrtBeginCFRule ([MS-XLSB] 2.4.23) .. BrtEndCFRule one rule, with BrtBeginColorScale,
//! BrtBeginDatabar or BrtBeginIconSet blocks of BrtCFVO ([MS-XLSB] 2.4.331) and BrtColor records.
//! Both map onto the model the way the .xlsx reader maps `<dataValidation>` and `<cfRule>`. Formulas
//! are BIFF12 parsed formulas decoded by the cell-formula decoder, relative to the top-left cell of
//! the first range; a rule or validation that can't be read whole is skipped, never kept half-read.

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_formula::Expr;
use gridcraft_model::{CfOperator, CfRule, CfValueKind, Color, CondFormat, ErrorStyle, Sheet, Style, Validation, ValidationKind};

use super::{Budget, Bytes, dxf, invalid};
use crate::sheet_read::MAX_COND_CELLS;
use crate::xls::formula::{Links, Mode, decode_xlsb};
use crate::{IoError, ReadReport};

/// UncheckedSqRfX in BrtDVal and BrtBeginConditionalFormatting: fewer than 8192 ranges.
const MAX_RANGES: u32 = 8191;
/// Most validations one sheet keeps (the DVALS collection holds at most 65534).
const MAX_VALIDATIONS: usize = 65_534;
/// Most validation ranges one sheet keeps over all its validations.
const MAX_VALIDATION_RANGES: usize = 1_000_000;
/// CFParsedFormula / DVParsedFormula `cce`: less than 16385 bytes of tokens.
const MAX_RGCE: u32 = 16_384;
/// A colour scale has 2 or 3 stops; an icon set 3 to 5 thresholds.
const MAX_CFVOS: usize = 5;

/// Rule-import state for one worksheet: the open conditional-formatting block and rule, and what
/// was skipped (reported once per kind when the sheet ends).
#[derive(Default)]
pub(super) struct Rules {
    /// Ranges of the open BrtBeginConditionalFormatting; `Err` when they couldn't be read.
    block: Option<Result<Vec<RangeRef>, String>>,
    /// The open BrtBeginCFRule; `Err` once any of its records was unusable.
    rule: Option<Result<Pending, String>>,
    dv_skipped: Skipped,
    cf_skipped: Skipped,
    cf_dropped: bool,
    validation_ranges: usize,
}

#[derive(Default)]
struct Skipped {
    count: usize,
    reason: String,
}
impl Skipped {
    fn add(&mut self, reason: impl Into<String>) {
        if self.count == 0 {
            self.reason = reason.into();
        }
        self.count += 1;
    }
}

/// The fixed part of a BrtBeginCFRule, with its sub-records as they arrive.
struct Pending {
    ranges: Vec<RangeRef>,
    kind: u32,
    template: u32,
    dxf: u32,
    priority: u32,
    param: u32,
    stop: bool,
    bottom: bool,
    percent: bool,
    text: String,
    f1: Option<Formula>,
    f2: Option<Formula>,
    /// BrtBeginDatabar `fShowValue`, or BrtBeginIconSet `iSet` and flags.
    show_value: bool,
    icon_set: Option<u32>,
    reverse: bool,
    cfvos: Vec<CfValueKind>,
    colors: Vec<Color>,
}

type Formula = (Vec<u8>, Vec<u8>);

impl Rules {
    /// BrtDVal (64); the BrtBeginDVals / BrtEndDVals brackets carry nothing this import uses.
    pub(super) fn validation(&mut self, id: u16, payload: &[u8], sheet: &mut Sheet, budget: &mut Budget, links: &Links) -> Result<(), IoError> {
        if id != 64 {
            return Ok(());
        }
        if sheet.validations.len() >= MAX_VALIDATIONS {
            self.dv_skipped.add(format!("more than {MAX_VALIDATIONS} validations"));
            return Ok(());
        }
        let v = match dval(payload, links) {
            Ok(v) => v,
            Err(e) => {
                self.dv_skipped.add(e);
                return Ok(());
            }
        };
        if self.validation_ranges.saturating_add(v.ranges.len()) > MAX_VALIDATION_RANGES {
            self.dv_skipped.add(format!("more than {MAX_VALIDATION_RANGES} validation ranges"));
            return Ok(());
        }
        for text in [&v.input_title, &v.error_title, &v.input_message, &v.error_message, &v.f1] {
            budget.text(text.len())?;
        }
        budget.text(v.f2.as_ref().map_or(0, String::len))?;
        self.validation_ranges += v.ranges.len();
        sheet.validations.push(v);
        Ok(())
    }

    /// The conditional-formatting records: 461..=471 and BrtColor (564).
    pub(super) fn cond_format(&mut self, id: u16, payload: &[u8], sheet: &mut Sheet, dxfs: &[Style], links: &Links) {
        match id {
            461 => {
                if self.rule.take().is_some() {
                    self.cf_skipped.add("unterminated rule");
                }
                let mut b = Bytes::new(payload);
                let ranges = (|| {
                    b.u32()?; // ccf
                    b.u32()?; // fPivot
                    sqrfx(&mut b)
                })();
                self.block = Some(ranges.map_err(|e| e.to_string()));
            }
            462 => {
                if self.rule.take().is_some() {
                    self.cf_skipped.add("unterminated rule");
                }
                self.block = None;
            }
            463 => {
                if self.rule.take().is_some() {
                    self.cf_skipped.add("unterminated rule");
                }
                self.rule = Some(match &self.block {
                    Some(Ok(ranges)) => header(payload, ranges.clone()).map_err(|e| e.to_string()),
                    Some(Err(e)) => Err(e.clone()),
                    None => Err("rule outside a conditional format".into()),
                });
            }
            464 => match self.rule.take() {
                Some(Ok(p)) => match build(p, dxfs, links) {
                    Ok(f) => self.push(sheet, f),
                    Err(e) => self.cf_skipped.add(e),
                },
                Some(Err(e)) => self.cf_skipped.add(e),
                None => {}
            },
            465 => self.update(|p| {
                let mut b = Bytes::new(payload);
                p.icon_set = Some(b.u32()?);
                let flags = b.u16()?;
                p.show_value = flags & 0b10 == 0; // fIcon: icons only
                p.reverse = flags & 0b100 != 0; // fReverse
                Ok(())
            }),
            467 => self.update(|p| {
                let mut b = Bytes::new(payload);
                b.u8()?; // bLenMin
                b.u8()?; // bLenMax
                p.show_value = b.u8()? != 0;
                Ok(())
            }),
            471 => self.update(|p| {
                let base = p.ranges.first().map_or(CellRef::new(0, 0), |r| r.start);
                let v = cfvo(payload, base, links)?;
                if p.cfvos.len() >= MAX_CFVOS {
                    return Err(invalid("too many value objects"));
                }
                p.cfvos.push(v);
                Ok(())
            }),
            564 => self.update(|p| {
                let c = dxf::color(&mut Bytes::new(payload))?;
                if p.colors.len() >= MAX_CFVOS {
                    return Err(invalid("too many colours"));
                }
                p.colors.push(c);
                Ok(())
            }),
            _ => {}
        }
    }

    /// Applies a sub-record to the open rule; a failure skips the whole rule.
    fn update(&mut self, f: impl FnOnce(&mut Pending) -> Result<(), IoError>) {
        if let Some(Ok(p)) = &mut self.rule
            && let Err(e) = f(p)
        {
            self.rule = Some(Err(e.to_string()));
        }
    }

    fn push(&mut self, sheet: &mut Sheet, f: CondFormat) {
        let rules = sheet.cond_formats.len() as u64;
        let area = f.ranges.iter().map(RangeRef::count).sum::<u64>().saturating_mul(1 + rules);
        if area > MAX_COND_CELLS {
            self.cf_dropped = true;
            return;
        }
        sheet.cond_formats.push(f);
    }

    /// Reports what was skipped on the sheet: one warning per kind.
    pub(super) fn finish(&mut self, name: &str, report: &mut ReadReport) {
        if self.rule.take().is_some() || self.block.take().is_some() {
            self.cf_skipped.add("unterminated rule");
        }
        let Skipped { count, reason } = &self.dv_skipped;
        if *count > 0 {
            report.warn(format!("{count} XLSB data validation(s) on sheet '{name}' were skipped: {reason}."));
        }
        let Skipped { count, reason } = &self.cf_skipped;
        if *count > 0 {
            report.warn(format!("{count} XLSB conditional formatting rule(s) on sheet '{name}' were skipped: {reason}."));
        }
        if self.cf_dropped {
            report.warn(format!("conditional formats past {MAX_COND_CELLS} kept range cells were dropped"));
        }
    }
}

/// UncheckedSqRfX ([MS-XLSB] 2.5.156): a count, then rwFirst, rwLast, colFirst, colLast per range.
/// Ranges outside the grid are dropped; none left is an error.
fn sqrfx(b: &mut Bytes<'_>) -> Result<Vec<RangeRef>, IoError> {
    let n = b.u32()?;
    if n == 0 || n > MAX_RANGES || n as usize > b.rest.len() / 16 {
        return Err(invalid("missing, too many or truncated ranges"));
    }
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let (r0, r1, c0, c1) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
        if r0 <= r1 && c0 <= c1 && r1 < MAX_ROWS && c1 < MAX_COLS {
            out.push(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)));
        }
    }
    if out.is_empty() {
        return Err(invalid("no range inside the sheet"));
    }
    Ok(out)
}

/// A parsed formula (`cce`, `rgce`, `cb`, `rgcb`): tokens and their extra data.
fn parsed<'a>(b: &mut Bytes<'a>) -> Result<(&'a [u8], &'a [u8]), IoError> {
    let cce = b.u32()?;
    if cce > MAX_RGCE {
        return Err(invalid("formula too long"));
    }
    let rgce = b.take(cce as usize)?;
    let cb = b.u32()?;
    let rgcb = b.take(cb as usize)?;
    Ok((rgce, rgcb))
}

/// Decodes `rgce` at `base` as formula text (no leading `=`), or says why it can't be kept.
fn decode((rgce, rgcb): (&[u8], &[u8]), base: CellRef, links: &Links) -> Result<Expr, String> {
    let e = decode_xlsb(rgce, rgcb, base, Mode::Cell, links).map_err(|_| "a formula uses a token this import can't decode".to_string())?;
    if gridcraft_formula::parse(&gridcraft_formula::print(&e)).is_err() {
        return Err("a formula can't be represented".into());
    }
    Ok(e)
}
fn text(formula: (&[u8], &[u8]), base: CellRef, links: &Links) -> Result<String, String> {
    decode(formula, base, links).map(|e| gridcraft_formula::print(&e))
}

/// The value comparisons shared by BrtDVal `typOperator` (from 0) and CFOper (from 1).
fn operator(n: u32) -> Option<CfOperator> {
    Some(match n {
        0 => CfOperator::Between,
        1 => CfOperator::NotBetween,
        2 => CfOperator::Equal,
        3 => CfOperator::NotEqual,
        4 => CfOperator::Greater,
        5 => CfOperator::Less,
        6 => CfOperator::GreaterOrEqual,
        7 => CfOperator::LessOrEqual,
        _ => return None,
    })
}

/// BrtDVal: flags, sqrfx, DValStrings (error title, error, prompt title, prompt) and two
/// DVParsedFormula.
fn dval(payload: &[u8], links: &Links) -> Result<Validation, String> {
    let mut b = Bytes::new(payload);
    let fields = (|| -> Result<_, IoError> {
        let flags = b.u32()?;
        let ranges = sqrfx(&mut b)?;
        let mut strings = [String::new(), String::new(), String::new(), String::new()];
        for s in &mut strings {
            *s = b.string(32_767, true)?.unwrap_or_default();
        }
        let f1 = parsed(&mut b)?;
        let f2 = parsed(&mut b)?;
        Ok((flags, ranges, strings, f1, f2))
    })();
    let (flags, ranges, [error_title, error_message, input_title, input_message], f1, f2) = fields.map_err(|e| e.to_string())?;
    let kind = match flags & 0xF {
        0 => ValidationKind::Any,
        1 => ValidationKind::Whole,
        2 => ValidationKind::Decimal,
        3 => ValidationKind::List,
        4 => ValidationKind::Date,
        5 => ValidationKind::Time,
        6 => ValidationKind::TextLength,
        7 => ValidationKind::Custom,
        _ => return Err("unknown validation type".into()),
    };
    let error_style = match (flags >> 4) & 7 {
        0 => ErrorStyle::Stop,
        1 => ErrorStyle::Warning,
        2 => ErrorStyle::Information,
        _ => return Err("unknown error style".into()),
    };
    // The operator is undefined for "any", list and custom validations.
    let op = if matches!(kind, ValidationKind::Any | ValidationKind::List | ValidationKind::Custom) {
        CfOperator::Between
    } else {
        operator((flags >> 20) & 0xF).ok_or("unknown validation operator")?
    };
    let base = ranges.first().map_or(CellRef::new(0, 0), |r| r.start);
    let f1 = if f1.0.is_empty() || kind == ValidationKind::Any {
        String::new()
    } else {
        match decode(f1, base, links)? {
            // A literal list is one string whose items are separated by NUL characters.
            Expr::Text(t) if kind == ValidationKind::List => gridcraft_formula::print(&Expr::Text(t.replace('\0', ",").into())),
            e if kind == ValidationKind::List => format!("={}", gridcraft_formula::print(&e)),
            e => gridcraft_formula::print(&e),
        }
    };
    let two = matches!(op, CfOperator::Between | CfOperator::NotBetween)
        && !matches!(kind, ValidationKind::Any | ValidationKind::List | ValidationKind::Custom);
    let f2 = if two && !f2.0.is_empty() { Some(text(f2, base, links)?) } else { None };
    Ok(Validation {
        ranges,
        kind,
        op,
        f1,
        f2,
        allow_blank: flags & (1 << 8) != 0,
        // fSuppressCombo hides the in-cell drop-down arrow.
        in_cell_dropdown: flags & (1 << 9) == 0,
        input_title,
        input_message,
        show_input: flags & (1 << 18) != 0,
        error_title,
        error_message,
        error_style,
        show_error: flags & (1 << 19) != 0,
    })
}

/// BrtBeginCFRule: iType, iTemplate, dxfId, iPri, iParam, two reserved fields, flags, the three
/// formula sizes, strParam and the formulas whose size is nonzero.
fn header(payload: &[u8], ranges: Vec<RangeRef>) -> Result<Pending, IoError> {
    let mut b = Bytes::new(payload);
    let kind = b.u32()?;
    let template = b.u32()?;
    let dxf = b.u32()?;
    let priority = u32::try_from(i32::from_le_bytes(b.u32()?.to_le_bytes())).unwrap_or(0);
    let param = b.u32()?;
    b.take(8)?; // reserved1, reserved2
    let flags = b.u16()?;
    let sizes = [b.u32()?, b.u32()?, b.u32()?];
    let text = b.string(32_767, true)?.unwrap_or_default();
    let f1 = if sizes[0] != 0 {
        let (rgce, rgcb) = parsed(&mut b)?;
        Some((rgce.to_vec(), rgcb.to_vec()))
    } else {
        None
    };
    let f2 = if sizes[1] != 0 {
        let (rgce, rgcb) = parsed(&mut b)?;
        Some((rgce.to_vec(), rgcb.to_vec()))
    } else {
        None
    };
    if sizes[2] != 0 {
        parsed(&mut b)?; // The display condition of scales, bars and icon sets is not imported.
    }
    Ok(Pending {
        ranges,
        kind,
        template,
        dxf,
        priority,
        param,
        stop: flags & 0b10 != 0,
        bottom: flags & 0b1000 != 0,
        percent: flags & 0b1_0000 != 0,
        text,
        f1,
        f2,
        show_value: true,
        icon_set: None,
        reverse: false,
        cfvos: vec![],
        colors: vec![],
    })
}

/// BrtCFVO: iType, numParam, fSaveGTE, fGTE, cbFmla and, when cbFmla is nonzero, the formula.
fn cfvo(payload: &[u8], base: CellRef, links: &Links) -> Result<CfValueKind, IoError> {
    let mut b = Bytes::new(payload);
    let kind = b.u32()?;
    let n = f64::from_le_bytes(b.take(8)?.try_into().map_err(|_| invalid("truncated number"))?);
    b.u32()?; // fSaveGTE
    b.u32()?; // fGTE
    let formula = if b.u32()? != 0 { Some(text(parsed(&mut b)?, base, links).map_err(invalid)?) } else { None };
    let number = |f: fn(f64) -> CfValueKind| match &formula {
        Some(t) => Ok(CfValueKind::Formula(t.clone())),
        None if n.is_finite() => Ok(f(n)),
        None => Err(invalid("nonfinite threshold")),
    };
    match kind {
        1 => number(CfValueKind::Number),
        2 => Ok(CfValueKind::Min),
        3 => Ok(CfValueKind::Max),
        4 => number(CfValueKind::Percent),
        5 => number(CfValueKind::Percentile),
        7 => formula.map(CfValueKind::Formula).ok_or_else(|| invalid("formula threshold without a formula")),
        _ => Err(invalid("unknown threshold type")),
    }
}

/// KPISets ([MS-XLSB] 2.5.86) as ECMA-376 icon set names.
fn icon_set(n: u32) -> Option<&'static str> {
    const SETS: [&str; 17] = [
        "3Arrows",
        "3ArrowsGray",
        "3Flags",
        "3TrafficLights1",
        "3TrafficLights2",
        "3Signs",
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
    SETS.get(n as usize).copied()
}

/// CFTemp time-period templates (0x0F..=0x18) as ECMA-376 `timePeriod` names.
fn time_period(template: u32) -> Option<&'static str> {
    Some(match template {
        0x0F => "today",
        0x10 => "tomorrow",
        0x11 => "yesterday",
        0x12 => "last7Days",
        0x13 => "lastMonth",
        0x14 => "nextMonth",
        0x15 => "thisWeek",
        0x16 => "nextWeek",
        0x17 => "lastWeek",
        0x18 => "thisMonth",
        _ => return None,
    })
}

/// The rule a complete BrtBeginCFRule .. BrtEndCFRule collection describes (CFType × CFTemp).
fn build(p: Pending, dxfs: &[Style], links: &Links) -> Result<CondFormat, String> {
    let base = p.ranges.first().map_or(CellRef::new(0, 0), |r| r.start);
    let style = || Box::new(dxfs.get(p.dxf as usize).cloned().unwrap_or_default());
    let formula =
        |f: &Option<Formula>| f.as_ref().ok_or_else(|| "a formula is missing".to_string()).and_then(|(rgce, rgcb)| text((rgce, rgcb), base, links));
    let rule = match (p.kind, p.template) {
        (1, _) => {
            let op = p.param.checked_sub(1).and_then(operator).ok_or("unknown comparison")?;
            let b = if matches!(op, CfOperator::Between | CfOperator::NotBetween) { Some(formula(&p.f2)?) } else { None };
            CfRule::CellIs { op, a: formula(&p.f1)?, b, style: style() }
        }
        (2, 1) => CfRule::Expression { formula: formula(&p.f1)?, style: style() },
        (2, 7) => CfRule::Unique { style: style() },
        (2, 8) => {
            let text = p.text.clone();
            match p.param {
                0 => CfRule::ContainsText { text, style: style() },
                1 => CfRule::NotContainsText { text, style: style() },
                2 => CfRule::BeginsWith { text, style: style() },
                3 => CfRule::EndsWith { text, style: style() },
                _ => return Err("unknown text comparison".into()),
            }
        }
        (2, 9) => CfRule::Blanks { style: style() },
        (2, 10) => CfRule::NoBlanks { style: style() },
        (2, 11) => CfRule::Errors { style: style() },
        (2, 12) => CfRule::NoErrors { style: style() },
        (2, t @ 0x0F..=0x18) => CfRule::TimePeriod { period: time_period(t).unwrap_or("today").to_string(), style: style() },
        (2, t @ (0x19 | 0x1A | 0x1D | 0x1E)) => {
            CfRule::AboveAverage { below: matches!(t, 0x1A | 0x1E), equal: matches!(t, 0x1D | 0x1E), std_dev: p.param.min(3) as u8, style: style() }
        }
        (2, 0x1B) => CfRule::Duplicate { style: style() },
        (3, _) => {
            if !(2..=3).contains(&p.cfvos.len()) || p.cfvos.len() != p.colors.len() {
                return Err("a colour scale needs two or three stops".into());
            }
            CfRule::ColorScale { stops: p.cfvos.iter().cloned().zip(p.colors.iter().copied()).collect() }
        }
        (4, _) => CfRule::DataBar {
            min: p.cfvos.first().cloned().unwrap_or(CfValueKind::Min),
            max: p.cfvos.get(1).cloned().unwrap_or(CfValueKind::Max),
            color: p.colors.first().copied().unwrap_or_default(),
            gradient: true,
            show_value: p.show_value,
        },
        (5, _) => CfRule::Top10 { bottom: p.bottom, percent: p.percent, rank: p.param, style: style() },
        (6, _) => {
            let set = p.icon_set.and_then(icon_set).ok_or("unknown icon set")?;
            CfRule::IconSet { set: set.to_string(), thresholds: p.cfvos.clone(), reverse: p.reverse, show_value: p.show_value }
        }
        _ => return Err("unsupported rule type".into()),
    };
    Ok(CondFormat { ranges: p.ranges, rule, priority: p.priority, stop_if_true: p.stop })
}
