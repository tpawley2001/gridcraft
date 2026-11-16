//! EMF text records ([MS-EMF] EMR_EXTCREATEFONTINDIRECTW, EMR_EXTTEXTOUTA/W, EMR_SETTEXTALIGN,
//! EMR_SETTEXTCOLOR, EMR_SETBKMODE, EMR_SETBKCOLOR).
//!
//! A run becomes one [`Item::Text`] at its baseline-left point. Font metrics are not available
//! here, so Arial's ascent (0.905 em) and descent (0.212 em) stand in for every face when an
//! alignment refers to the top or bottom of the text cell; that is close enough for print.

use super::{Interp, Item, Obj, colorref, i32_at, point_at, u16_at, u32_at};

/// Ascent and descent of the text cell, in ems (Arial's).
const ASCENT: f64 = 0.905;
const DESCENT: f64 = 0.212;
/// A positive LOGFONT height is the cell height (em plus internal leading).
const CELL_PER_EM: f64 = 1.15;
/// The em height of the default (stock) font, in logical units.
const DEFAULT_EM: f64 = 12.0;

const TA_UPDATECP: u32 = 1;
const TA_RIGHT: u32 = 2;
const TA_CENTER: u32 = 6;
const TA_BOTTOM: u32 = 8;
const TA_BASELINE: u32 = 24;
const ETO_OPAQUE: u32 = 0x02;
const ETO_GLYPH_INDEX: u32 = 0x10;
const ETO_PDY: u32 = 0x2000;

/// The parts of a LOGFONT that print uses.
#[derive(Debug, Clone)]
pub(super) struct LogFont {
    /// LOGFONT height: negative = em height, positive = cell height, 0 = default.
    height: i32,
    /// Tenths of a degree, counter-clockwise.
    escapement: i32,
    bold: bool,
    italic: bool,
    face: String,
}

impl Default for LogFont {
    fn default() -> Self {
        LogFont { height: -(DEFAULT_EM as i32), escapement: 0, bold: false, italic: false, face: "Arial".to_string() }
    }
}

impl LogFont {
    /// The em height in logical units.
    fn em(&self) -> f64 {
        match self.height {
            0 => DEFAULT_EM,
            h if h < 0 => f64::from(h).abs(),
            h => f64::from(h) / CELL_PER_EM,
        }
    }
    fn pdf_font(&self) -> gridcraft_pdf::Font {
        gridcraft_pdf::Font::styled(gridcraft_pdf::Font::family_for(&self.face), self.bold, self.italic)
    }
}

/// Text state of the device context.
#[derive(Debug, Clone)]
pub(super) struct TextState {
    pub(super) font: LogFont,
    color: [u8; 3],
    align: u32,
    /// Background mode OPAQUE (fills the text cell) rather than TRANSPARENT.
    opaque: bool,
    bk_color: [u8; 3],
}

impl Default for TextState {
    fn default() -> Self {
        TextState { font: LogFont::default(), color: [0, 0, 0], align: 0, opaque: true, bk_color: [255, 255, 255] }
    }
}

/// Decodes Windows-1252 bytes.
fn windows_1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}',
        '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}',
        '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => HIGH.get(usize::from(b - 0x80)).copied().unwrap_or('\u{FFFD}'),
            _ => char::from(b),
        })
        .collect()
}

impl Interp {
    /// Text state and text output records.
    pub(super) fn text_record(&mut self, ty: u32, r: &[u8]) -> Option<()> {
        match ty {
            18 => self.dc.text.opaque = u32_at(r, 8)? != 1,
            22 => self.dc.text.align = u32_at(r, 8)?,
            24 => self.dc.text.color = colorref(u32_at(r, 8)?),
            25 => self.dc.text.bk_color = colorref(u32_at(r, 8)?),
            82 => {
                let ih = u32_at(r, 8)?;
                let height = i32_at(r, 12)?;
                let escapement = i32_at(r, 20)?;
                let weight = i32_at(r, 28)?;
                let italic = *r.get(32)? != 0;
                let units: Vec<u16> = (0..32).map_while(|i| u16_at(r, 40 + 2 * i)).take_while(|u| *u != 0).collect();
                let face = String::from_utf16_lossy(&units).trim().to_string();
                let face = if face.is_empty() { "Arial".to_string() } else { face };
                self.objects.insert(ih, Obj::Font(LogFont { height, escapement, bold: weight >= 600, italic, face }));
            }
            83 | 84 => self.text_out(ty == 84, r)?,
            _ => {}
        }
        Some(())
    }

    /// EMR_EXTTEXTOUTW (`wide`) or EMR_EXTTEXTOUTA.
    fn text_out(&mut self, wide: bool, r: &[u8]) -> Option<()> {
        if self.building {
            return None;
        }
        let reference = point_at(r, 36)?;
        let n = u32_at(r, 44)? as usize;
        let off_string = u32_at(r, 48)? as usize;
        let options = u32_at(r, 52)?;
        let off_dx = u32_at(r, 72)? as usize;
        if options & ETO_GLYPH_INDEX != 0 {
            return None;
        }
        let raw = r.get(off_string..off_string.checked_add(n.checked_mul(if wide { 2 } else { 1 })?)?)?;
        let decoded = if wide {
            let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
            String::from_utf16_lossy(&units)
        } else {
            windows_1252(raw)
        };
        if options & ETO_OPAQUE != 0 {
            let rect = [i32_at(r, 56)?, i32_at(r, 60)?, i32_at(r, 64)?, i32_at(r, 68)?].map(f64::from);
            if rect[2] != rect[0] && rect[3] != rect[1] {
                let ring = self.map(&[[rect[0], rect[1]], [rect[2], rect[1]], [rect[2], rect[3]], [rect[0], rect[3]]]);
                self.emit(vec![ring], true, Some(self.dc.text.bk_color), None);
            }
        }
        let text: String = decoded.chars().filter(|c| u32::from(*c) >= 0x20 && *c != '\u{7F}').collect();
        let font = self.dc.text.font.clone();
        let em = font.em();
        // The run's advance in logical units: the per-character advances when present (x and y
        // pairs under ETO_PDY), else the mapped print font's width.
        let per = if options & ETO_PDY != 0 { 2 } else { 1 };
        let advances = (off_dx != 0)
            .then(|| (0..n).map(|i| i32_at(r, off_dx.checked_add(i.checked_mul(4 * per)?)?).map(f64::from)).sum::<Option<f64>>())
            .flatten();
        let width = advances.unwrap_or_else(|| f64::from(gridcraft_pdf::text_width(font.pdf_font(), em as f32, &text)));
        let align = self.dc.text.align;
        let update_cp = align & TA_UPDATECP != 0;
        let start = if update_cp { self.dc.pos } else { reference };
        let angle = f64::from(font.escapement) / 10.0;
        let (sin, cos) = angle.to_radians().sin_cos();
        let t = self.dc.transform();
        let horizontal = align & TA_CENTER;
        if update_cp {
            // The current position moves past the run (back over it when right-aligned).
            let step = match horizontal {
                TA_CENTER => 0.0,
                TA_RIGHT => -width,
                _ => width,
            };
            let y_down = if t[3] < 0.0 { -1.0 } else { 1.0 };
            self.dc.pos = [start[0] + step * cos, start[1] - step * sin * y_down];
        }
        if text.is_empty() {
            return Some(());
        }
        // Device units: the reference point, the em height (a vertical distance) and the width.
        let p = [start[0] * t[0] + start[1] * t[2] + t[4], start[0] * t[1] + start[1] * t[3] + t[5]];
        let em_dev = em * t[2].hypot(t[3]);
        let width_dev = width * t[0].hypot(t[1]);
        // Escapement is counter-clockwise on the device (y down): along the baseline, and up
        // towards the ascent.
        let along = [cos, -sin];
        let up = [-sin, -cos];
        let shift = match horizontal {
            TA_CENTER => -width_dev / 2.0,
            TA_RIGHT => -width_dev,
            _ => 0.0,
        };
        let rise = match align & TA_BASELINE {
            TA_BASELINE => 0.0,
            TA_BOTTOM => DESCENT * em_dev,
            _ => -ASCENT * em_dev,
        };
        let origin = [p[0] + along[0] * shift + up[0] * rise, p[1] + along[1] * shift + up[1] * rise];
        if self.dc.text.opaque && options & ETO_OPAQUE == 0 && width_dev > 0.0 {
            // An opaque background fills the text cell behind the run.
            let corner = |a: f64, u: f64| [origin[0] + along[0] * a + up[0] * u, origin[1] + along[1] * a + up[1] * u];
            let cell = [
                corner(0.0, ASCENT * em_dev),
                corner(width_dev, ASCENT * em_dev),
                corner(width_dev, -DESCENT * em_dev),
                corner(0.0, -DESCENT * em_dev),
            ];
            let ring = cell.iter().map(|c| self.device_rel(*c)).collect();
            self.emit(vec![ring], true, Some(self.dc.text.bk_color), None);
        }
        let origin = self.device_rel(origin);
        let size = (em_dev / self.frame[3].abs()) as f32;
        if !(origin.iter().all(|v| v.is_finite()) && size.is_finite() && size > 0.0 && angle.is_finite()) {
            return None;
        }
        self.items.push(Item::Text {
            origin,
            size,
            angle: angle as f32,
            text,
            face: font.face,
            bold: font.bold,
            italic: font.italic,
            color: self.dc.text.color,
        });
        Some(())
    }

    /// A device point relative to the frame.
    fn device_rel(&self, p: [f64; 2]) -> [f32; 2] {
        [((p[0] - self.frame[0]) / self.frame[2]) as f32, ((p[1] - self.frame[1]) / self.frame[3]) as f32]
    }
}

#[cfg(test)]
mod tests;
