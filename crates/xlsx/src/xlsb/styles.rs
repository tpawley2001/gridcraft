//! [MS-XLSB] styles part (2.1.7.50): number formats, fonts, fills, borders and the cell XFs that
//! combine them with alignment and protection. Cell style XFs (BrtBeginCellStyleXFs) are not cell
//! XFs and never contribute to cell style indices; ixfeParent inheritance, dxfs and gradient fills
//! are not imported.

use std::collections::HashMap;

use gridcraft_core::color::INDEXED;
use gridcraft_model::style::Protection;
use gridcraft_model::{
    Alignment, BorderLine, BorderStyle, Borders, Color, Fill, Font, HAlign, NumFmt, PatternType, Style, StyleId, Underline, VAlign, VertAlign,
    Workbook,
};

use super::{Budget, Bytes, MAX_STYLE_RECORDS, MAX_TEXT, Records, invalid};
use crate::{IoError, ReadReport};

const DEFAULT_FONT_FAMILY: u8 = 2;
const MAX_FONT_FAMILY: u8 = 14;

/// A BrtXF from the cell XF list, with its alignment and protection decoded.
struct Xf {
    fmt: u16,
    font: u16,
    fill: u16,
    border: u16,
    align: Alignment,
    protection: Protection,
}

#[derive(Default)]
pub(super) struct Parsed {
    formats: HashMap<u16, String>,
    fonts: Vec<Font>,
    fills: Vec<Fill>,
    borders: Vec<Borders>,
    xfs: Vec<Xf>,
}

fn count(n: &mut usize, what: &str) -> Result<(), IoError> {
    *n += 1;
    if *n > MAX_STYLE_RECORDS {
        return Err(invalid(format!("too many {what} records")));
    }
    Ok(())
}

/// Parses the whole styles part; any damage fails it so the caller can fall back to defaults.
pub(super) fn parse(bytes: &[u8], budget: &mut Budget) -> Result<Parsed, IoError> {
    let mut records = Records::new(bytes);
    let mut ignored_report = ReadReport::default();
    records.first(278, &mut ignored_report)?; // BrtBeginStyleSheet
    let mut out = Parsed::default();
    let mut block = None;
    let (mut fmts, mut fonts, mut fills, mut borders, mut xfs) = (0, 0, 0, 0, 0);
    while let Some((id, payload)) = records.next(&mut ignored_report)? {
        let mut b = Bytes::new(payload);
        match id {
            279 => {
                if block.is_some() {
                    return Err(invalid("incomplete styles block"));
                }
                records.end(payload, &mut ignored_report)?;
                return Ok(out);
            }
            // Begin records of the fills, fonts, borders, formats, cell XF and cell style XF lists.
            603 | 611 | 613 | 615 | 617 | 626 => {
                if block.is_some() {
                    return Err(invalid("nested styles block"));
                }
                block = Some(id);
            }
            604 | 612 | 614 | 616 | 618 | 627 => {
                if block.take() != Some(id - 1) {
                    return Err(invalid("mismatched styles block end"));
                }
            }
            44 => {
                count(&mut fmts, "number format")?;
                if block == Some(615) {
                    let id = b.u16()?;
                    let code = b.text(MAX_TEXT.saturating_sub(budget.text) as u32)?;
                    b.finish()?;
                    budget.text(code.len())?;
                    out.formats.insert(id, code);
                }
            }
            43 => {
                count(&mut fonts, "font")?;
                if block == Some(611) {
                    out.fonts.push(font(&mut b, budget)?);
                }
            }
            45 => {
                count(&mut fills, "fill")?;
                if block == Some(603) {
                    out.fills.push(fill(&mut b)?);
                }
            }
            46 => {
                count(&mut borders, "border")?;
                if block == Some(613) {
                    out.borders.push(border(&mut b)?);
                }
            }
            47 => {
                count(&mut xfs, "XF")?;
                if block == Some(617) {
                    out.xfs.push(xf(&mut b)?);
                }
            }
            _ => {}
        }
    }
    Err(invalid("missing styles end record"))
}

/// One interned style per cell XF, in XF order. Font, fill and border indices past their lists
/// keep the default part.
pub(super) fn build(parsed: Parsed, wb: &mut Workbook) -> Vec<StyleId> {
    parsed
        .xfs
        .iter()
        .map(|xf| {
            let code = if xf.fmt < 164 { crate::tables::builtin_format(u32::from(xf.fmt)) } else { parsed.formats.get(&xf.fmt).map(String::as_str) }
                .unwrap_or("General");
            let style = Style {
                font: parsed.fonts.get(usize::from(xf.font)).cloned().unwrap_or_default(),
                fill: parsed.fills.get(usize::from(xf.fill)).copied().unwrap_or_default(),
                borders: parsed.borders.get(usize::from(xf.border)).copied().unwrap_or_default(),
                align: xf.align,
                num_fmt: if code.eq_ignore_ascii_case("General") { NumFmt::default() } else { NumFmt::new(code) },
                protection: xf.protection,
                ..Style::default()
            };
            if style == Style::default() { StyleId::DEFAULT } else { wb.styles.intern(style) }
        })
        .collect()
}

/// BrtXF (2.4.862).
fn xf(b: &mut Bytes<'_>) -> Result<Xf, IoError> {
    b.u16()?; // ixfeParent
    let fmt = b.u16()?;
    let font = b.u16()?;
    let fill = b.u16()?;
    let border = b.u16()?;
    let trot = b.u8()?;
    let indent = b.u8()?;
    let flags = b.u16()?;
    b.u16()?; // xfGrbitAtr
    let h = match flags & 7 {
        1 => HAlign::Left,
        2 => HAlign::Center,
        3 => HAlign::Right,
        4 => HAlign::Fill,
        5 => HAlign::Justify,
        6 => HAlign::CenterAcross,
        7 => HAlign::Distributed,
        _ => HAlign::General,
    };
    let v = match (flags >> 3) & 7 {
        0 => VAlign::Top,
        1 => VAlign::Center,
        3 => VAlign::Justify,
        4 => VAlign::Distributed,
        _ => VAlign::Bottom,
    };
    let rotation = match trot {
        0..=90 => i16::from(trot),
        91..=180 => -(i16::from(trot) - 90),
        255 => 255,
        _ => 0,
    };
    let align = Alignment { h, v, wrap: flags & 1 << 6 != 0, shrink: flags & 1 << 8 != 0, indent: indent.min(250), rotation };
    let protection = Protection { locked: flags & 1 << 12 != 0, hidden: flags & 1 << 13 != 0 };
    Ok(Xf { fmt, font, fill, border, align, protection })
}

/// BrtColor (2.4.337).
pub(super) fn color(b: &mut Bytes<'_>) -> Result<Color, IoError> {
    let kind = b.u8()? >> 1;
    let index = b.u8()?;
    let tint = i16::from_le_bytes(b.u16()?.to_le_bytes());
    let (r, g, bl) = (b.u8()?, b.u8()?, b.u8()?);
    b.u8()?; // alpha
    Ok(match kind {
        1 => INDEXED.get(usize::from(index)).map_or(Color::Auto, |rgb| Color::Rgb(*rgb)),
        2 => Color::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(bl)),
        3 if index <= 11 => {
            let tint = (f64::from(tint) * 1000.0 / 32767.0).round().clamp(-1000.0, 1000.0) as i16;
            Color::Theme(index, tint)
        }
        _ => Color::Auto,
    })
}

/// BrtFont (2.4.690), mapped like the XLSX reader's fonts.
fn font(b: &mut Bytes<'_>, budget: &mut Budget) -> Result<Font, IoError> {
    let height = b.u16()?;
    let flags = b.u16()?;
    let weight = b.u16()?;
    let script = b.u16()?;
    let underline = b.u8()?;
    let family = b.u8()?;
    let charset = b.u8()?;
    b.u8()?; // unused
    let mut color = color(b)?;
    let scheme = b.u8()?;
    let name = b.text(MAX_TEXT.saturating_sub(budget.text) as u32)?;
    budget.text(name.len())?;
    // Text 1 (dk1) is what "automatic" font colour means in practice.
    if color == Color::Theme(1, 0) {
        color = Color::Auto;
    }
    Ok(Font {
        name,
        size: (f32::from(height) / 20.0).clamp(1.0, 409.0),
        bold: weight >= 700,
        italic: flags & 1 << 1 != 0,
        underline: match underline {
            1 => Underline::Single,
            2 => Underline::Double,
            0x21 => Underline::SingleAccounting,
            0x22 => Underline::DoubleAccounting,
            _ => Underline::None,
        },
        strike: flags & 1 << 3 != 0,
        color,
        vert: match script {
            1 => VertAlign::Superscript,
            2 => VertAlign::Subscript,
            _ => VertAlign::Baseline,
        },
        charset: Some(charset).filter(|c| *c > 1),
        family: Some(family).filter(|f| *f != 0 && *f != DEFAULT_FONT_FAMILY && *f <= MAX_FONT_FAMILY),
        minor: scheme == 2,
    })
}

/// BrtFill (2.4.681); the gradient fields after the two colours are not imported.
fn fill(b: &mut Bytes<'_>) -> Result<Fill, IoError> {
    let fls = b.u32()?;
    let fg = color(b)?;
    let bg = color(b)?;
    let pattern = match fls {
        1 => return Ok(Fill::solid(fg)),
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
        _ => return Ok(Fill::default()),
    };
    Ok(Fill { pattern, fg, bg })
}

/// Blxf (2.5.5).
fn line(b: &mut Bytes<'_>) -> Result<BorderLine, IoError> {
    let style = match b.u8()? {
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
    };
    b.u8()?; // reserved
    Ok(BorderLine { style, color: color(b)? })
}

/// BrtBorder (2.4.314): diagonal flags, then top, bottom, left, right and diagonal lines.
fn border(b: &mut Bytes<'_>) -> Result<Borders, IoError> {
    let flags = b.u8()?;
    let (top, bottom, left, right, diag) = (line(b)?, line(b)?, line(b)?, line(b)?, line(b)?);
    let pick = |bit: u8| if flags & bit != 0 { diag } else { BorderLine::default() };
    Ok(Borders { left, right, top, bottom, diag_down: pick(1), diag_up: pick(2) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_out_of_range_are_automatic_and_tints_clamp() {
        let read = |bytes: [u8; 8]| color(&mut Bytes::new(&bytes)).unwrap();
        assert_eq!(read([1 << 1, 64, 0, 0, 0, 0, 0, 0]), Color::Auto);
        assert_eq!(read([3 << 1, 12, 0, 0, 0, 0, 0, 0]), Color::Auto);
        assert_eq!(read([3 << 1, 4, 0x00, 0x80, 0, 0, 0, 0]), Color::Theme(4, -1000));
        assert_eq!(read([9 << 1, 0, 0, 0, 1, 2, 3, 0]), Color::Auto);
        assert!(color(&mut Bytes::new(&[2 << 1, 0, 0])).is_err());
    }

    #[test]
    fn gradient_and_unknown_fills_are_default_and_short_borders_fail() {
        let mut gradient = 0x28u32.to_le_bytes().to_vec();
        gradient.extend([0; 16]);
        assert_eq!(fill(&mut Bytes::new(&gradient)).unwrap(), Fill::default());
        assert!(border(&mut Bytes::new(&[3; 50])).is_err());
    }
}
