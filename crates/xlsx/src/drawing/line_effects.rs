//! A shape's outline dash, cap and join (`a:ln`, CT_LineProperties, ECMA-376 Part 1
//! §20.1.2.2.24) and its effect list (`a:effectLst`, §20.1.8.26) when that is one outer shadow
//! (`a:outerShdw`, §20.1.8.45) or empty. Values are rebuilt from parsed fields; a value the
//! standard does not list is dropped.

use std::fmt::Write as _;

use gridcraft_model::{Color, KeptDmlColor, LineCap, LineJoin, OuterShadow, ShapeEffects, ShapeFormat};

use super::{DML_COLOR_TRANSFORMS, SCHEME_COLOR_VALS, dml_color};
use crate::chart::DASHES;
use crate::xml::El;

/// ST_RectAlignment (§20.1.10.53).
const RECT_ALIGNMENTS: [&str; 9] = ["tl", "t", "tr", "l", "ctr", "r", "bl", "b", "br"];

/// Reads the dash, cap and join of `ln` and the effect list of `sp_pr` into `f`.
pub(super) fn read(sp_pr: &El, ln: Option<&El>, f: &mut ShapeFormat) {
    if let Some(ln) = ln {
        f.line_dash = ln.child("prstDash").and_then(|d| d.attr("val")).and_then(|v| DASHES.iter().find(|(_, n)| *n == v)).map(|(d, _)| *d);
        f.line_cap = match ln.attr("cap") {
            Some("rnd") => Some(LineCap::Round),
            Some("sq") => Some(LineCap::Square),
            Some("flat") => Some(LineCap::Flat),
            _ => None,
        };
        f.line_join = ln.children.iter().find_map(|c| match c.name.as_str() {
            "round" => Some(LineJoin::Round),
            "bevel" => Some(LineJoin::Bevel),
            "miter" => Some(LineJoin::Miter(c.attr("lim").and_then(|v| v.trim().parse::<i32>().ok()).filter(|v| *v >= 0))),
            _ => None,
        });
    }
    f.effects = sp_pr.child("effectLst").and_then(read_effects);
}

fn read_effects(e: &El) -> Option<ShapeEffects> {
    match e.children.as_slice() {
        [] => Some(ShapeEffects::None),
        [s] if s.name == "outerShdw" => read_shadow(s).map(ShapeEffects::OuterShadow),
        _ => None,
    }
}

fn read_shadow(s: &El) -> Option<OuterShadow> {
    let i32_attr = |k: &str| s.attr(k).and_then(|v| v.trim().parse::<i32>().ok());
    let i64_attr = |k: &str| s.attr(k).and_then(|v| v.trim().parse::<i64>().ok());
    // A shadow needs its colour; one whose colour isn't kept is left unstated.
    let color = kept_color(s)?;
    Some(OuterShadow {
        blur_rad: i64_attr("blurRad").filter(|v| *v >= 0),
        dist: i64_attr("dist").filter(|v| *v >= 0),
        dir: i32_attr("dir"),
        sx: i32_attr("sx"),
        sy: i32_attr("sy"),
        kx: i32_attr("kx"),
        ky: i32_attr("ky"),
        algn: s.attr("algn").filter(|a| RECT_ALIGNMENTS.contains(a)).map(str::to_string),
        rot_with_shape: s.attr_bool("rotWithShape"),
        color: Some(color),
    })
}

/// A name of an `a:prstClr` / `a:sysClr` value: letters and digits only, so it is safe to write.
fn colour_name(v: &str) -> Option<&str> {
    (!v.is_empty() && v.len() <= 32 && v.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(v)
}

/// The colour child of `e` (`a:srgbClr`, `a:schemeClr`, `a:prstClr` or `a:sysClr`) with its
/// transforms in document order, rebuilt from parsed values.
fn kept_color(e: &El) -> Option<KeptDmlColor> {
    let el = e.children.iter().find(|c| matches!(c.name.as_str(), "srgbClr" | "schemeClr" | "prstClr" | "sysClr"))?;
    let val = el.attr("val")?;
    let (head, color) = match el.name.as_str() {
        "srgbClr" => {
            let Some(Color::Rgb(v)) = Color::from_hex(val) else { return None };
            (format!("<a:srgbClr val=\"{:06X}\"", v & 0xFF_FFFF), Color::Rgb(v))
        }
        "schemeClr" => {
            let v = SCHEME_COLOR_VALS.iter().find(|s| **s == val)?;
            (format!("<a:schemeClr val=\"{v}\""), dml_color(e).unwrap_or_default())
        }
        "prstClr" => (format!("<a:prstClr val=\"{}\"", colour_name(val)?), Color::default()),
        _ => {
            let mut h = format!("<a:sysClr val=\"{}\"", colour_name(val)?);
            let last = el.attr("lastClr").and_then(Color::from_hex);
            if let Some(Color::Rgb(v)) = last {
                let _ = write!(h, " lastClr=\"{:06X}\"", v & 0xFF_FFFF);
            }
            (h, last.unwrap_or_default())
        }
    };
    let mut kids = String::new();
    for k in &el.children {
        let Some(&(name, has_val)) = DML_COLOR_TRANSFORMS.iter().find(|(n, _)| *n == k.name) else { continue };
        if has_val {
            let Some(v) = k.attr("val").and_then(|v| v.trim().parse::<i64>().ok()) else { continue };
            let _ = write!(kids, "<a:{name} val=\"{v}\"/>");
        } else {
            let _ = write!(kids, "<a:{name}/>");
        }
    }
    let tag = el.name.as_str();
    let xml = if kids.is_empty() { format!("{head}/>") } else { format!("{head}>{kids}</a:{tag}>") };
    Some(KeptDmlColor { color, xml })
}

/// The `cap` attribute of `a:ln`, with its leading space; empty when not stated.
pub(super) fn cap_attr(f: &ShapeFormat) -> &'static str {
    match f.line_cap {
        Some(LineCap::Round) => " cap=\"rnd\"",
        Some(LineCap::Square) => " cap=\"sq\"",
        Some(LineCap::Flat) => " cap=\"flat\"",
        None => "",
    }
}

/// The dash and join children of `a:ln`, written after the line fill.
pub(super) fn write_dash_join(s: &mut String, f: &ShapeFormat) {
    if let Some((_, v)) = f.line_dash.and_then(|d| DASHES.iter().find(|(k, _)| *k == d)) {
        let _ = write!(s, "<a:prstDash val=\"{v}\"/>");
    }
    match f.line_join {
        Some(LineJoin::Round) => s.push_str("<a:round/>"),
        Some(LineJoin::Bevel) => s.push_str("<a:bevel/>"),
        Some(LineJoin::Miter(Some(lim))) if lim >= 0 => {
            let _ = write!(s, "<a:miter lim=\"{lim}\"/>");
        }
        Some(LineJoin::Miter(_)) => s.push_str("<a:miter/>"),
        None => {}
    }
}

/// The kept effect list, written after `a:ln`.
pub(super) fn write_effects(s: &mut String, f: &ShapeFormat) {
    match &f.effects {
        None => {}
        Some(ShapeEffects::None) => s.push_str("<a:effectLst/>"),
        Some(ShapeEffects::OuterShadow(sh)) => {
            // CT_OuterShadowEffect requires a colour; without one nothing is written.
            let Some(color) = sh.color.as_ref().filter(|c| !c.xml.is_empty()) else { return };
            s.push_str("<a:effectLst><a:outerShdw");
            let nums: [(&str, Option<i64>); 7] = [
                ("blurRad", sh.blur_rad.filter(|v| *v >= 0)),
                ("dist", sh.dist.filter(|v| *v >= 0)),
                ("dir", sh.dir.map(i64::from)),
                ("sx", sh.sx.map(i64::from)),
                ("sy", sh.sy.map(i64::from)),
                ("kx", sh.kx.map(i64::from)),
                ("ky", sh.ky.map(i64::from)),
            ];
            for (k, v) in nums {
                if let Some(v) = v {
                    let _ = write!(s, " {k}=\"{v}\"");
                }
            }
            if let Some(a) = sh.algn.as_deref().filter(|a| RECT_ALIGNMENTS.contains(a)) {
                let _ = write!(s, " algn=\"{a}\"");
            }
            if let Some(r) = sh.rot_with_shape {
                let _ = write!(s, " rotWithShape=\"{}\"", u8::from(r));
            }
            let _ = write!(s, ">{}</a:outerShdw></a:effectLst>", color.xml);
        }
    }
}
