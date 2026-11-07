//! Cell formatting in a BIFF8 Workbook stream, from [MS-XLS]: Font (2.4.122), XF (2.4.353) with
//! CellXF (2.5.91), Palette (2.4.188), Style (2.4.269), XFExt (2.4.355) with FullColorExt
//! (2.5.155) and Theme (2.4.326). A BIFF5 (Excel 5.0/95) Font differs only in its 8-bit name,
//! and its 16-byte XF packs the same fields differently; it is translated into the BIFF8
//! CellXF layout so both versions share one decoder. Every XF is
//! decoded on its own: a cell XF stores its complete formatting (the fAtr* bits only say whether
//! later changes to the parent style propagate), so nothing is inherited from `ixfParent`.

use std::collections::HashMap;

use gridcraft_model::style::Protection;
use gridcraft_model::{
    Alignment, BorderLine, BorderStyle, Borders, Color, Fill, Font, HAlign, NumFmt, PatternType, Style, StyleId, StyleTable, Theme, Underline,
    VAlign, VertAlign, Workbook,
};

use super::invalid;
use super::records::Cursor;
use crate::IoError;
use crate::package::Package;
use crate::tables::{INDEXED, builtin_format};

/// Font records kept at most ([MS-XLS] FontIndex allows indexes up to 1022).
const MAX_FONTS: usize = 1024;
/// Style records kept at most.
const MAX_STYLES: usize = 65_536;
/// Characters kept from a style name (Excel allows 255).
const MAX_STYLE_NAME: usize = 255;

/// The automatic font colour (IcvFont 0x7FFF) and the system foreground/background colours.
const ICV_FONT_AUTO: u16 = 0x7FFF;
const ICV_DEFAULT_FG: u16 = 0x40;
const ICV_DEFAULT_BG: u16 = 0x41;

/// XFExt colours replacing an XF's palette colours ([MS-XLS] 2.5.108 ExtPropType values).
#[derive(Clone, Copy, Debug)]
enum ExtColor {
    /// A colour-table index, resolved once the palette is known.
    Indexed(u16),
    Color(Color),
}

impl ExtColor {
    fn resolve(self, table: &[u32; 64]) -> Color {
        match self {
            ExtColor::Indexed(icv) => color(icv, table),
            ExtColor::Color(c) => c,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct XfExt {
    fill_fg: Option<ExtColor>,
    fill_bg: Option<ExtColor>,
    top: Option<ExtColor>,
    bottom: Option<ExtColor>,
    left: Option<ExtColor>,
    right: Option<ExtColor>,
    diag: Option<ExtColor>,
    text: Option<ExtColor>,
}

/// The Theme record's data (an OPC package) gathered with its ContinueFrt12 records.
#[derive(Debug, Default)]
pub(super) struct ThemeData {
    /// dwThemeVersion: 0 for a custom theme stored in the data.
    version: u32,
    bytes: Vec<u8>,
    /// More than `MAX_THEME` bytes, or a record too short for its header.
    bad: bool,
}

/// The `dwThemeVersion` of the built-in Office theme, which stores no data.
const THEME_BUILTIN: u32 = 124_226;
/// Bytes of theme package kept at most.
const MAX_THEME: usize = 16 * 1024 * 1024;
/// Largest theme part read from the package.
const MAX_THEME_PART: u64 = 1024 * 1024;
/// Length of FrtHeader (2.5.135).
const FRT_HEADER: usize = 12;

/// One XF record, decoded as far as its bytes allow.
#[derive(Clone, Debug, Default)]
pub(super) struct Xf {
    ifnt: u16,
    ifmt: u16,
    flags: u16,
    /// The CellXF/StyleXF data: 14 bytes when complete.
    data: [u8; 14],
    /// Whether all 20 bytes were present.
    complete: bool,
}

/// A Style record: the XF it names and its name.
#[derive(Clone, Debug)]
pub(super) struct NamedStyle {
    ixf: u16,
    /// `Some(id)` for a built-in style (BuiltInStyle.istyBuiltIn), `None` for a user style.
    builtin: Option<u8>,
    /// A user style's name (empty for built-in styles).
    name: String,
}

/// Formatting records collected from the globals substream.
#[derive(Debug, Default)]
pub(super) struct StyleRecords {
    fonts: Vec<Font>,
    font_colors: Vec<u16>,
    /// Raw height and weight for shape text, before cell-font normalisation.
    font_metrics: Vec<(u16, u16)>,
    xfs: Vec<Xf>,
    /// Palette entries for colour indexes 8 and up (RRGGBB), when a Palette record is present.
    palette: Option<Vec<u32>>,
    styles: Vec<NamedStyle>,
    /// XFExt colours by the index of the XF they extend.
    exts: HashMap<u16, XfExt>,
    /// An XFExt record that was malformed or names a missing XF.
    pub(super) bad_ext: bool,
    /// The Theme record, when present.
    pub(super) theme: Option<ThemeData>,
    /// Records that were shorter than their structure; the parts present were used.
    pub(super) short: bool,
}

impl StyleRecords {
    /// A Font record. The colour is resolved later, once the palette is known.
    pub(super) fn font(&mut self, c: &mut Cursor<'_, '_>) {
        self.push_font(c, |c| c.short_string().ok());
    }

    /// A BIFF5 Font record: the BIFF8 fields, then a 1-byte count and 8-bit characters that
    /// `decode` turns into text.
    pub(super) fn font_biff5(&mut self, c: &mut Cursor<'_, '_>, decode: impl Fn(&[u8]) -> String) {
        self.push_font(c, |c| {
            let cch = usize::from(c.u8().ok()?);
            Some(decode(&c.bytes(cch).ok()?))
        });
    }

    fn push_font(&mut self, c: &mut Cursor<'_, '_>, name: impl FnOnce(&mut Cursor<'_, '_>) -> Option<String>) {
        if self.fonts.len() >= MAX_FONTS {
            return;
        }
        match read_font(c, name) {
            Some((font, icv, height, weight)) => {
                self.font_metrics.push((height, weight));
                self.fonts.push(font);
                self.font_colors.push(icv);
            }
            None => {
                // Keep the numbering of the fonts after it.
                self.short = true;
                self.font_metrics.push((220, 400));
                self.fonts.push(Font::default());
                self.font_colors.push(ICV_FONT_AUTO);
            }
        }
    }

    /// Parsed FONT records for shape text; colours remain indexes until the palette is known.
    pub(super) fn drawing_fonts(&self) -> Vec<(Font, u16)> {
        self.fonts
            .iter()
            .zip(&self.font_colors)
            .zip(&self.font_metrics)
            .map(|((font, icv), (height, weight))| (Font { size: f32::from(*height) / 20.0, bold: *weight >= 700, ..font.clone() }, *icv))
            .collect()
    }

    /// An XF record. A short record keeps the fields it has.
    pub(super) fn xf(&mut self, c: &mut Cursor<'_, '_>, max: usize) {
        if self.xfs.len() >= max {
            return;
        }
        let mut xf = Xf::default();
        let head = (|| Some((c.u16().ok()?, c.u16().ok()?, c.u16().ok()?)))();
        match head {
            Some((ifnt, ifmt, flags)) => {
                xf.ifnt = ifnt;
                xf.ifmt = ifmt;
                xf.flags = flags;
                xf.complete = true;
                for b in xf.data.iter_mut() {
                    match c.u8() {
                        Ok(v) => *b = v,
                        Err(_) => {
                            xf.complete = false;
                            break;
                        }
                    }
                }
            }
            None => {
                // Not even the font and format: an unformatted, locked cell.
                xf.flags = 1;
            }
        }
        if !xf.complete {
            self.short = true;
        }
        self.xfs.push(xf);
    }

    /// A BIFF5 XF record (16 bytes), translated into the BIFF8 CellXF layout. A short record
    /// keeps the font, format and flags it has.
    pub(super) fn xf_biff5(&mut self, c: &mut Cursor<'_, '_>, max: usize) {
        if self.xfs.len() >= max {
            return;
        }
        let mut xf = Xf { flags: 1, ..Xf::default() };
        let head = (|| Some((c.u16().ok()?, c.u16().ok()?, c.u16().ok()?)))();
        if let Some((ifnt, ifmt, flags)) = head {
            xf.ifnt = ifnt;
            xf.ifmt = ifmt;
            xf.flags = flags;
            let body = (|| Some((c.u16().ok()?, c.u32().ok()?, c.u32().ok()?)))();
            if let Some((align, fill_bottom, sides)) = body {
                xf.data = biff5_xf_data(align, fill_bottom, sides);
                xf.complete = true;
            }
        }
        if !xf.complete {
            self.short = true;
        }
        self.xfs.push(xf);
    }

    /// A Palette record: colours for indexes 8 onwards.
    pub(super) fn palette(&mut self, c: &mut Cursor<'_, '_>) {
        let Ok(ccv) = c.u16() else {
            self.short = true;
            return;
        };
        let n = usize::from(ccv).min(56);
        if usize::from(ccv) > c.remaining() / 4 {
            self.short = true;
        }
        let mut colors = Vec::with_capacity(n);
        for _ in 0..n {
            let (Ok(r), Ok(g), Ok(b), Ok(_)) = (c.u8(), c.u8(), c.u8(), c.u8()) else { break };
            colors.push((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
        }
        self.palette = Some(colors);
    }

    /// A Style record.
    pub(super) fn style(&mut self, c: &mut Cursor<'_, '_>) {
        if self.styles.len() >= MAX_STYLES {
            return;
        }
        let Ok(word) = c.u16() else {
            self.short = true;
            return;
        };
        let ixf = word & 0x0FFF;
        if word & 0x8000 != 0 {
            let Ok(id) = c.u8() else {
                self.short = true;
                return;
            };
            // BuiltInStyle: the style's number (0 is Normal) and outline level.
            self.styles.push(NamedStyle { ixf, builtin: Some(id), name: String::new() });
        } else {
            match c.string() {
                Ok(name) => {
                    let name: String = name.chars().filter(|ch| *ch != '\0').take(MAX_STYLE_NAME).collect();
                    if !name.is_empty() {
                        self.styles.push(NamedStyle { ixf, builtin: None, name });
                    }
                }
                Err(_) => self.short = true,
            }
        }
    }

    /// An XFExt record: full-fidelity colours of the XF `ixfe`. Malformed records are dropped.
    pub(super) fn xf_ext(&mut self, c: &mut Cursor<'_, '_>) {
        if self.exts.len() >= MAX_STYLES {
            return;
        }
        match read_xf_ext(c) {
            Some((ixfe, ext)) => {
                self.exts.insert(ixfe, ext);
            }
            None => self.bad_ext = true,
        }
    }

    /// A Theme record: starts the theme data.
    pub(super) fn theme(&mut self, c: &mut Cursor<'_, '_>) {
        let mut t = ThemeData::default();
        match (c.skip(FRT_HEADER), c.u32()) {
            (Ok(()), Ok(version)) => {
                t.version = version;
                t.append(c.rest());
            }
            _ => t.bad = true,
        }
        self.theme = Some(t);
    }

    /// A ContinueFrt12 record following the Theme record: more theme data.
    pub(super) fn theme_continue(&mut self, c: &mut Cursor<'_, '_>) {
        let Some(t) = self.theme.as_mut() else { return };
        if c.skip(FRT_HEADER).is_err() {
            t.bad = true;
            return;
        }
        t.append(c.rest());
    }

    /// The 64-entry colour table: the built-in eight, then the palette (or the default one).
    pub(super) fn color_table(&self) -> [u32; 64] {
        let mut table = INDEXED;
        if let Some(p) = &self.palette {
            for (slot, c) in table.iter_mut().skip(8).zip(p.iter()) {
                *slot = *c;
            }
        }
        table
    }
}

impl ThemeData {
    fn append(&mut self, data: Vec<u8>) {
        if self.bytes.len().saturating_add(data.len()) > MAX_THEME {
            self.bad = true;
            return;
        }
        self.bytes.extend_from_slice(&data);
    }

    /// The workbook theme: `Ok(None)` for the built-in Office theme, an error when the data
    /// can't be read.
    pub(super) fn read(&self) -> Result<Option<Theme>, IoError> {
        if self.bad {
            return Err(invalid("theme data too large or truncated"));
        }
        if self.version == THEME_BUILTIN && self.bytes.is_empty() {
            return Ok(None);
        }
        let mut package = Package::open(&self.bytes)?;
        let part = theme_part(&mut package)?.ok_or_else(|| invalid("missing theme part"))?;
        let data = package.read_limited(&part, MAX_THEME_PART)?.ok_or_else(|| invalid("missing theme part"))?;
        let xml = crate::xml::parse(&data)?;
        Ok(Some(crate::theme::read_theme(&xml)))
    }
}

/// The theme part of a theme package: the package's own theme relationship, else the theme
/// relationship of the part it points at (the theme manager part), else the usual part name.
fn theme_part(package: &mut Package<'_>) -> Result<Option<String>, IoError> {
    let root = package.rels("")?;
    if let Some(rel) = root.iter().find(|r| r.kind == "theme" && !r.external) {
        return Ok(Some(rel.target.clone()));
    }
    for rel in root.iter().filter(|r| !r.external) {
        if let Some(theme) = package.rels(&rel.target)?.into_iter().find(|r| r.kind == "theme" && !r.external) {
            return Ok(Some(theme.target));
        }
    }
    Ok(package.has("theme/theme/theme1.xml").then(|| "theme/theme/theme1.xml".to_string()))
}

/// XFExt fields: the XF index and the colours of the ExtProp entries GridCraft keeps.
fn read_xf_ext(c: &mut Cursor<'_, '_>) -> Option<(u16, XfExt)> {
    c.skip(FRT_HEADER + 2).ok()?;
    let ixfe = c.u16().ok()?;
    c.skip(2).ok()?;
    let cexts = c.u16().ok()?;
    let mut ext = XfExt::default();
    for _ in 0..cexts {
        let ty = c.u16().ok()?;
        let cb = usize::from(c.u16().ok()?);
        // cb counts the whole ExtProp, its four-byte head included.
        let data_len = cb.checked_sub(4)?;
        if data_len > c.remaining() {
            return None;
        }
        let slot = match ty {
            0x04 => &mut ext.fill_fg,
            0x05 => &mut ext.fill_bg,
            0x07 => &mut ext.top,
            0x08 => &mut ext.bottom,
            0x09 => &mut ext.left,
            0x0A => &mut ext.right,
            0x0B => &mut ext.diag,
            0x0D => &mut ext.text,
            _ => {
                c.skip(data_len).ok()?;
                continue;
            }
        };
        if data_len < 8 {
            return None;
        }
        let kind = c.u16().ok()?;
        let tint = i16::from_le_bytes(c.u16().ok()?.to_le_bytes());
        let value = c.u32().ok()?;
        c.skip(data_len - 8).ok()?;
        *slot = full_color(kind, tint, value);
    }
    Some((ixfe, ext))
}

/// A FullColorExt colour (2.5.155): `None` for automatic and unset, so the XF's colour stays.
/// Indexed colours are resolved with the palette when the styles are built.
fn full_color(kind: u16, tint: i16, value: u32) -> Option<ExtColor> {
    match kind {
        // XColorType: indexed; kept as an index until the palette is known.
        1 => Some(ExtColor::Indexed(value.min(u32::from(u16::MAX)) as u16)),
        // LongRGBA: red, green, blue, reserved bytes.
        2 => {
            let [r, g, b, _] = value.to_le_bytes();
            Some(ExtColor::Color(Color::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))))
        }
        3 if value <= 11 => {
            let tint = (f64::from(tint) * 1000.0 / 32767.0).round().clamp(-1000.0, 1000.0) as i16;
            Some(ExtColor::Color(Color::Theme(value as u8, tint)))
        }
        _ => None,
    }
}

/// Font record fields up to the name; `None` if the record is too short for them.
fn read_font(c: &mut Cursor<'_, '_>, name: impl FnOnce(&mut Cursor<'_, '_>) -> Option<String>) -> Option<(Font, u16, u16, u16)> {
    let height = c.u16().ok()?;
    let flags = c.u16().ok()?;
    let icv = c.u16().ok()?;
    let weight = c.u16().ok()?;
    let script = c.u16().ok()?;
    let underline = c.u8().ok()?;
    let family = c.u8().ok()?;
    let charset = c.u8().ok()?;
    c.skip(1).ok()?; // reserved
    let name = name(c)?;
    let mut f = Font::default();
    let name: String = name.chars().filter(|ch| *ch != '\0').take(255).collect();
    if !name.trim().is_empty() {
        f.name = name;
    }
    if height > 0 {
        f.size = (f32::from(height) / 20.0).clamp(1.0, 409.0);
    }
    f.italic = flags & 0x0002 != 0;
    f.strike = flags & 0x0008 != 0;
    // Weights run 100-1000; Excel writes 400 for normal and 700 for bold.
    f.bold = weight >= 600;
    f.vert = match script {
        1 => VertAlign::Superscript,
        2 => VertAlign::Subscript,
        _ => VertAlign::Baseline,
    };
    f.underline = match underline {
        0x01 => Underline::Single,
        0x02 => Underline::Double,
        0x21 => Underline::SingleAccounting,
        0x22 => Underline::DoubleAccounting,
        _ => Underline::None,
    };
    f.family = Some(family);
    f.charset = Some(charset);
    Some((f, icv, height, weight))
}

/// The BIFF8 CellXF bytes (offsets 6-19 of a BIFF8 XF) holding a BIFF5 XF's alignment, fill and
/// borders. `align` is offset 6 (alignment, wrap, orientation), `fill_bottom` offset 8 (fill
/// colours and pattern, bottom border) and `sides` offset 12 (top, left and right borders).
fn biff5_xf_data(align: u16, fill_bottom: u32, sides: u32) -> [u8; 14] {
    let mut d = [0u8; 14];
    // Horizontal alignment, wrap, vertical alignment and fJustLast share the BIFF8 bit layout.
    d[0] = (align & 0x00FF) as u8;
    d[1] = match (align >> 8) & 0x03 {
        1 => 255,
        2 => 90,
        3 => 180,
        _ => 0,
    };
    let fore = fill_bottom & 0x7F;
    let back = (fill_bottom >> 7) & 0x7F;
    let fls = (fill_bottom >> 16) & 0x3F;
    let dg_bottom = (fill_bottom >> 22) & 0x07;
    let icv_bottom = (fill_bottom >> 25) & 0x7F;
    let dg_top = sides & 0x07;
    let dg_left = (sides >> 3) & 0x07;
    let dg_right = (sides >> 6) & 0x07;
    let icv_top = (sides >> 9) & 0x7F;
    let icv_left = (sides >> 16) & 0x7F;
    let icv_right = (sides >> 23) & 0x7F;
    let b1 = dg_left | dg_right << 4 | dg_top << 8 | dg_bottom << 12 | icv_left << 16 | icv_right << 23;
    let b2 = icv_top | icv_bottom << 7 | fls << 26;
    let fill = (fore | back << 7) as u16;
    d[4..8].copy_from_slice(&b1.to_le_bytes());
    d[8..12].copy_from_slice(&b2.to_le_bytes());
    d[12..14].copy_from_slice(&fill.to_le_bytes());
    d
}

/// A colour index in an XF (IcvXF) or Font (IcvFont): 0-63 from the colour table, the system
/// and automatic colours as automatic.
fn color(icv: u16, table: &[u32; 64]) -> Color {
    match icv {
        ICV_DEFAULT_FG | ICV_DEFAULT_BG | ICV_FONT_AUTO => Color::Auto,
        _ => table.get(usize::from(icv)).map_or(Color::Auto, |c| Color::Rgb(*c)),
    }
}

/// A border line style from its BIFF8 code (dgLeft and the like); unknown codes are no border.
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

/// A fill pattern from its BIFF8 code (fls); unknown codes are no pattern.
fn pattern(fls: u16) -> PatternType {
    match fls {
        0x01 => PatternType::Solid,
        0x02 => PatternType::Gray50,
        0x03 => PatternType::Gray75,
        0x04 => PatternType::Gray25,
        0x05 => PatternType::DarkHorizontal,
        0x06 => PatternType::DarkVertical,
        0x07 => PatternType::DarkDown,
        0x08 => PatternType::DarkUp,
        0x09 => PatternType::DarkGrid,
        0x0A => PatternType::DarkTrellis,
        0x0B => PatternType::LightHorizontal,
        0x0C => PatternType::LightVertical,
        0x0D => PatternType::LightDown,
        0x0E => PatternType::LightUp,
        0x0F => PatternType::LightGrid,
        0x10 => PatternType::LightTrellis,
        0x11 => PatternType::Gray125,
        0x12 => PatternType::Gray0625,
        _ => PatternType::None,
    }
}

/// One border side: its style and colour; a side without a style has no colour.
fn line(dg: u32, icv: u32, table: &[u32; 64]) -> BorderLine {
    let style = border_style(dg);
    if style == BorderStyle::None {
        return BorderLine::default();
    }
    // Colour 0 means "not specified": automatic.
    let color = if icv == 0 { Color::Auto } else { color(icv as u16, table) };
    BorderLine { style, color }
}

/// The font an XF names (FontIndex: index 4 is never written, so 5 and up count from one).
fn xf_font(ifnt: u16, fonts: &[Font]) -> Font {
    let i = match ifnt {
        0..=3 => usize::from(ifnt),
        4 => return fonts.first().cloned().unwrap_or_default(),
        _ => usize::from(ifnt) - 1,
    };
    fonts.get(i).or_else(|| fonts.first()).cloned().unwrap_or_default()
}

/// The number format an XF names: a custom format, else a built-in one, else General.
fn num_fmt(ifmt: u16, formats: &HashMap<u16, String>) -> NumFmt {
    let code = formats.get(&ifmt).map(String::as_str).or_else(|| builtin_format(u32::from(ifmt))).unwrap_or("General");
    if code.eq_ignore_ascii_case("General") { NumFmt::default() } else { NumFmt::new(code) }
}

/// The model style of one XF.
fn xf_style(xf: &Xf, ext: Option<&XfExt>, fonts: &[Font], formats: &HashMap<u16, String>, table: &[u32; 64]) -> Style {
    let mut s = xf_palette_style(xf, fonts, formats, table);
    if let Some(ext) = ext {
        apply_ext(&mut s, ext, table);
    }
    s
}

/// Replaces the palette colours of a style with an XFExt's colours. Borders and fills keep
/// their XF line styles and pattern; only sides that have a line take a colour.
fn apply_ext(s: &mut Style, ext: &XfExt, table: &[u32; 64]) {
    if let Some(c) = ext.text {
        let c = c.resolve(table);
        // Text 1 (dk1) is what "automatic" font colour means in practice.
        s.font.color = if c == Color::Theme(1, 0) { Color::Auto } else { c };
    }
    let sides = [
        (ext.top, &mut s.borders.top),
        (ext.bottom, &mut s.borders.bottom),
        (ext.left, &mut s.borders.left),
        (ext.right, &mut s.borders.right),
        (ext.diag, &mut s.borders.diag_down),
    ];
    for (c, side) in sides {
        if let Some(c) = c
            && side.style != BorderStyle::None
        {
            side.color = c.resolve(table);
        }
    }
    if let Some(c) = ext.diag
        && s.borders.diag_up.style != BorderStyle::None
    {
        s.borders.diag_up.color = c.resolve(table);
    }
    if s.fill.pattern != PatternType::None {
        if let Some(c) = ext.fill_fg {
            s.fill.fg = c.resolve(table);
        }
        // A solid fill shows only its foreground colour.
        if let Some(c) = ext.fill_bg
            && s.fill.pattern != PatternType::Solid
        {
            s.fill.bg = c.resolve(table);
        }
    }
}

/// The model style of one XF from its own (palette) fields.
fn xf_palette_style(xf: &Xf, fonts: &[Font], formats: &HashMap<u16, String>, table: &[u32; 64]) -> Style {
    let mut s = Style {
        font: xf_font(xf.ifnt, fonts),
        num_fmt: num_fmt(xf.ifmt, formats),
        protection: Protection { locked: xf.flags & 0x0001 != 0, hidden: xf.flags & 0x0002 != 0 },
        ..Style::default()
    };
    if !xf.complete {
        return s;
    }
    let d = &xf.data;
    let byte = |i: usize| d.get(i).copied().unwrap_or(0);
    let word32 = |i: usize| u32::from_le_bytes([byte(i), byte(i + 1), byte(i + 2), byte(i + 3)]);
    let (a0, rot, a2) = (byte(0), byte(1), byte(2));
    s.align = Alignment {
        h: match a0 & 0x07 {
            1 => HAlign::Left,
            2 => HAlign::Center,
            3 => HAlign::Right,
            4 => HAlign::Fill,
            5 => HAlign::Justify,
            6 => HAlign::CenterAcross,
            7 => HAlign::Distributed,
            _ => HAlign::General,
        },
        wrap: a0 & 0x08 != 0,
        v: match (a0 >> 4) & 0x07 {
            0 => VAlign::Top,
            1 => VAlign::Center,
            3 => VAlign::Justify,
            4 => VAlign::Distributed,
            _ => VAlign::Bottom,
        },
        rotation: match rot {
            0..=90 => i16::from(rot),
            91..=180 => -(i16::from(rot) - 90),
            255 => 255,
            _ => 0,
        },
        indent: a2 & 0x0F,
        shrink: a2 & 0x10 != 0,
        ..Alignment::default()
    };
    let b1 = word32(4);
    let b2 = word32(8);
    let mut borders = Borders {
        left: line(b1 & 0x0F, (b1 >> 16) & 0x7F, table),
        right: line((b1 >> 4) & 0x0F, (b1 >> 23) & 0x7F, table),
        top: line((b1 >> 8) & 0x0F, b2 & 0x7F, table),
        bottom: line((b1 >> 12) & 0x0F, (b2 >> 7) & 0x7F, table),
        ..Borders::default()
    };
    let diag = line((b2 >> 21) & 0x0F, (b2 >> 14) & 0x7F, table);
    let which = (b1 >> 30) & 0x03;
    if which & 1 != 0 {
        borders.diag_down = diag;
    }
    if which & 2 != 0 {
        borders.diag_up = diag;
    }
    s.borders = borders;
    let fill = u16::from_le_bytes([byte(12), byte(13)]);
    let fls = ((b2 >> 26) & 0x3F) as u16;
    let fg = color(fill & 0x7F, table);
    let bg = color((fill >> 7) & 0x7F, table);
    s.fill = match pattern(fls) {
        PatternType::None => Fill::default(),
        PatternType::Solid => Fill::solid(fg),
        p => Fill { pattern: p, fg, bg },
    };
    s
}

/// The XFExt colours of XF `i`, if any.
fn ext_of(r: &StyleRecords, i: usize) -> Option<&XfExt> {
    u16::try_from(i).ok().and_then(|i| r.exts.get(&i))
}

/// Whether an XFExt record was malformed or extends an XF the workbook doesn't have.
pub(super) fn ext_ignored(r: &StyleRecords) -> bool {
    r.bad_ext || r.exts.keys().any(|i| usize::from(*i) >= r.xfs.len())
}

/// Imports the formatting: sets the workbook's default (Normal) font and named cell styles, and
/// returns the model style of every XF in file order.
pub(super) fn build(r: &StyleRecords, formats: &HashMap<u16, String>, wb: &mut Workbook) -> Vec<StyleId> {
    let table = r.color_table();
    let fonts: Vec<Font> = r.fonts.iter().zip(r.font_colors.iter()).map(|(f, icv)| Font { color: color(*icv, &table), ..f.clone() }).collect();
    // The Normal style's font is what every unformatted cell shows.
    let normal_xf = r.styles.iter().find(|s| s.builtin == Some(0)).map_or(0, |s| usize::from(s.ixf));
    let normal_font = r.xfs.get(normal_xf).map(|xf| xf_font(xf.ifnt, &fonts)).or_else(|| fonts.first().cloned());
    if let Some(font) = normal_font {
        wb.styles = StyleTable::with_default(Style { font, ..Style::default() });
    }
    let out = r.xfs.iter().enumerate().map(|(i, xf)| wb.styles.intern(xf_style(xf, ext_of(r, i), &fonts, formats, &table))).collect();
    // User-defined named styles. Excel 2007 and later write every built-in style into each .xls
    // (about fifty, their colours approximated to the palette); GridCraft defines those itself.
    let mut named: Vec<(String, Style)> = Vec::new();
    // Names are compared ASCII-case-insensitively; a set keeps a hostile file's many names linear.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::from(["normal".to_string()]);
    for ns in &r.styles {
        if ns.builtin.is_some() {
            continue;
        }
        let key = ns.name.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        if let Some(xf) = r.xfs.get(usize::from(ns.ixf)) {
            seen.insert(key);
            named.push((ns.name.clone(), xf_style(xf, ext_of(r, usize::from(ns.ixf)), &fonts, formats, &table)));
        }
    }
    wb.cell_styles = named;
    out
}
