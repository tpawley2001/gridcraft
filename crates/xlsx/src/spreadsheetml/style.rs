//! `Workbook/Styles` of an XML Spreadsheet 2003 file: each `Style` (with its optional `ss:Parent`)
//! becomes a model style, plus the `ss:Type="DateTime"` text to serial conversion.

use std::collections::HashMap;

use gridcraft_core::DateSystem;
use gridcraft_core::date::{days_in_month, serial_from_ymd};
use gridcraft_model::{BorderLine, BorderStyle, Color, Fill, HAlign, NumFmt, Style, StyleId, StyleTable, Underline, VAlign, VertAlign};

use crate::xml::El;

/// Deepest `ss:Parent` chain followed; longer chains (or cycles) stop inheriting there.
const MAX_PARENTS: usize = 32;

/// The file's styles by `ss:ID`, interned into the workbook's style table.
pub(super) struct Styles {
    pub table: StyleTable,
    ids: HashMap<String, StyleId>,
}

impl Styles {
    /// Reads `Workbook/Styles`. The `Default` style becomes the table's default (Normal) style, and
    /// every other style starts from it (or from its `ss:Parent`).
    pub fn read(styles: Option<&El>) -> Styles {
        let defs: HashMap<&str, &El> = styles.map(|s| s.kids("Style").filter_map(|e| Some((e.attr("ID")?, e))).collect()).unwrap_or_default();
        let normal = resolve(&defs, "Default", 0);
        let mut table = StyleTable::with_default(normal);
        let mut ids = HashMap::new();
        ids.insert("Default".to_string(), StyleId::DEFAULT);
        for id in defs.keys() {
            if *id != "Default" {
                ids.insert((*id).to_string(), table.intern(resolve(&defs, id, 0)));
            }
        }
        Styles { table, ids }
    }

    /// The style a `ss:StyleID` names; `None` for no attribute, the default style for unknown ids.
    pub fn id(&self, e: &El) -> Option<StyleId> {
        e.attr("StyleID").map(|id| self.ids.get(id).copied().unwrap_or(StyleId::DEFAULT))
    }
}

fn resolve(defs: &HashMap<&str, &El>, id: &str, depth: usize) -> Style {
    let Some(def) = defs.get(id) else { return Style::default() };
    let mut style = if depth >= MAX_PARENTS {
        Style::default()
    } else if let Some(parent) = def.attr("Parent") {
        resolve(defs, parent, depth + 1)
    } else if id != "Default" {
        resolve(defs, "Default", depth + 1)
    } else {
        Style::default()
    };
    apply(&mut style, def);
    style
}

fn apply(s: &mut Style, def: &El) {
    for child in &def.children {
        match child.name.as_str() {
            "NumberFormat" => {
                if let Some(f) = child.attr("Format") {
                    s.num_fmt = NumFmt::new(number_format(f));
                }
            }
            "Font" => font(s, child),
            "Alignment" => alignment(s, child),
            "Interior" => {
                let color = child.attr("Color").and_then(Color::from_hex);
                match (child.attr("Pattern"), color) {
                    (Some("None"), _) => s.fill = Fill::default(),
                    (_, Some(c)) => s.fill = Fill::solid(c),
                    _ => {}
                }
            }
            "Borders" => {
                for b in child.kids("Border") {
                    let line = border(b);
                    match b.attr("Position") {
                        Some("Left") => s.borders.left = line,
                        Some("Right") => s.borders.right = line,
                        Some("Top") => s.borders.top = line,
                        Some("Bottom") => s.borders.bottom = line,
                        Some("DiagonalLeft") => s.borders.diag_down = line,
                        Some("DiagonalRight") => s.borders.diag_up = line,
                        _ => {}
                    }
                }
            }
            "Protection" => {
                if let Some(v) = child.attr_bool("Protected") {
                    s.protection.locked = v;
                }
                if let Some(v) = child.attr_bool("HideFormula") {
                    s.protection.hidden = v;
                }
            }
            _ => {}
        }
    }
}

/// `ss:Format`: a named format or a format code.
fn number_format(f: &str) -> &str {
    match f {
        "General" | "General Number" | "Yes/No" | "True/False" | "On/Off" => "General",
        "Fixed" => "0.00",
        "Standard" => "#,##0.00",
        "Percent" => "0.00%",
        "Scientific" => "0.00E+00",
        "Short Date" => "m/d/yyyy",
        "Medium Date" => "d-mmm-yy",
        "Long Date" => "dddd, mmmm d, yyyy",
        "Short Time" => "h:mm",
        "Medium Time" => "h:mm AM/PM",
        "Long Time" => "h:mm:ss AM/PM",
        "Currency" => "\"$\"#,##0.00",
        _ => f,
    }
}

fn font(s: &mut Style, e: &El) {
    let f = &mut s.font;
    if let Some(name) = e.attr("FontName").map(str::trim).filter(|n| !n.is_empty() && *n != f.name) {
        f.name = name.to_string();
        f.minor = false;
        f.major = false;
    }
    if let Some(size) = e.attr_f64("Size").filter(|v| *v > 0.0 && *v <= 409.0) {
        f.size = size as f32;
    }
    if let Some(v) = e.attr_bool("Bold") {
        f.bold = v;
    }
    if let Some(v) = e.attr_bool("Italic") {
        f.italic = v;
    }
    if let Some(v) = e.attr_bool("StrikeThrough") {
        f.strike = v;
    }
    match e.attr("Underline") {
        Some("Single") => f.underline = Underline::Single,
        Some("Double") => f.underline = Underline::Double,
        Some("SingleAccounting") => f.underline = Underline::SingleAccounting,
        Some("DoubleAccounting") => f.underline = Underline::DoubleAccounting,
        Some("None") => f.underline = Underline::None,
        _ => {}
    }
    match e.attr("VerticalAlign") {
        Some("Superscript") => f.vert = VertAlign::Superscript,
        Some("Subscript") => f.vert = VertAlign::Subscript,
        Some("None") => f.vert = VertAlign::Baseline,
        _ => {}
    }
    if let Some(c) = e.attr("Color").and_then(Color::from_hex) {
        f.color = c;
    }
}

fn alignment(s: &mut Style, e: &El) {
    let a = &mut s.align;
    a.h = match e.attr("Horizontal") {
        Some("Automatic") => HAlign::General,
        Some("Left") => HAlign::Left,
        Some("Center") => HAlign::Center,
        Some("Right") => HAlign::Right,
        Some("Justify") => HAlign::Justify,
        Some("Fill") => HAlign::Fill,
        Some("CenterAcrossSelection") => HAlign::CenterAcross,
        Some("Distributed") => HAlign::Distributed,
        _ => a.h,
    };
    a.v = match e.attr("Vertical") {
        Some("Top") => VAlign::Top,
        Some("Center") => VAlign::Center,
        Some("Bottom") => VAlign::Bottom,
        Some("Justify") => VAlign::Justify,
        Some("Distributed") => VAlign::Distributed,
        _ => a.v,
    };
    if let Some(v) = e.attr_bool("WrapText") {
        a.wrap = v;
    }
    if let Some(v) = e.attr_bool("ShrinkToFit") {
        a.shrink = v;
    }
    if let Some(n) = e.attr_u32("Indent") {
        a.indent = n.min(250) as u8;
    }
    if let Some(r) = e.attr_f64("Rotate").filter(|r| (-90.0..=90.0).contains(r)) {
        a.rotation = r.round() as i16;
    }
    if e.attr_bool("VerticalText") == Some(true) {
        a.rotation = 255;
    }
}

fn border(e: &El) -> BorderLine {
    let weight = e.attr_u32("Weight").unwrap_or(1);
    let heavy = weight >= 2;
    let style = match e.attr("LineStyle") {
        Some("Continuous") => match weight {
            0 => BorderStyle::Hair,
            1 => BorderStyle::Thin,
            2 => BorderStyle::Medium,
            _ => BorderStyle::Thick,
        },
        Some("Dash") if heavy => BorderStyle::MediumDashed,
        Some("Dash") => BorderStyle::Dashed,
        Some("Dot") => BorderStyle::Dotted,
        Some("Double") => BorderStyle::Double,
        Some("DashDot") if heavy => BorderStyle::MediumDashDot,
        Some("DashDot") => BorderStyle::DashDot,
        Some("DashDotDot") if heavy => BorderStyle::MediumDashDotDot,
        Some("DashDotDot") => BorderStyle::DashDotDot,
        Some("SlantDashDot") => BorderStyle::SlantDashDot,
        _ => BorderStyle::None,
    };
    let color = e.attr("Color").and_then(Color::from_hex).unwrap_or_default();
    BorderLine { style, color }
}

/// `YYYY-MM-DDTHH:MM:SS(.fff)?` as a serial in `sys`. In the 1900 system serial 1 is 1900-01-01
/// and 1899-12-31 is day 0 (Excel writes a time with no date there); the fictitious 1900-02-29 is
/// counted, so 1900-03-01 is 61.
pub(super) fn datetime(s: &str, sys: DateSystem) -> Option<f64> {
    let (date, time) = s.trim().split_once('T')?;
    let digits = |t: &str| -> Option<i64> { t.bytes().all(|b| b.is_ascii_digit()).then(|| t.parse().ok()).flatten() };
    let mut d = date.split('-');
    let (y, m, day) = (d.next()?, d.next()?, d.next()?);
    if d.next().is_some() || y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    let (y, m, day) = (digits(y)?, digits(m)?, digits(day)?);
    if !(1..=12).contains(&m) || day < 1 || day > i64::from(days_in_month(i32::try_from(y).ok()?, u32::try_from(m).ok()?)) {
        return None;
    }
    let days = if sys == DateSystem::D1900 && (y, m, day) == (1899, 12, 31) {
        0.0
    } else if (sys == DateSystem::D1900 && y < 1900) || (sys == DateSystem::D1904 && y < 1904) {
        return None;
    } else {
        serial_from_ymd(sys, y, m, day)?
    };
    let mut t = time.split(':');
    let (h, min, sec) = (t.next()?, t.next()?, t.next()?);
    if t.next().is_some() || h.len() != 2 || min.len() != 2 {
        return None;
    }
    let (whole, frac) = sec.split_once('.').unwrap_or((sec, "0"));
    if whole.len() != 2 || frac.is_empty() {
        return None;
    }
    let (h, min, whole) = (digits(h)?, digits(min)?, digits(whole)?);
    digits(frac)?;
    let sec = format!("{whole}.{frac}").parse::<f64>().ok()?;
    if h >= 24 || min >= 60 || sec >= 60.0 {
        return None;
    }
    Some(days + (h as f64 * 3600.0 + min as f64 * 60.0 + sec) / 86400.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetimes_in_both_systems() {
        let d = DateSystem::D1900;
        assert_eq!(datetime("2026-01-15T00:00:00.000", d), Some(46037.0));
        assert_eq!(datetime("2026-01-15T18:00:00.000", d), Some(46037.75));
        assert_eq!(datetime("1900-03-01T00:00:00.000", d), Some(61.0));
        assert_eq!(datetime("1900-01-01T00:00:00", d), Some(1.0));
        assert_eq!(datetime("1899-12-31T12:00:00.000", d), Some(0.5));
        assert_eq!(datetime("2026-01-15T00:00:00.000", DateSystem::D1904), Some(44575.0));
        assert_eq!(datetime("2026-01-15T18:00:00.000", DateSystem::D1904), Some(44575.75));
        for bad in
            ["2026-02-30T00:00:00", "2026-01-15", "1899-12-30T00:00:00", "2026-01-15T24:00:00", "x", "2026-01-15T1:00:00", "٢٠٢٦-01-15T00:00:00"]
        {
            assert_eq!(datetime(bad, d), None, "{bad}");
        }
        assert_eq!(datetime("1903-01-01T00:00:00", DateSystem::D1904), None);
    }
}
