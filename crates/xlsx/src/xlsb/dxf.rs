//! Differential formats of the XLSB styles part ([MS-XLSB] 2.2.6.2): the BrtDXF records between
//! BrtBeginDXFs and BrtEndDXFs, each an XFProps list ([MS-XLSB] 2.5.164) of XFProp entries
//! ([MS-XLSB] 2.5.159). They are mapped the way the .xlsx reader maps `<dxf>`; property types this
//! import doesn't use (alignment, gradients, number-format strings) are ignored.

use std::collections::HashMap;

use gridcraft_core::color::INDEXED;
use gridcraft_model::style::Protection;
use gridcraft_model::{BorderLine, BorderStyle, Color, Fill, Font, NumFmt, PatternType, Style, Underline, VertAlign};

use super::{Budget, Bytes, Entry, MAX_STYLE_RECORDS, MAX_TEXT, Package, Records, Rel, STYLES, check_type, invalid, required};
use crate::{IoError, ReadReport};

/// Most XFProp entries one BrtDXF may carry: one per property type.
const MAX_PROPS: u16 = 64;

/// The workbook's differential formats in BrtDXF order (a CF rule's `dxfId` indexes this list).
/// A damaged styles part yields no formats and one warning; values still import.
pub(super) fn import(package: &mut Package<'_>, types: &[Entry], rels: &[Rel], budget: &mut Budget, report: &mut ReadReport) -> Vec<Style> {
    let mut rels = rels.iter().filter(|r| r.kind == "styles");
    let Some(rel) = rels.next() else { return vec![] };
    let text_before = budget.text;
    let mut parse = || -> Result<_, IoError> {
        if rel.external || rels.next().is_some() {
            return Err(invalid("external or multiple styles relationships"));
        }
        check_type(types, &rel.target, STYLES)?;
        read(&required(package, &rel.target)?, budget)
    };
    match parse() {
        Ok((dxfs, skipped)) => {
            if skipped > 0 {
                report.warn(format!("{skipped} damaged XLSB differential format(s) were imported without formatting."));
            }
            dxfs
        }
        Err(e) => {
            budget.text = text_before;
            report.warn(format!("XLSB differential formats could not be imported; conditional formats use no formatting: {e}"));
            vec![]
        }
    }
}

/// The BrtDXF list of a styles part, and how many damaged entries were kept as empty formats so
/// later indices stay aligned.
fn read(bytes: &[u8], budget: &mut Budget) -> Result<(Vec<Style>, usize), IoError> {
    let mut records = Records::new(bytes);
    let mut ignored_report = ReadReport::default();
    records.first(278, &mut ignored_report)?; // BrtBeginStyleSheet
    let mut formats: HashMap<u16, String> = HashMap::new();
    let mut dxfs = vec![];
    let mut skipped = 0;
    let mut in_fmts = false;
    let mut in_dxfs = false;
    let mut count = 0usize;
    while let Some((id, payload)) = records.next(&mut ignored_report)? {
        match id {
            279 => return Ok((dxfs, skipped)), // BrtEndStyleSheet
            615 => in_fmts = true,             // BrtBeginFmts
            616 => in_fmts = false,
            505 => in_dxfs = true, // BrtBeginDXFs
            506 => in_dxfs = false,
            44 if in_fmts => {
                count += 1;
                if count > MAX_STYLE_RECORDS {
                    return Err(invalid("too many styles records"));
                }
                let mut b = Bytes::new(payload);
                let ifmt = b.u16()?;
                let code = b.text(MAX_TEXT.saturating_sub(budget.text) as u32)?;
                budget.text(code.len())?;
                formats.insert(ifmt, code);
            }
            507 if in_dxfs => {
                count += 1;
                if count > MAX_STYLE_RECORDS {
                    return Err(invalid("too many styles records"));
                }
                match dxf(payload, &formats, budget) {
                    Ok(style) => dxfs.push(style),
                    Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
                    Err(_) => {
                        skipped += 1;
                        dxfs.push(Style::default());
                    }
                }
            }
            _ => {}
        }
    }
    Err(invalid("missing styles end record"))
}

/// What a differential fill states: the pattern and its two colours, each optional.
#[derive(Default)]
struct FillProps {
    pattern: Option<u8>,
    fg: Option<Color>,
    bg: Option<Color>,
}

/// BrtDXF ([MS-XLSB] 2.4.356): 2 flag bytes, then XFProps (2 reserved bytes, the property count).
fn dxf(payload: &[u8], formats: &HashMap<u16, String>, budget: &mut Budget) -> Result<Style, IoError> {
    let mut b = Bytes::new(payload);
    b.u16()?; // unused bits and fNewBorder
    b.u16()?; // reserved
    let cprops = b.u16()?;
    if cprops > MAX_PROPS {
        return Err(invalid("too many differential format properties"));
    }
    let mut s = Style::default();
    let mut font = Font::default();
    let mut stated = Stated::default();
    let mut fill = FillProps::default();
    let mut protection = None;
    let mut diag = (BorderLine::default(), false, false);
    for _ in 0..cprops {
        let kind = b.u16()?;
        let cb = b.u16()?;
        let len = usize::from(cb).checked_sub(4).ok_or_else(|| invalid("short differential format property"))?;
        let blob = b.take(len)?;
        let mut v = Bytes::new(blob);
        match kind {
            0x00 => fill.pattern = Some(v.u8()?),
            0x01 => fill.fg = Some(color(&mut v)?),
            0x02 => fill.bg = Some(color(&mut v)?),
            0x05 => font.color = color(&mut v)?,
            0x06..=0x0A => {
                let line = border(&mut v)?;
                match kind {
                    0x06 => s.borders.top = line,
                    0x07 => s.borders.bottom = line,
                    0x08 => s.borders.left = line,
                    0x09 => s.borders.right = line,
                    _ => diag.0 = line,
                }
            }
            0x0D => diag.1 = number(blob) != 0,
            0x0E => diag.2 = number(blob) != 0,
            0x18 => {
                let n = v.u16()?;
                let size = usize::from(n).checked_mul(2).ok_or_else(|| invalid("font name length overflow"))?;
                let units: Vec<u16> = v.take(size)?.as_chunks::<2>().0.iter().map(|p| u16::from_le_bytes(*p)).collect();
                let name = String::from_utf16(&units).map_err(|_| invalid("invalid font name"))?;
                budget.text(name.len())?;
                if !name.is_empty() {
                    font.name = name;
                    stated.name = true;
                }
            }
            0x19 => {
                font.bold = number(blob) >= 700;
                stated.bold = true;
            }
            0x1A => {
                font.underline = match number(blob) {
                    0x01 => Underline::Single,
                    0x02 => Underline::Double,
                    0x21 => Underline::SingleAccounting,
                    0x22 => Underline::DoubleAccounting,
                    _ => Underline::None,
                };
                stated.underline = true;
            }
            0x1B => {
                font.vert = match number(blob) {
                    1 => VertAlign::Superscript,
                    2 => VertAlign::Subscript,
                    _ => VertAlign::Baseline,
                }
            }
            0x1C => {
                font.italic = number(blob) != 0;
                stated.italic = true;
            }
            0x1D => {
                font.strike = number(blob) != 0;
                stated.strike = true;
            }
            0x22 => font.charset = u8::try_from(number(blob)).ok(),
            0x23 => font.family = u8::try_from(number(blob)).ok().filter(|f| *f <= 5),
            0x24 => {
                let twips = number(blob);
                if (20..=8191).contains(&twips) {
                    font.size = twips as f32 / 20.0;
                    stated.size = true;
                }
            }
            0x25 => {
                stated.scheme = match number(blob) {
                    1 => Some("major".into()),
                    2 => Some("minor".into()),
                    _ => Some("none".into()),
                }
            }
            0x29 => {
                let ifmt = v.u16()?;
                let code = if ifmt < 164 { crate::tables::builtin_format(u32::from(ifmt)).map(str::to_string) } else { formats.get(&ifmt).cloned() };
                s.num_fmt = NumFmt::new(code.as_deref().unwrap_or("General"));
            }
            0x2B => protection.get_or_insert_with(Protection::default).locked = number(blob) != 0,
            0x2C => protection.get_or_insert_with(Protection::default).hidden = number(blob) != 0,
            // Alignment, gradients, number-format strings and the other font flags are not imported.
            _ => {}
        }
    }
    b.finish()?;
    if diag.1 {
        s.borders.diag_up = diag.0;
    }
    if diag.2 {
        s.borders.diag_down = diag.0;
    }
    s.fill = fill_of(fill);
    // As in the .xlsx reader: a stated part is recorded only where it matches the defaults.
    let def = Font::default();
    let st = &mut s.dxf_stated;
    st.bold = stated.bold && !font.bold;
    st.italic = stated.italic && !font.italic;
    st.strike = stated.strike && !font.strike;
    st.underline = stated.underline && font.underline == Underline::None;
    st.size = stated.size && font.size == def.size;
    st.name = stated.name && font.name == def.name;
    st.scheme = stated.scheme;
    s.font = Font { minor: s.font.minor, major: s.font.major, ..font };
    if let Some(p) = protection {
        s.protection = p;
        s.dxf_stated.protection = p == Protection::default();
    }
    Ok(s)
}

#[derive(Default)]
struct Stated {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    size: bool,
    name: bool,
    scheme: Option<String>,
}

/// A little-endian unsigned value of 1 to 4 bytes (the size comes from the XFProp's `cb`).
fn number(blob: &[u8]) -> u32 {
    blob.iter().take(4).rev().fold(0, |n, &b| (n << 8) | u32::from(b))
}

/// The fill a differential format states, mapped like the .xlsx reader's differential fills: a
/// solid (or unstated) pattern's colour is the background colour, else the foreground one.
fn fill_of(f: FillProps) -> Fill {
    let fg = f.fg.unwrap_or(Color::Auto);
    let bg = f.bg.unwrap_or(Color::Auto);
    match f.pattern {
        None | Some(1) => {
            let c = if bg != Color::Auto { bg } else { fg };
            if c == Color::Auto && f.pattern.is_none() { Fill::default() } else { Fill::solid(c) }
        }
        Some(p) => {
            let pattern = match p {
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
                _ => return Fill::default(),
            };
            Fill { pattern, fg, bg }
        }
    }
}

/// XFPropColor ([MS-XLSB] 2.5.161) and BrtColor ([MS-XLSB] 2.4.337) share this 8-byte layout:
/// type bits, index, tint (-32767..32767 for -1..1), then red, green, blue and alpha.
pub(super) fn color(b: &mut Bytes<'_>) -> Result<Color, IoError> {
    let kind = b.u8()? >> 1;
    let index = b.u8()?;
    let tint = i16::from_le_bytes(b.u16()?.to_le_bytes());
    let (r, g, bl) = (b.u8()?, b.u8()?, b.u8()?);
    b.u8()?; // alpha
    Ok(match kind {
        1 => INDEXED.get(usize::from(index)).map_or(Color::Auto, |rgb| Color::Rgb(*rgb)),
        2 => Color::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(bl)),
        3 if index <= 11 => Color::Theme(index, (f64::from(tint) * 1000.0 / 32767.0).round().clamp(-1000.0, 1000.0) as i16),
        _ => Color::Auto,
    })
}

/// XFPropBorder ([MS-XLSB] 2.5.160): a colour and a BorderStyle ([MS-XLSB] 2.5.8).
fn border(b: &mut Bytes<'_>) -> Result<BorderLine, IoError> {
    let color = color(b)?;
    let style = match b.u16()? {
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
        _ => return Ok(BorderLine::default()),
    };
    Ok(BorderLine { style, color })
}
