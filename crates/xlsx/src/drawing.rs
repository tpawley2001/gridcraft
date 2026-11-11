//! DrawingML spreadsheet drawings (`xl/drawings/drawingN.xml`): anchors, pictures, shapes,
//! equations and chart frames.
//!
//! An equation is written as `mc:AlternateContent`: the `a14` choice is a text box holding the
//! equation as Office Math (`a14:m`), the fallback the same text box holding the LaTeX as plain
//! text. Both carry the source in `descr="gridcraft:latex:…"`, which wins on reading; without it,
//! Office Math in the subset `omml` reads is converted back to LaTeX.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS};
use gridcraft_model::{
    Anchor, AnchorMode, CameraTool, Color, DrawingHyperlink, GeometryPath, Image, PathCommand, PathShade, PictureLocks, Shape, ShapeFormat,
    ShapeGeometry, ShapeKind, ShapeTextAlign, ShapeTextAnchor, ShapeTextVert, ShapeTextWrap, Sheet, Theme,
};

use crate::IoError;
use crate::read::Ctx;
use crate::tables::EMU_PER_PX;
use crate::xml::{El, esc, esc_attr};

mod blip_ext;
pub use blip_ext::write as write_blip_ext;
mod ink;

/// The chart's own theme is optional and stays opaque (ECMA-376 Part 1 §14.2.8).
fn read_theme_override(cx: &mut Ctx<'_>, chart_part: &str) -> Result<Option<Arc<Vec<u8>>>, IoError> {
    let Some(rel) = cx.pkg.rels(chart_part)?.into_iter().find(|r| r.kind == "themeOverride" && !r.external) else {
        return Ok(None);
    };
    match cx.pkg.read_optional_limited(&rel.target, 4 * 1024 * 1024) {
        Ok(Some(bytes)) if valid_theme_override(&bytes) => Ok(Some(Arc::new(bytes))),
        Ok(None) => Ok(None),
        Ok(Some(_)) => {
            cx.warn(format!("chart theme override {} was dropped: invalid XML or root element", rel.target));
            Ok(None)
        }
        Err(e) => {
            cx.warn(format!("chart theme override {} was dropped: {e}", rel.target));
            Ok(None)
        }
    }
}

fn valid_theme_override(bytes: &[u8]) -> bool {
    use quick_xml::events::Event;

    // The general XML reader recovers truncated parts. Overrides must instead be complete
    // documents, with one root, matching end tags and bounded nesting.
    let data = crate::xml::to_utf8(bytes);
    let mut reader = quick_xml::Reader::from_reader(data.as_ref());
    let mut depth = 0usize;
    let mut seen_root = false;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) if depth == 0 => {
                if seen_root || e.local_name().as_ref() != b"themeOverride" || e.attributes().any(|a| a.is_err()) {
                    return false;
                }
                seen_root = true;
            }
            Ok(Event::Start(e)) if depth == 0 => {
                if seen_root || e.local_name().as_ref() != b"themeOverride" || e.attributes().any(|a| a.is_err()) {
                    return false;
                }
                seen_root = true;
                depth = 1;
            }
            Ok(Event::Start(e)) => {
                if e.attributes().any(|a| a.is_err()) {
                    return false;
                }
                depth = depth.saturating_add(1);
                if depth > 256 {
                    return false;
                }
            }
            Ok(Event::Empty(e)) if e.attributes().any(|a| a.is_err()) => return false,
            Ok(Event::CData(_) | Event::GeneralRef(_)) if depth == 0 => return false,
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Text(t)) if depth == 0 && !t.as_ref().iter().all(u8::is_ascii_whitespace) => return false,
            Ok(Event::Eof) => return seen_root && depth == 0,
            Err(_) => return false,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- reading

fn emu_px(v: Option<i64>) -> f32 {
    v.map(|e| (e as f64 / EMU_PER_PX).clamp(-1e7, 1e7) as f32).unwrap_or(0.0)
}

pub(crate) fn marker(e: &El) -> (CellRef, f32, f32) {
    let num = |n: &str| e.child(n).and_then(|c| c.text.trim().parse::<i64>().ok());
    let col = num("col").unwrap_or(0).clamp(0, MAX_COLS as i64 - 1) as u32;
    let row = num("row").unwrap_or(0).clamp(0, MAX_ROWS as i64 - 1) as u32;
    (CellRef::new(row, col), emu_px(num("colOff")).max(0.0), emu_px(num("rowOff")).max(0.0))
}

fn ext_px(e: Option<&El>) -> (f32, f32) {
    match e {
        Some(x) => (emu_px(x.attr_i64("cx")).max(0.0), emu_px(x.attr_i64("cy")).max(0.0)),
        None => (0.0, 0.0),
    }
}

fn read_anchor(a: &El, sheet: &Sheet) -> Option<Anchor> {
    match a.name.as_str() {
        "twoCellAnchor" => {
            let (cell, dx, dy) = marker(a.child("from")?);
            let (to, tdx, tdy) = a.child("to").map(marker).unwrap_or((cell, dx, dy));
            let x1 = sheet.col_left(cell.col) + dx as f64;
            let y1 = sheet.row_top(cell.row) + dy as f64;
            let x2 = sheet.col_left(to.col) + tdx as f64;
            let y2 = sheet.row_top(to.row) + tdy as f64;
            // Excel stores "Move and size with cells" as a plain twoCellAnchor, "Move but don't
            // size" as editAs="oneCell", and "Don't move or size" as editAs="absolute".
            let mode = match a.attr("editAs") {
                Some("absolute") => AnchorMode::Absolute,
                Some("oneCell") => AnchorMode::MoveOnly,
                _ => AnchorMode::MoveAndSize,
            };
            Some(Anchor { cell, dx, dy, width: (x2 - x1).max(0.0) as f32, height: (y2 - y1).max(0.0) as f32, mode })
        }
        "oneCellAnchor" => {
            let (cell, dx, dy) = marker(a.child("from")?);
            let (width, height) = ext_px(a.child("ext"));
            Some(Anchor { cell, dx, dy, width, height, mode: AnchorMode::MoveOnly })
        }
        "absoluteAnchor" => {
            let pos = a.child("pos");
            let x = emu_px(pos.and_then(|p| p.attr_i64("x"))).max(0.0) as f64;
            let y = emu_px(pos.and_then(|p| p.attr_i64("y"))).max(0.0) as f64;
            let col = sheet.col_at(x);
            let row = sheet.row_at(y);
            let (width, height) = ext_px(a.child("ext"));
            Some(Anchor {
                cell: CellRef::new(row, col),
                dx: (x - sheet.col_left(col)).max(0.0) as f32,
                dy: (y - sheet.row_top(row)).max(0.0) as f32,
                width,
                height,
                mode: AnchorMode::Absolute,
            })
        }
        _ => None,
    }
}

/// Colour of a DrawingML colour choice inside `e` (e.g. a `solidFill`).
pub fn dml_color(e: &El) -> Option<Color> {
    if let Some(c) = e.child("srgbClr").and_then(|c| c.attr("val")) {
        return Color::from_hex(c);
    }
    if let Some(c) = e.child("sysClr").and_then(|c| c.attr("lastClr")) {
        return Color::from_hex(c);
    }
    if let Some(c) = e.child("schemeClr").and_then(|c| c.attr("val")) {
        let i = match c {
            "bg1" | "lt1" => 0,
            "tx1" | "dk1" => 1,
            "bg2" | "lt2" => 2,
            "tx2" | "dk2" => 3,
            "accent1" => 4,
            "accent2" => 5,
            "accent3" => 6,
            "accent4" => 7,
            "accent5" => 8,
            "accent6" => 9,
            "hlink" => 10,
            "folHlink" => 11,
            _ => return None,
        };
        return Some(Color::Theme(i, 0));
    }
    None
}

/// Colour of a DrawingML colour choice inside `e`, like [`dml_color`], with the `shade` and
/// `tint` children of `a:srgbClr` / `a:schemeClr` applied in document order. A transformed colour
/// is resolved against the theme colours (lt1, dk1, lt2, dk2, accent1..6, hlink, folHlink) to RGB;
/// an untransformed one is returned as [`dml_color`] reads it.
pub(crate) fn dml_color_resolved(e: &El, theme: &[u32; 12]) -> Option<Color> {
    let base = dml_color(e)?;
    let el = if e.child("srgbClr").is_some_and(|c| c.attr("val").is_some()) {
        e.child("srgbClr")
    } else if e.child("sysClr").is_some_and(|c| c.attr("lastClr").is_some()) {
        None
    } else {
        e.child("schemeClr")
    };
    let Some(el) = el else { return Some(base) };
    let transforms: Vec<(bool, f64)> = el
        .children
        .iter()
        .filter(|c| c.name == "shade" || c.name == "tint")
        .filter_map(|c| {
            let v = c.attr("val")?.trim().parse::<f64>().ok().filter(|v| v.is_finite())?;
            Some((c.name == "shade", (v / 100_000.0).clamp(0.0, 1.0)))
        })
        .collect();
    if transforms.is_empty() {
        return Some(base);
    }
    let rgb = match base {
        Color::Rgb(v) => v,
        Color::Theme(i, _) => theme.get(i as usize).copied().unwrap_or(0),
        Color::Auto => return Some(base),
    };
    let channels = [(rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF].map(|c| srgb_to_linear(c as u8));
    let out = channels.map(|mut l| {
        for &(shade, k) in &transforms {
            l = if shade { l * k } else { 1.0 - (1.0 - l) * k };
        }
        linear_to_srgb(l)
    });
    Some(Color::rgb(out[0], out[1], out[2]))
}

/// An sRGB channel as linear light, 0..=1 (IEC 61966-2-1).
fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Linear light (0..=1) back to an sRGB channel.
fn linear_to_srgb(l: f64) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let c = if l <= 0.0031308 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn read_hyperlink(props: Option<&El>, rels: &[crate::package::Rel]) -> Option<DrawingHyperlink> {
    let link = props?.child("hlinkClick")?;
    let rid = link.attr("id")?;
    let rel = rels.iter().find(|r| r.id == rid && r.kind == "hyperlink" && (r.external || r.target.starts_with('#')))?;
    Some(DrawingHyperlink { target: rel.target.clone(), tooltip: link.attr("tooltip").map(str::to_string) })
}

pub fn read_drawing(cx: &mut Ctx<'_>, part: &str, sheet: &mut Sheet) -> Result<(), IoError> {
    let Some(x) = cx.optional_xml(part)? else { return Ok(()) };
    let rels = cx.pkg.rels(part)?;
    // Blip extensions (picture effects) are kept as written: scan the raw XML for them, matched
    // to the parsed `a:blip` elements by position.
    let blips = blip_ext::blips(&x);
    let blip_exts = if blips.iter().any(|b| b.child("extLst").is_some()) {
        let scanned = cx.pkg.read(part)?.as_deref().and_then(blip_ext::scan);
        match scanned {
            Some(v) if v.len() == blips.len() => v,
            _ => {
                cx.warn(format!("picture effects could not be read and were dropped ({part})"));
                vec![]
            }
        }
    } else {
        vec![]
    };
    let mut lookup = blip_ext::Lookup::default();
    for (n, a) in x.children.iter().enumerate() {
        let Some(anchor) = read_anchor(a, sheet) else { continue };
        let n = n + 1;
        // "Print object" off (ECMA-376 Part 1 §20.5.2.3, `fPrintsWithSheet`, default true).
        let no_print = a.child("clientData").is_some_and(|c| !c.flag("fPrintsWithSheet", true));
        // Objects in markup-compatibility blocks: the first choice (chartex frames, or Office Math
        // that converts to LaTeX), else the fallback.
        let objs: Vec<&El> = a
            .children
            .iter()
            .flat_map(|o| match o.name.as_str() {
                "AlternateContent" => o
                    .child("Choice")
                    .filter(|c| {
                        // Office Math is taken only when the box holds one equation that
                        // converts: otherwise the fallback (a picture or text from the writer)
                        // keeps everything visible.
                        c.child("graphicFrame").is_some()
                            || c.child("sp").and_then(|sp| sp.child("txBody")).and_then(crate::omml::equation_source).is_some()
                    })
                    .or_else(|| o.child("Fallback"))
                    .map(|c| c.children.iter().collect())
                    .unwrap_or_default(),
                _ => vec![o],
            })
            .collect();
        for obj in objs {
            match obj.name.as_str() {
                "pic" => {
                    let blip = obj.path(&["blipFill", "blip"]);
                    // A linked picture names its image by `r:link` (an external relationship);
                    // "Link and Insert" carries both, the embedded copy and the link.
                    let link = blip.and_then(|b| b.attr("link")).and_then(|rid| {
                        let rel = rels.iter().find(|r| r.id == rid && r.external)?;
                        Some(rel.target.clone())
                    });
                    let embed = blip.and_then(|b| b.attr("embed"));
                    let (target, data) = match embed {
                        Some(rid) => {
                            let Some(rel) = rels.iter().find(|r| r.id == rid && !r.external) else {
                                cx.warn("a linked (not embedded) picture was skipped");
                                continue;
                            };
                            let target = rel.target.clone();
                            let Some(data) = cx.pkg.read(&target)? else {
                                cx.warn(format!("missing picture {target}"));
                                continue;
                            };
                            (target, data)
                        }
                        None => match &link {
                            Some(target) => (target.clone(), Vec::new()),
                            None => continue,
                        },
                    };
                    let alt = obj.path(&["nvPicPr", "cNvPr"]).and_then(|c| c.attr("descr")).unwrap_or("").to_string();
                    let exts = blip
                        .and_then(|b| blips.iter().position(|e| std::ptr::eq(*e, b)))
                        .and_then(|i| blip_exts.get(i))
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    let blip_ext = blip_ext::read(cx, part, &mut lookup, exts)?;
                    let id = cx.next_object_id();
                    sheet.images.push(Image {
                        camera: obj.path(&["nvPicPr", "cNvPicPr", "extLst"]).and_then(|list| {
                            let camera = list
                                .kids("ext")
                                .filter(|ext| ext.attr("uri") == Some("{84589F7E-364E-4C9E-8A38-B11213B215E9}"))
                                .find_map(|ext| ext.child("cameraTool"))?;
                            Some(CameraTool { cell_range: camera.attr("cellRange")?.to_string(), spid: camera.attr("spid")?.to_string() })
                        }),
                        locks: obj.path(&["nvPicPr", "cNvPicPr"]).map(|p| {
                            p.child("picLocks")
                                .map(|l| PictureLocks {
                                    no_grp: l.flag("noGrp", false),
                                    no_select: l.flag("noSelect", false),
                                    no_rot: l.flag("noRot", false),
                                    no_change_aspect: l.flag("noChangeAspect", false),
                                    no_move: l.flag("noMove", false),
                                    no_resize: l.flag("noResize", false),
                                    no_edit_points: l.flag("noEditPoints", false),
                                    no_adjust_handles: l.flag("noAdjustHandles", false),
                                    no_change_arrowheads: l.flag("noChangeArrowheads", false),
                                    no_change_shape_type: l.flag("noChangeShapeType", false),
                                    no_crop: l.flag("noCrop", false),
                                })
                                .unwrap_or_default()
                        }),
                        hidden: obj.path(&["nvPicPr", "cNvPr"]).is_some_and(|p| p.flag("hidden", false)),
                        no_print,
                        hyperlink: read_hyperlink(obj.path(&["nvPicPr", "cNvPr"]), &rels),
                        name: obj.path(&["nvPicPr", "cNvPr"]).and_then(|p| p.attr("name")).map(str::to_string),
                        id,
                        anchor,
                        data: data.into(),
                        mime: mime_for(&target).to_string(),
                        alt,
                        link,
                        blip_ext,
                    });
                }
                "graphicFrame" => {
                    let Some(gd) = obj.path(&["graphic", "graphicData"]) else { continue };
                    let uri = gd.attr("uri").unwrap_or("");
                    if let Some(rid) = gd.child("chart").and_then(|c| c.attr("id")) {
                        let Some(rel) = rels.iter().find(|r| r.id == rid) else {
                            cx.warn(format!("a chart without its chart part was skipped ({part}, object {n})"));
                            continue;
                        };
                        let target = rel.target.clone();
                        if let Some(cx_xml) = cx.optional_xml(&target)? {
                            let chartex = uri.contains("chartex");
                            match if chartex { crate::chartex::read_chartex(&cx_xml) } else { crate::chart::read_chart(&cx_xml) } {
                                Some(mut ch) => {
                                    if !chartex {
                                        ch.theme_override = read_theme_override(cx, &target)?;
                                    }
                                    ch.id = cx.next_object_id();
                                    ch.anchor = anchor;
                                    ch.hidden = obj.path(&["nvGraphicFramePr", "cNvPr"]).is_some_and(|p| p.flag("hidden", false));
                                    ch.no_print = no_print;
                                    ch.alt = obj.path(&["nvGraphicFramePr", "cNvPr"]).and_then(|p| p.attr("descr")).unwrap_or("").to_string();
                                    sheet.charts.push(ch);
                                }
                                None => cx.warn(format!("chart {target} has no supported plot and was skipped")),
                            }
                        } else {
                            cx.warn(format!("chart {target} is missing from the package and was skipped ({part}, object {n})"));
                        }
                    } else if uri == "http://schemas.openxmlformats.org/drawingml/2006/diagram" {
                        // SmartArt: kept as it is by the pass-through reader, which reports it.
                    } else if uri.contains("chartex") || uri.contains("chartEx") {
                        cx.warn(format!("a histogram, waterfall, treemap… chart without its chart part was skipped ({part}, object {n})"));
                    } else {
                        cx.warn(format!("an embedded object (SmartArt, OLE…) was skipped ({part}, object {n})"));
                    }
                }
                "sp" | "cxnSp" => {
                    if let Some(mut s) = read_shape(obj, anchor, &cx.theme_colors) {
                        s.hidden =
                            obj.path(&["nvSpPr", "cNvPr"]).or_else(|| obj.path(&["nvCxnSpPr", "cNvPr"])).is_some_and(|p| p.flag("hidden", false));
                        s.no_print = no_print;
                        s.hyperlink = read_hyperlink(obj.path(&["nvSpPr", "cNvPr"]).or_else(|| obj.path(&["nvCxnSpPr", "cNvPr"])), &rels);
                        s.name = obj
                            .path(&["nvSpPr", "cNvPr"])
                            .or_else(|| obj.path(&["nvCxnSpPr", "cNvPr"]))
                            .and_then(|p| p.attr("name"))
                            .map(str::to_string);
                        // An equation's description is its LaTeX marker, not alternative text.
                        let descr =
                            obj.path(&["nvSpPr", "cNvPr"]).or_else(|| obj.path(&["nvCxnSpPr", "cNvPr"])).and_then(|p| p.attr("descr")).unwrap_or("");
                        if s.kind != ShapeKind::Equation && !descr.starts_with(LATEX_DESCR) {
                            s.alt = descr.to_string();
                        }
                        let id = cx.next_object_id();
                        sheet.shapes.push(Shape { id, ..s });
                    }
                }
                // Groups are kept as they are by the pass-through, which reports them.
                "grpSp" => {}
                _ => {}
            }
        }
    }
    Ok(())
}

/// A shape's `a:prstGeom`: the preset it was read with and its adjust guides when it kept them,
/// else the kind's own preset without guides.
fn write_preset_geometry(s: &mut String, prst: &str, geometry: Option<&ShapeGeometry>) {
    let Some(g) = geometry.filter(|g| !g.preset.is_empty()) else {
        let _ = write!(s, "<a:prstGeom prst=\"{prst}\"><a:avLst/></a:prstGeom>");
        return;
    };
    let _ = write!(s, "<a:prstGeom prst=\"{}\">", esc_attr(&g.preset));
    if g.adjust.is_empty() {
        s.push_str("<a:avLst/>");
    } else {
        s.push_str("<a:avLst>");
        for (name, fmla) in &g.adjust {
            let _ = write!(s, "<a:gd name=\"{}\" fmla=\"{}\"/>", esc_attr(name), esc_attr(fmla));
        }
        s.push_str("</a:avLst>");
    }
    s.push_str("</a:prstGeom>");
}

/// A shape's custom geometry paths (`a:custGeom/a:pathLst/a:path`, ECMA-376 Part 1 §20.1.9.15),
/// coordinates as written. `None` without a path, or when a coordinate is a guide name rather than
/// a number (guides aren't evaluated) or a command is unknown: the shape then keeps the rectangle
/// fallback.
fn read_custom_paths(geom: &El) -> Option<Vec<GeometryPath>> {
    let mut paths = Vec::new();
    for path in geom.child("pathLst")?.children.iter().filter(|p| p.name == "path") {
        let pt = |c: &El| c.attr_i64("x").zip(c.attr_i64("y"));
        let pts = |c: &El| c.children.iter().filter(|p| p.name == "pt").map(pt).collect::<Option<Vec<_>>>();
        let mut commands = Vec::new();
        for c in &path.children {
            commands.push(match c.name.as_str() {
                "moveTo" => {
                    let (x, y) = c.child("pt").and_then(pt)?;
                    PathCommand::MoveTo(x, y)
                }
                "lnTo" => {
                    let (x, y) = c.child("pt").and_then(pt)?;
                    PathCommand::LineTo(x, y)
                }
                "cubicBezTo" => PathCommand::CubicBezTo(pts(c)?.try_into().ok()?),
                "quadBezTo" => PathCommand::QuadBezTo(pts(c)?.try_into().ok()?),
                "arcTo" => {
                    PathCommand::ArcTo { w_r: c.attr_i64("wR")?, h_r: c.attr_i64("hR")?, st_ang: c.attr_i64("stAng")?, sw_ang: c.attr_i64("swAng")? }
                }
                "close" => PathCommand::Close,
                _ => return None,
            });
        }
        let size = |k: &str| match path.attr(k) {
            None => Some(0),
            Some(_) => path.attr_i64(k).filter(|v| *v >= 0),
        };
        paths.push(GeometryPath {
            w: size("w")?,
            h: size("h")?,
            fill: path.attr("fill") != Some("none"),
            stroke: path.attr_bool("stroke").unwrap_or(true),
            shade: match path.attr("fill") {
                Some("lighten") => PathShade::Lighten,
                Some("lightenLess") => PathShade::LightenLess,
                Some("darken") => PathShade::Darken,
                Some("darkenLess") => PathShade::DarkenLess,
                _ => PathShade::Norm,
            },
            commands,
        });
    }
    (!paths.is_empty()).then_some(paths)
}

/// A shape's geometry: a freeform's custom paths, else its preset.
fn write_geometry(s: &mut String, prst: &str, sh: &Shape) {
    match sh.geometry.as_ref().filter(|g| sh.kind == ShapeKind::Freeform && !g.paths.is_empty()) {
        Some(g) => write_custom_geometry(s, &g.paths),
        None => write_preset_geometry(s, prst, sh.geometry.as_ref()),
    }
}

/// A freeform's `a:custGeom`: its paths as read, no guides, the text box the whole shape.
fn write_custom_geometry(s: &mut String, paths: &[GeometryPath]) {
    s.push_str("<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"l\" t=\"t\" r=\"r\" b=\"b\"/><a:pathLst>");
    for p in paths {
        let _ = write!(s, "<a:path w=\"{}\" h=\"{}\"", p.w, p.h);
        let shade = match p.shade {
            PathShade::Norm => None,
            PathShade::Lighten => Some("lighten"),
            PathShade::LightenLess => Some("lightenLess"),
            PathShade::Darken => Some("darken"),
            PathShade::DarkenLess => Some("darkenLess"),
        };
        if !p.fill {
            s.push_str(" fill=\"none\"");
        } else if let Some(mode) = shade {
            let _ = write!(s, " fill=\"{mode}\"");
        }
        if !p.stroke {
            s.push_str(" stroke=\"0\"");
        }
        s.push('>');
        let pt = |s: &mut String, (x, y): (i64, i64)| {
            let _ = write!(s, "<a:pt x=\"{x}\" y=\"{y}\"/>");
        };
        for c in &p.commands {
            match c {
                PathCommand::MoveTo(x, y) => {
                    s.push_str("<a:moveTo>");
                    pt(s, (*x, *y));
                    s.push_str("</a:moveTo>");
                }
                PathCommand::LineTo(x, y) => {
                    s.push_str("<a:lnTo>");
                    pt(s, (*x, *y));
                    s.push_str("</a:lnTo>");
                }
                PathCommand::CubicBezTo(ps) => {
                    s.push_str("<a:cubicBezTo>");
                    ps.iter().for_each(|p| pt(s, *p));
                    s.push_str("</a:cubicBezTo>");
                }
                PathCommand::QuadBezTo(ps) => {
                    s.push_str("<a:quadBezTo>");
                    ps.iter().for_each(|p| pt(s, *p));
                    s.push_str("</a:quadBezTo>");
                }
                PathCommand::ArcTo { w_r, h_r, st_ang, sw_ang } => {
                    let _ = write!(s, "<a:arcTo wR=\"{w_r}\" hR=\"{h_r}\" stAng=\"{st_ang}\" swAng=\"{sw_ang}\"/>");
                }
                PathCommand::Close => s.push_str("<a:close/>"),
            }
        }
        s.push_str("</a:path>");
    }
    s.push_str("</a:pathLst></a:custGeom>");
}

fn read_shape(obj: &El, anchor: Anchor, theme: &[u32; 12]) -> Option<Shape> {
    let sp = obj.child("spPr")?;
    if let Some(shape) = ink::read(obj, anchor) {
        return Some(shape);
    }
    // A freeform (custom geometry with at least one path that isn't an ink stroke).
    let custom = sp.child("custGeom").and_then(read_custom_paths);
    let prst = sp.child("prstGeom").and_then(|g| g.attr("prst")).unwrap_or("rect");
    let tx_box = obj.path(&["nvSpPr", "cNvSpPr"]).is_some_and(|c| c.flag("txBox", false));
    let ln = sp.child("ln");
    let arrow = ln.is_some_and(|l| l.child("tailEnd").or_else(|| l.child("headEnd")).is_some_and(|t| t.attr("type").is_some_and(|v| v != "none")));
    let kind = match prst {
        _ if custom.is_some() => ShapeKind::Freeform,
        _ if tx_box => ShapeKind::TextBox,
        "roundRect" => ShapeKind::RoundedRectangle,
        "ellipse" => ShapeKind::Ellipse,
        "triangle" => ShapeKind::Triangle,
        "line" | "straightConnector1" if arrow => ShapeKind::Arrow,
        "line" | "straightConnector1" => ShapeKind::Line,
        _ => ShapeKind::Rectangle,
    };
    // A preset the kind doesn't name (a star, a callout, a flowchart box), or one with adjust
    // values, is kept as read so Save writes it back unchanged.
    let custom = custom.map(|paths| ShapeGeometry { paths, ..ShapeGeometry::default() });
    let geometry = custom.or_else(|| {
        sp.child("prstGeom").and_then(|g| {
            let adjust: Vec<(String, String)> = g
                .child("avLst")
                .map(|av| {
                    av.children
                        .iter()
                        .filter(|gd| gd.name == "gd")
                        .map(|gd| (gd.attr("name").unwrap_or("").to_string(), gd.attr("fmla").unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let known = matches!(prst, "rect" | "roundRect" | "ellipse" | "triangle" | "line" | "straightConnector1");
            (!known || !adjust.is_empty()).then(|| ShapeGeometry { preset: prst.to_string(), adjust, paths: Vec::new() })
        })
    });
    // An equation: GridCraft's own source while the marker in the description still matches the
    // body (another application that edits the Office Math, or the text of a box without Office
    // Math, leaves the description alone, as Excel does), else the Office Math converted back to
    // LaTeX. A box without Office Math whose text was edited is a text box with the edited text,
    // not the stale source, which the next save would write over the edit; as the marker joins
    // run text across paragraphs, such a box must also hold the paragraphs the writer wrote.
    let descr = obj.path(&["nvSpPr", "cNvPr"]).and_then(|c| c.attr("descr")).unwrap_or("");
    let tb = obj.child("txBody");
    let has_math = tb.is_some_and(|tb| crate::omml::find_math(tb, 0).is_some());
    let source = match (has_math, described_source(descr)) {
        (_, Some((marker, src))) if tb.is_some_and(|tb| crate::omml::body_marker(tb) == marker && (has_math || holds_written_text(tb, src))) => {
            Some(src.to_string())
        }
        (true, _) => tb.and_then(crate::omml::equation_source),
        (false, _) => None,
    };
    if let Some(text) = source {
        let ink = obj.child("txBody").and_then(|tb| first_run_color(tb, 0)).unwrap_or_default();
        return Some(Shape {
            hidden: false,
            no_print: false,
            hyperlink: None,
            name: None,
            alt: String::new(),
            id: 0,
            kind: ShapeKind::Equation,
            geometry: None,
            anchor,
            fill: Color::Auto,
            line: ink,
            text,
            format: Default::default(),
            math: Default::default(),
        });
    }
    let fill = sp.child("solidFill").and_then(|f| dml_color_resolved(f, theme)).unwrap_or_default();
    let line = ln.and_then(|l| l.child("solidFill")).and_then(|f| dml_color_resolved(f, theme)).unwrap_or_default();
    let text = obj.child("txBody").map(body_text).unwrap_or_default();
    let mut format = read_text_format(obj.child("txBody"));
    format.no_line = ln.is_some_and(|l| l.child("noFill").is_some());
    format.no_fill = sp.child("noFill").is_some();
    // ST_LineWidth: 0..=20116800 EMU; anything else keeps the default width.
    format.line_width_emu = ln.and_then(|l| l.attr("w")).and_then(|w| w.parse::<i32>().ok()).filter(|w| (0..=20_116_800).contains(w));
    Some(Shape {
        hidden: false,
        no_print: false,
        hyperlink: None,
        name: None,
        alt: String::new(),
        id: 0,
        kind,
        geometry,
        anchor,
        fill,
        line,
        text,
        format,
        math: Default::default(),
    })
}

/// A shape's text formatting: vertical anchor (`a:bodyPr anchor`), the first paragraph's
/// alignment and direction (`a:pPr algn`, `rtl`) and the first run's properties (`a:rPr`: size,
/// bold, italic, fill, outline, `a:latin` typeface).
fn read_text_format(body: Option<&El>) -> ShapeFormat {
    let mut f = ShapeFormat::default();
    let Some(body) = body else { return f };
    f.anchor = body.child("bodyPr").and_then(|b| b.attr("anchor")).map(|a| match a {
        "ctr" => ShapeTextAnchor::Middle,
        "b" => ShapeTextAnchor::Bottom,
        _ => ShapeTextAnchor::Top,
    });
    if let Some(b) = body.child("bodyPr") {
        // ST_Coordinate32 / ST_Angle: signed 32-bit integers; anything else is dropped.
        let int = |k: &str| b.attr(k).and_then(|v| v.parse::<i32>().ok());
        f.inset_left = int("lIns");
        f.inset_top = int("tIns");
        f.inset_right = int("rIns");
        f.inset_bottom = int("bIns");
        f.text_rotation = int("rot");
        f.wrap = b.attr("wrap").and_then(|w| match w {
            "none" => Some(ShapeTextWrap::None),
            "square" => Some(ShapeTextWrap::Square),
            _ => None,
        });
        f.vert = b.attr("vert").and_then(ShapeTextVert::from_xml);
    }
    let ppr = body.kids("p").next().and_then(|p| p.child("pPr"));
    f.align = ppr.and_then(|p| p.attr("algn")).map(|a| match a {
        "ctr" => ShapeTextAlign::Center,
        "r" => ShapeTextAlign::Right,
        _ => ShapeTextAlign::Left,
    });
    f.rtl = ppr.is_some_and(|p| p.flag("rtl", false));
    let rpr = body.kids("p").flat_map(|p| p.kids("r")).find_map(|r| r.child("rPr"));
    if let Some(r) = rpr {
        // `sz` is in hundredths of a point (100..=400000 in the schema).
        f.font_size = r.attr_f64("sz").filter(|v| v.is_finite()).map(|v| (v / 100.0).clamp(1.0, 4000.0) as f32);
        f.bold = r.flag("b", false);
        f.italic = r.flag("i", false);
        f.font = r.child("latin").and_then(|l| l.attr("typeface")).filter(|t| !t.is_empty() && !t.starts_with('+')).map(str::to_string);
        f.text_color = r.child("solidFill").and_then(dml_color);
        f.text_outline = r.child("ln").and_then(|l| {
            let c = l.child("solidFill").and_then(dml_color)?;
            // Width in EMU (12700 per point); Excel's default hairline is 0.75 pt.
            let w = l.attr_f64("w").filter(|w| w.is_finite()).map_or(0.75, |w| (w / 12700.0).clamp(0.0, 100.0)) as f32;
            Some((c, w))
        });
    }
    f
}

/// The `descr` prefix that carries an equation's LaTeX source, as
/// `gridcraft:latex:<marker>:<source>`: the marker is [`crate::omml::body_marker`] of the text
/// body written beside it, 16 hex digits.
const LATEX_DESCR: &str = "gridcraft:latex:";

/// The marker and source of a `gridcraft:latex:<marker>:<source>` description; `None` for any
/// other description, a malformed marker or a blank source.
fn described_source(descr: &str) -> Option<(&str, &str)> {
    let rest = descr.strip_prefix(LATEX_DESCR)?;
    let (marker, src) = (rest.get(..16)?, rest.get(17..)?);
    (rest.as_bytes().get(16) == Some(&b':') && marker.bytes().all(|b| b.is_ascii_hexdigit()) && !src.trim().is_empty()).then_some((marker, src))
}

/// The colour of the first run (DrawingML `rPr` with a fill) under `e`.
fn first_run_color(e: &El, depth: usize) -> Option<Color> {
    if depth > 12 {
        return None;
    }
    if e.name == "rPr"
        && let Some(c) = e.child("solidFill").and_then(dml_color)
    {
        return Some(c);
    }
    e.children.iter().find_map(|c| first_run_color(c, depth + 1))
}

// ---------------------------------------------------------------- writing

/// An equation shape (inside the anchor): Office Math with a plain-text fallback when the source
/// is in the subset OMML covers, else the plain-text box alone. Both carry the source.
fn write_equation(s: &mut String, sh: &Shape, id: usize, theme: &Theme) {
    // The font size the equation is drawn at (it scales with its frame), in points.
    let size_pt = sh
        .math
        .get(&sh.text)
        .and_then(Result::ok)
        .map(|l| {
            let (w, h) = (l.width.max(0.01), l.total_height().max(0.01));
            ((sh.anchor.width - 8.0) / w).min((sh.anchor.height - 8.0) / h) * 72.0 / 96.0
        })
        .filter(|v| v.is_finite() && *v >= 1.0)
        .unwrap_or(11.0)
        .min(409.0);
    let fill = srgb(&sh.line, theme).map(|c| format!("<a:solidFill>{c}</a:solidFill>")).unwrap_or_default();
    let sz = (size_pt * 100.0).round() as u32;
    // Each box carries the marker of its own body beside the source in its description, the
    // Office Math box and the plain-text fallback alike; the reader takes the source only while
    // the body it reads still matches its marker.
    let tx = |body: &str| {
        format!(
            "<xdr:txBody><a:bodyPr wrap=\"none\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" rtlCol=\"0\" anchor=\"ctr\" anchorCtr=\"1\"/><a:lstStyle/>{body}</xdr:txBody>"
        )
    };
    let marker = |tx: &str| crate::xml::parse(tx.as_bytes()).map(|e| crate::omml::body_marker(&e)).unwrap_or_else(|_| "0".repeat(16));
    let sp = |tx: &str, marker: &str| {
        format!(
            "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"Equation {}\" descr=\"{}\"/><xdr:cNvSpPr txBox=\"1\"/></xdr:nvSpPr><xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></xdr:spPr>{tx}</xdr:sp>",
            id - 1,
            esc_attr(&format!("{LATEX_DESCR}{marker}:{}", sh.text)),
            xfrm(&sh.anchor, "a:xfrm")
        )
    };
    let plain = plain_paragraphs(&sh.text, &format!("<a:rPr lang=\"en-US\" sz=\"{sz}\">{fill}</a:rPr>"));
    match gridcraft_math::to_omml(&sh.text, size_pt, sh.line.resolve(theme)) {
        Ok(omml) => {
            let math = format!(
                "<a:p><a:pPr algn=\"ctr\"/><a14:m><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMathParaPr><m:jc m:val=\"centerGroup\"/></m:oMathParaPr>{omml}</m:oMathPara></a14:m><a:endParaRPr lang=\"en-US\" sz=\"{sz}\"/></a:p>"
            );
            let _ = write!(
                s,
                "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" Requires=\"a14\">{}</mc:Choice><mc:Fallback>{}</mc:Fallback></mc:AlternateContent>",
                sp(&tx(&math), &marker(&tx(&math))),
                sp(&tx(&plain), &marker(&tx(&plain)))
            );
        }
        Err(_) => s.push_str(&sp(&tx(&plain), &marker(&tx(&plain)))),
    }
}

/// The paragraphs of an equation's plain text, one per line of `text`, each a run with `rpr` as
/// its properties: the fallback body, and the body of an equation outside the Office Math subset.
fn plain_paragraphs(text: &str, rpr: &str) -> String {
    text.split('\n').map(|line| format!("<a:p><a:pPr algn=\"ctr\"/><a:r>{rpr}<a:t>{}</a:t></a:r></a:p>", esc(line))).collect()
}

/// The text of a text body: the runs of each paragraph, paragraphs joined with newlines.
fn body_text(tb: &El) -> String {
    tb.kids("p").map(|p| p.kids("r").filter_map(|r| r.child("t")).map(|t| t.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// Whether a body without Office Math still holds the text the writer wrote for `src`: the same
/// paragraphs, read back through the same parser. The marker joins run text across paragraphs,
/// so a paragraph split that changes no character passes it; the text does not.
fn holds_written_text(tb: &El, src: &str) -> bool {
    let written = format!("<xdr:txBody xmlns:xdr=\"x\" xmlns:a=\"a\">{}</xdr:txBody>", plain_paragraphs(src, ""));
    crate::xml::parse(written.as_bytes()).is_ok_and(|w| body_text(&w) == body_text(tb))
}

/// A shape's `a:bodyPr`: text rotation, direction, wrapping and insets as read (absent ones stay
/// absent) and the vertical anchor attribute `anchor`.
fn write_body_pr(s: &mut String, f: &ShapeFormat, anchor: &str) {
    s.push_str("<a:bodyPr");
    if let Some(r) = f.text_rotation {
        let _ = write!(s, " rot=\"{r}\"");
    }
    if let Some(v) = f.vert {
        let _ = write!(s, " vert=\"{}\"", v.xml());
    }
    // Square wrapping is the schema default, so an absent `wrap` stays absent.
    match f.wrap {
        Some(ShapeTextWrap::None) => s.push_str(" wrap=\"none\""),
        Some(ShapeTextWrap::Square) => s.push_str(" wrap=\"square\""),
        None => {}
    }
    for (k, v) in [("lIns", f.inset_left), ("tIns", f.inset_top), ("rIns", f.inset_right), ("bIns", f.inset_bottom)] {
        if let Some(v) = v {
            let _ = write!(s, " {k}=\"{v}\"");
        }
    }
    let _ = write!(s, " rtlCol=\"0\"{anchor}/>");
}

/// The opening `<xdr:twoCellAnchor>` for a model anchor, carrying Excel's `editAs` when the mode
/// is not the default: omitted for "Move and size with cells", `editAs="oneCell"` for "Move but
/// don't size", `editAs="absolute"` for "Don't move or size". (Excel writes pictures this way;
/// the standalone `oneCellAnchor`/`absoluteAnchor` elements are read on import but not written.)
/// Returns the opening tag and the matching closing tag.
fn anchor_open(sheet: &Sheet, a: &Anchor) -> (String, &'static str) {
    let px = |v: f64| ((v.max(0.0)) * EMU_PER_PX).round() as i64;
    let x1 = sheet.col_left(a.cell.col) + a.dx as f64;
    let y1 = sheet.row_top(a.cell.row) + a.dy as f64;
    let x2 = x1 + a.width.max(0.0) as f64;
    let y2 = y1 + a.height.max(0.0) as f64;
    let tc = sheet.col_at(x2);
    let tr = sheet.row_at(y2);
    let tdx = x2 - sheet.col_left(tc);
    let tdy = y2 - sheet.row_top(tr);
    let edit = match a.mode {
        AnchorMode::MoveAndSize => "",
        AnchorMode::MoveOnly => " editAs=\"oneCell\"",
        AnchorMode::Absolute => " editAs=\"absolute\"",
    };
    let open = format!(
        "<xdr:twoCellAnchor{edit}><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from><xdr:to><xdr:col>{tc}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{tr}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to>",
        a.cell.col,
        px(a.dx as f64),
        a.cell.row,
        px(a.dy as f64),
        px(tdx),
        px(tdy)
    );
    (open, "</xdr:twoCellAnchor>")
}

fn xfrm(a: &Anchor, tag: &str) -> String {
    let e = |v: f32| ((v.max(0.0) as f64) * EMU_PER_PX).round() as i64;
    format!("<{tag}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></{tag}>", e(a.width), e(a.height))
}

pub fn srgb(c: &Color, theme: &Theme) -> Option<String> {
    let [r, g, b] = c.resolve(theme)?;
    Some(format!("<a:srgbClr val=\"{r:02X}{g:02X}{b:02X}\"/>"))
}

/// One drawing object to write.
pub enum Obj<'a> {
    /// A picture, its image relationship id, its kept `a:extLst` (relationship ids rewritten) and
    /// the relationship id of its external link (`a:blip r:link`), if it is linked.
    Image(&'a Image, String, String, Option<String>),
    Chart(&'a gridcraft_model::Chart, String),
    Shape(&'a Shape),
}

fn write_hyperlink(link: Option<&DrawingHyperlink>, rels: &mut crate::write::Rels) -> String {
    let Some(link) = link else { return String::new() };
    let rid = if link.target.starts_with('#') { rels.add("hyperlink", &link.target) } else { rels.add_external("hyperlink", &link.target) };
    let tooltip = link.tooltip.as_ref().map(|t| format!(" tooltip=\"{}\"", esc_attr(t))).unwrap_or_default();
    format!("<a:hlinkClick r:id=\"{rid}\"{tooltip}/>")
}

/// The `cNvPr` ids used in kept anchor XML.
fn kept_ids(kept: &[(usize, String)]) -> HashSet<usize> {
    fn walk(e: &El, ids: &mut HashSet<usize>) {
        if e.name == "cNvPr"
            && let Some(id) = e.attr("id").and_then(|v| v.trim().parse::<usize>().ok())
        {
            ids.insert(id);
        }
        for c in &e.children {
            walk(c, ids);
        }
    }
    let mut ids = HashSet::new();
    for (_, x) in kept {
        if let Ok(el) = crate::xml::parse(x.as_bytes()) {
            walk(&el, &mut ids);
        }
    }
    ids
}

fn write_picture_locks(locks: &Option<PictureLocks>, camera: Option<&CameraTool>) -> String {
    let mut inner = match locks {
        None => "<a:picLocks noChangeAspect=\"1\"/>".to_string(),
        Some(l) => {
            let mut attrs = String::new();
            for (name, enabled) in [
                ("noGrp", l.no_grp),
                ("noSelect", l.no_select),
                ("noRot", l.no_rot),
                ("noChangeAspect", l.no_change_aspect),
                ("noMove", l.no_move),
                ("noResize", l.no_resize),
                ("noEditPoints", l.no_edit_points),
                ("noAdjustHandles", l.no_adjust_handles),
                ("noChangeArrowheads", l.no_change_arrowheads),
                ("noChangeShapeType", l.no_change_shape_type),
                ("noCrop", l.no_crop),
            ] {
                if enabled {
                    let _ = write!(attrs, " {name}=\"1\"");
                }
            }
            if attrs.is_empty() { String::new() } else { format!("<a:picLocks{attrs}/>") }
        }
    };
    if let Some(camera) = camera {
        let _ = write!(
            inner,
            "<a:extLst><a:ext uri=\"{{84589F7E-364E-4C9E-8A38-B11213B215E9}}\"><a14:cameraTool xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" cellRange=\"{}\" spid=\"{}\"/></a:ext></a:extLst>",
            esc_attr(&camera.cell_range),
            esc_attr(&camera.spid)
        );
    }
    if inner.is_empty() { "<xdr:cNvPicPr/>".to_string() } else { format!("<xdr:cNvPicPr>{inner}</xdr:cNvPicPr>") }
}

/// An anchor's `xdr:clientData`: `fPrintsWithSheet="0"` when "Print object" is off, else bare
/// (the default, `true`, is not written).
fn client_data(o: &Obj<'_>) -> &'static str {
    let no_print = match o {
        Obj::Image(i, ..) => i.no_print,
        Obj::Chart(c, _) => c.no_print,
        Obj::Shape(s) => s.no_print,
    };
    if no_print { "<xdr:clientData fPrintsWithSheet=\"0\"/>" } else { "<xdr:clientData/>" }
}

/// The `hidden` and `descr` attributes of an object's `cNvPr`, in schema order after `name`.
/// Pictures write their own `descr`; an empty alternative text writes none.
fn cnvpr_flags(o: &Obj<'_>) -> (&'static str, String) {
    let (hidden, descr) = match o {
        Obj::Image(i, ..) => (i.hidden, ""),
        Obj::Chart(c, _) => (c.hidden, c.alt.as_str()),
        Obj::Shape(s) => (s.hidden, s.alt.as_str()),
    };
    let hidden = if hidden { " hidden=\"1\"" } else { "" };
    let descr = if descr.is_empty() { String::new() } else { format!(" descr=\"{}\"", esc_attr(descr)) };
    (hidden, descr)
}

/// The drawing part for a sheet's objects (relationship ids already assigned), with the kept
/// anchors GridCraft doesn't model (`(position, XML)`, relationship ids rewritten) back at their
/// positions. Object ids skip the ids the kept anchors use; adds click hyperlink relationships.
pub fn write_drawing(sheet: &Sheet, theme: &Theme, objs: &[Obj<'_>], kept: &[(usize, String)], rels: &mut crate::write::Rels) -> String {
    let taken = kept_ids(kept);
    let mut next_id = 1usize;
    let mut written = 0usize;
    let mut kept = kept.iter().peekable();
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">",
    );
    for o in objs {
        while let Some((_, x)) = kept.next_if(|(i, _)| *i <= written) {
            s.push_str(x);
            written += 1;
        }
        written += 1;
        next_id = next_id.saturating_add(1);
        while taken.contains(&next_id) {
            next_id = next_id.saturating_add(1);
        }
        let id = next_id;
        let (hidden, descr) = cnvpr_flags(o);
        let close_tag;
        match o {
            Obj::Image(img, rid, ext_list, link_rid) => {
                let blip_end = if ext_list.is_empty() { "/>".to_string() } else { format!(">{ext_list}</a:blip>") };
                let blip_ref = match (rid.is_empty(), link_rid) {
                    (false, Some(l)) => format!("r:embed=\"{rid}\" r:link=\"{l}\""),
                    (false, None) => format!("r:embed=\"{rid}\""),
                    (true, Some(l)) => format!("r:link=\"{l}\""),
                    (true, None) => String::new(),
                };
                let locks = write_picture_locks(&img.locks, img.camera.as_ref());
                let link = write_hyperlink(img.hyperlink.as_ref(), rels);
                let name = esc_attr(&img.name.clone().unwrap_or_else(|| format!("Picture {}", id - 1)));
                let (open, close) = anchor_open(sheet, &img.anchor);
                close_tag = close;
                s.push_str(&open);
                let _ = write!(
                    s,
                    "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{name}\" descr=\"{}\"{hidden}>{link}</xdr:cNvPr>{locks}</xdr:nvPicPr><xdr:blipFill><a:blip {blip_ref}{blip_end}<a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
                    esc_attr(&img.alt),
                    xfrm(&img.anchor, "a:xfrm")
                );
            }
            Obj::Chart(ch, rid) if crate::chartex::is_chartex(ch.kind) => {
                // Chartex frames come with a fallback for readers that don't know them.
                let (open, close) = anchor_open(sheet, &ch.anchor);
                close_tag = close;
                s.push_str(&open);
                let (prefix, ns) = crate::chartex::requires(ch.kind);
                let _ = write!(
                    s,
                    "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:{prefix}=\"{ns}\" Requires=\"{prefix}\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"Chart {}\"{descr}{hidden}/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>{}<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\" r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"0\" name=\"\"{hidden}/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>This chart type can't be shown by this application.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
                    id - 1,
                    xfrm(&ch.anchor, "xdr:xfrm"),
                    xfrm(&ch.anchor, "a:xfrm")
                );
            }
            Obj::Chart(ch, rid) => {
                let (open, close) = anchor_open(sheet, &ch.anchor);
                close_tag = close;
                s.push_str(&open);
                let _ = write!(
                    s,
                    "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"Chart {}\"{descr}{hidden}/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>{}<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame>",
                    id - 1,
                    xfrm(&ch.anchor, "xdr:xfrm")
                );
            }
            Obj::Shape(sh) if sh.kind == ShapeKind::Equation => {
                let (open, close) = anchor_open(sheet, &sh.anchor);
                close_tag = close;
                s.push_str(&open);
                write_equation(&mut s, sh, id, theme);
            }
            Obj::Shape(sh) => {
                let link = write_hyperlink(sh.hyperlink.as_ref(), rels);
                let name = esc_attr(&sh.name.clone().unwrap_or_else(|| format!("Shape {}", id - 1)));
                let (open, close) = anchor_open(sheet, &sh.anchor);
                close_tag = close;
                s.push_str(&open);
                if let Some(xml) = ink::write(sh, theme, id, &link, &descr) {
                    s.push_str(&xml);
                    s.push_str(client_data(o));
                    s.push_str(close_tag);
                    continue;
                }
                let (prst, tx) = match sh.kind {
                    ShapeKind::Rectangle => ("rect", false),
                    ShapeKind::RoundedRectangle => ("roundRect", false),
                    ShapeKind::Ellipse => ("ellipse", false),
                    ShapeKind::Triangle => ("triangle", false),
                    ShapeKind::Line | ShapeKind::Arrow => ("line", false),
                    // Equations are written by `write_equation`; this arm keeps the match whole.
                    ShapeKind::TextBox | ShapeKind::Equation => ("rect", true),
                    ShapeKind::Icon | ShapeKind::Ink | ShapeKind::Freeform => ("rect", false),
                };
                let _ = write!(
                    s,
                    "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"{descr}{hidden}>{link}</xdr:cNvPr><xdr:cNvSpPr{}/></xdr:nvSpPr><xdr:spPr>{}",
                    if tx { " txBox=\"1\"" } else { "" },
                    xfrm(&sh.anchor, "a:xfrm")
                );
                write_geometry(&mut s, prst, sh);
                if sh.format.no_fill {
                    s.push_str("<a:noFill/>");
                } else if let Some(c) = srgb(&sh.fill, theme) {
                    let _ = write!(s, "<a:solidFill>{c}</a:solidFill>");
                }
                let line_c = srgb(&sh.line, theme);
                if sh.format.no_line {
                    s.push_str("<a:ln><a:noFill/></a:ln>");
                } else if line_c.is_some() || sh.kind == ShapeKind::Arrow {
                    // The width is written only when the shape has one, so a shape without it reads back the same.
                    match sh.format.line_width_emu {
                        Some(w) => {
                            let _ = write!(s, "<a:ln w=\"{w}\">");
                        }
                        None => s.push_str("<a:ln>"),
                    }
                    match line_c {
                        Some(c) => {
                            let _ = write!(s, "<a:solidFill>{c}</a:solidFill>");
                        }
                        None => s.push_str("<a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill>"),
                    }
                    if sh.kind == ShapeKind::Arrow {
                        s.push_str("<a:tailEnd type=\"triangle\"/>");
                    }
                    s.push_str("</a:ln>");
                }
                s.push_str("</xdr:spPr>");
                if !sh.text.is_empty() {
                    let f = &sh.format;
                    let anchor = match f.anchor {
                        Some(ShapeTextAnchor::Top) => " anchor=\"t\"",
                        Some(ShapeTextAnchor::Middle) => " anchor=\"ctr\"",
                        Some(ShapeTextAnchor::Bottom) => " anchor=\"b\"",
                        None if sh.kind == ShapeKind::TextBox => " anchor=\"t\"",
                        None => "",
                    };
                    s.push_str("<xdr:txBody>");
                    write_body_pr(&mut s, f, anchor);
                    s.push_str("<a:lstStyle/>");
                    let algn = match f.align {
                        None => "",
                        Some(ShapeTextAlign::Left) => " algn=\"l\"",
                        Some(ShapeTextAlign::Center) => " algn=\"ctr\"",
                        Some(ShapeTextAlign::Right) => " algn=\"r\"",
                    };
                    let rtl = if f.rtl { " rtl=\"1\"" } else { "" };
                    let ppr = if algn.is_empty() && rtl.is_empty() { String::new() } else { format!("<a:pPr{algn}{rtl}/>") };
                    let (rpr_attrs, rpr_body) = run_props(f, theme);
                    for line in sh.text.split('\n') {
                        if line.is_empty() {
                            let _ = write!(s, "<a:p>{ppr}<a:endParaRPr lang=\"en-US\"{rpr_attrs}/></a:p>");
                        } else if rpr_body.is_empty() {
                            let _ = write!(s, "<a:p>{ppr}<a:r><a:rPr lang=\"en-US\"{rpr_attrs}/><a:t>{}</a:t></a:r></a:p>", esc(line));
                        } else {
                            let _ =
                                write!(s, "<a:p>{ppr}<a:r><a:rPr lang=\"en-US\"{rpr_attrs}>{rpr_body}</a:rPr><a:t>{}</a:t></a:r></a:p>", esc(line));
                        }
                    }
                    s.push_str("</xdr:txBody>");
                }
                s.push_str("</xdr:sp>");
            }
        }
        s.push_str(client_data(o));
        s.push_str(close_tag);
    }
    for (_, x) in kept {
        s.push_str(x);
    }
    s.push_str("</xdr:wsDr>");
    s
}

/// A text run's `a:rPr` attributes (size, bold, italic) and children in schema order (outline,
/// fill, typeface).
fn run_props(f: &ShapeFormat, theme: &Theme) -> (String, String) {
    let mut attrs = String::new();
    if let Some(sz) = f.font_size.filter(|v| v.is_finite()) {
        let _ = write!(attrs, " sz=\"{}\"", (sz.clamp(1.0, 4000.0) * 100.0).round() as u32);
    }
    if f.bold {
        attrs.push_str(" b=\"1\"");
    }
    if f.italic {
        attrs.push_str(" i=\"1\"");
    }
    let mut body = String::new();
    if let Some((c, w)) = &f.text_outline
        && let Some(c) = srgb(c, theme)
    {
        let emu = (w.clamp(0.0, 100.0) * 12700.0).round() as u32;
        let _ = write!(body, "<a:ln w=\"{emu}\"><a:solidFill>{c}</a:solidFill></a:ln>");
    }
    if let Some(c) = f.text_color.as_ref().and_then(|c| srgb(c, theme)) {
        let _ = write!(body, "<a:solidFill>{c}</a:solidFill>");
    }
    if let Some(font) = &f.font {
        let _ = write!(body, "<a:latin typeface=\"{}\"/>", esc(font));
    }
    (attrs, body)
}

/// File extension for an image MIME type.
pub fn image_ext(mime: &str, data: &[u8]) -> &'static str {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        "image/x-emf" => "emf",
        "image/x-wmf" => "wmf",
        "image/webp" => "webp",
        _ => {
            if data.starts_with(b"\x89PNG") {
                "png"
            } else if data.starts_with(&[0xFF, 0xD8]) {
                "jpeg"
            } else if data.starts_with(b"GIF8") {
                "gif"
            } else {
                "png"
            }
        }
    }
}

pub fn image_content_type(ext: &str) -> &'static str {
    match ext {
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

#[cfg(test)]
mod theme_override_tests {
    use super::{read_theme_override, valid_theme_override};
    use crate::package::Package;
    use crate::read::Ctx;
    use std::io::{Cursor, Write};

    #[test]
    fn override_requires_one_complete_xml_root() {
        assert!(valid_theme_override(b"<a:themeOverride xmlns:a=\"urn:test\"/>"));
        assert!(valid_theme_override(b"<themeOverride><fontScheme/></themeOverride>"));
        for bad in [
            "<themeOverride>",
            "<themeOverride><fontScheme></themeOverride>",
            "<themeOverride/><themeOverride/>",
            "<themeOverride/>trailing text",
            "<themeOverride bad/>",
            "<themeOverride><![CDATA[unfinished",
        ] {
            assert!(!valid_theme_override(bad.as_bytes()), "{bad}");
        }
    }

    #[test]
    fn override_size_limit_warns_and_keeps_the_workbook_readable() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("xl/charts/_rels/chart1.xml.rels", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(br#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/themeOverride" Target="../theme/themeOverride1.xml"/></Relationships>"#).unwrap();
        zip.start_file("xl/theme/themeOverride1.xml", zip::write::SimpleFileOptions::default()).unwrap();
        let mut payload = b"<themeOverride>".to_vec();
        payload.resize(4 * 1024 * 1024, b' ');
        payload.extend_from_slice(b"</themeOverride>");
        zip.write_all(&payload).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let mut cx = Ctx::new(Package::open(&bytes).unwrap());
        assert!(read_theme_override(&mut cx, "xl/charts/chart1.xml").unwrap().is_none());
        assert_eq!(cx.report.warnings.len(), 1);
        assert!(cx.report.warnings.first().unwrap().contains("theme override"));
        assert!(cx.pkg.read("xl/charts/_rels/chart1.xml.rels").unwrap().is_some());
    }
}

#[cfg(test)]
mod custom_geometry_tests {
    use super::{read_custom_paths, write_custom_geometry};
    use gridcraft_model::PathCommand;

    const GEOM: &str = r#"<a:custGeom><a:pathLst><a:path w="100" h="50" fill="none" stroke="0"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:cubicBezTo><a:pt x="10" y="0"/><a:pt x="20" y="50"/><a:pt x="30" y="50"/></a:cubicBezTo><a:quadBezTo><a:pt x="40" y="0"/><a:pt x="50" y="25"/></a:quadBezTo><a:arcTo wR="10" hR="5" stAng="0" swAng="5400000"/><a:close/></a:path><a:path w="10" h="10"><a:moveTo><a:pt x="1" y="2"/></a:moveTo></a:path></a:pathLst></a:custGeom>"#;

    #[test]
    fn every_path_command_survives_a_write_and_read() {
        let el = crate::xml::parse(GEOM.as_bytes()).unwrap();
        let paths = read_custom_paths(&el).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(!paths[0].fill && !paths[0].stroke && paths[1].fill && paths[1].stroke);
        assert_eq!(paths[0].commands[1], PathCommand::CubicBezTo([(10, 0), (20, 50), (30, 50)]));
        assert_eq!(paths[0].commands[3], PathCommand::ArcTo { w_r: 10, h_r: 5, st_ang: 0, sw_ang: 5_400_000 });
        let mut xml = String::new();
        write_custom_geometry(&mut xml, &paths);
        let again = read_custom_paths(&crate::xml::parse(xml.as_bytes()).unwrap()).unwrap();
        assert_eq!(again, paths);
    }

    #[test]
    fn guide_named_coordinates_keep_the_rectangle_fallback() {
        let guided = GEOM.replace(r#"<a:pt x="10" y="0"/>"#, r#"<a:pt x="x1" y="0"/>"#);
        assert!(read_custom_paths(&crate::xml::parse(guided.as_bytes()).unwrap()).is_none());
        assert!(read_custom_paths(&crate::xml::parse(b"<a:custGeom><a:pathLst/></a:custGeom>").unwrap()).is_none());
    }

    #[test]
    fn path_fill_modes_read_as_fill_and_shade() {
        use gridcraft_model::PathShade;
        let xml = r#"<a:custGeom><a:pathLst><a:path fill="norm"/><a:path fill="darken"/><a:path fill="bogus"/><a:path fill="none"/></a:pathLst></a:custGeom>"#;
        let paths = read_custom_paths(&crate::xml::parse(xml.as_bytes()).unwrap()).unwrap();
        let got: Vec<_> = paths.iter().map(|p| (p.fill, p.shade)).collect();
        assert_eq!(got, [(true, PathShade::Norm), (true, PathShade::Darken), (true, PathShade::Norm), (false, PathShade::Norm)]);
    }
}

#[cfg(test)]
mod colour_transform_tests {
    use super::dml_color_resolved;
    use gridcraft_model::{Color, Theme};

    fn resolved(fill: &str) -> Option<Color> {
        let xml = format!(r#"<a:solidFill xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{fill}</a:solidFill>"#);
        let el = crate::xml::parse(xml.as_bytes()).unwrap();
        dml_color_resolved(&el, &Theme::default().colors)
    }

    #[test]
    fn shade_is_applied_in_linear_light() {
        assert_eq!(resolved(r#"<a:schemeClr val="lt1"><a:shade val="50000"/></a:schemeClr>"#), Some(Color::Rgb(0xBCBCBC)));
        assert_eq!(resolved(r#"<a:srgbClr val="FFFFFF"><a:shade val="85000"/></a:srgbClr>"#), Some(Color::Rgb(0xEDEDED)));
        assert_eq!(resolved(r#"<a:srgbClr val="4F81BD"><a:shade val="50000"/></a:srgbClr>"#), Some(Color::Rgb(0x385D8A)));
    }

    #[test]
    fn tint_mirrors_shade_and_plain_colours_are_unchanged() {
        assert_eq!(resolved(r#"<a:srgbClr val="000000"><a:tint val="50000"/></a:srgbClr>"#), Some(Color::Rgb(0xBCBCBC)));
        assert_eq!(resolved(r#"<a:schemeClr val="accent1"/>"#), Some(Color::Theme(4, 0)));
        assert_eq!(resolved(r#"<a:srgbClr val="FF0000"/>"#), Some(Color::Rgb(0xFF0000)));
        // Hostile values are clamped, never a crash.
        assert_eq!(resolved(r#"<a:srgbClr val="808080"><a:shade val="-5"/><a:tint val="NaN"/></a:srgbClr>"#), Some(Color::Rgb(0)));
    }
}
