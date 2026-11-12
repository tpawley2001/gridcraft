//! A shape's outer shadow (`a:outerShdw`, ECMA-376 Part 1 §20.1.8.45) prints as a copy of the
//! shape's outline moved by `dist` along `dir`, in the shadow colour, under the shape.

use gridcraft_model::{Color, KeptDmlColor, OuterShadow, ShapeEffects};
use serde_json::json;

use super::*;

/// A session with one red-filled, blue-outlined rectangle at B2.
fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "x"})).unwrap();
    s.execute("insert.shape", json!({"kind": "rectangle", "at": "B2", "width": 144, "height": 96, "fill": "#FF0000", "line": "#0000FF"})).unwrap();
    s
}

fn set_effects(s: &mut Session, effects: Option<ShapeEffects>) {
    let d = s.doc_mut().unwrap();
    let sh = std::sync::Arc::make_mut(&mut d.wb).sheet_mut(0).unwrap().shapes.last_mut().unwrap();
    sh.format.effects = effects;
}

fn shadow(dist: Option<i64>, dir: Option<i32>, color: Option<KeptDmlColor>) -> Option<ShapeEffects> {
    Some(ShapeEffects::OuterShadow(OuterShadow {
        blur_rad: Some(50800),
        dist,
        dir,
        algn: Some("tl".into()),
        rot_with_shape: Some(false),
        color,
        ..Default::default()
    }))
}

fn srgb(rgb: u32, alpha: Option<u32>) -> Option<KeptDmlColor> {
    let xml = match alpha {
        Some(a) => format!("<a:srgbClr val=\"{rgb:06X}\"><a:alpha val=\"{a}\"/></a:srgbClr>"),
        None => format!("<a:srgbClr val=\"{rgb:06X}\"/>"),
    };
    Some(KeptDmlColor { color: Color::Rgb(rgb), xml })
}

/// `shape_shadow` of the session's shape at `scale`.
fn printed(s: &mut Session, scale: f32) -> Option<((f32, f32), [u8; 3])> {
    let d = s.doc().unwrap();
    let sh = d.wb.sheet(0).unwrap().shapes.last().unwrap();
    objects::shape_shadow(&d.wb, sh, scale)
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

fn page_stream(s: &mut Session) -> String {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let bytes = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The vertices (`x y m` / `x y l`) of every path painted after the fill colour operator `op`.
fn filled_paths(text: &str, op: &str) -> Vec<Vec<(f32, f32)>> {
    let mut out = Vec::new();
    for part in text.split(op).skip(1) {
        let end = [" B", " f", " S"].iter().filter_map(|o| part.find(o)).min().unwrap_or(part.len());
        let toks: Vec<&str> = part[..end].split_whitespace().collect();
        let mut pts = Vec::new();
        for i in 2..toks.len() {
            if (toks[i] == "m" || toks[i] == "l")
                && let (Ok(x), Ok(y)) = (toks[i - 2].parse::<f32>(), toks[i - 1].parse::<f32>())
            {
                pts.push((x, y));
            }
        }
        out.push(pts);
    }
    out
}

fn sorted(mut v: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| near(*a, *b));
    v
}

/// Excel's default shadow: 3 pt down-right, black at 40 %.
#[test]
fn the_default_shadow_is_3_pt_down_right_in_black_at_40_percent() {
    let mut s = session();
    set_effects(&mut s, shadow(Some(38100), Some(2_700_000), srgb(0x000000, Some(40000))));
    let (off, rgb) = printed(&mut s, 1.0).expect("a shadow to print");
    assert!(near(off, (2.1213, 2.1213)), "{off:?}");
    assert_eq!(rgb, [153, 153, 153], "black at 40 % over white");
}

#[test]
fn a_preset_black_shadow_colour_is_black() {
    let mut s = session();
    let color = KeptDmlColor { color: Color::default(), xml: "<a:prstClr val=\"black\"><a:alpha val=\"40000\"/></a:prstClr>".into() };
    set_effects(&mut s, shadow(Some(38100), Some(2_700_000), Some(color)));
    let (off, rgb) = printed(&mut s, 1.0).expect("a shadow to print");
    assert!(near(off, (2.1213, 2.1213)), "{off:?}");
    assert_eq!(rgb, [153, 153, 153]);
}

/// `dir` turns clockwise on the page: 0 right, 90 degrees down, 270 degrees up; the offset scales with the printout.
#[test]
fn the_direction_turns_clockwise_and_the_offset_scales() {
    let mut s = session();
    for (dir, want) in [(0, (3.0, 0.0)), (5_400_000, (0.0, 3.0)), (16_200_000, (0.0, -3.0))] {
        set_effects(&mut s, shadow(Some(38100), Some(dir), srgb(0x00FF00, None)));
        let (off, rgb) = printed(&mut s, 1.0).expect("a shadow to print");
        assert!(near(off, want), "dir {dir}: {off:?}");
        assert_eq!(rgb, [0, 255, 0], "no alpha: the colour itself");
    }
    set_effects(&mut s, shadow(Some(38100), Some(0), srgb(0x00FF00, None)));
    let (off, _) = printed(&mut s, 0.5).expect("a shadow to print");
    assert!(near(off, (1.5, 0.0)), "{off:?}");
    // `dir` absent is 0: to the right.
    set_effects(&mut s, shadow(Some(38100), None, srgb(0x00FF00, None)));
    let (off, _) = printed(&mut s, 1.0).expect("a shadow to print");
    assert!(near(off, (3.0, 0.0)), "{off:?}");
}

#[test]
fn nothing_prints_without_a_distance_a_colour_or_a_shadow() {
    let mut s = session();
    assert_eq!(printed(&mut s, 1.0), None, "no effects");
    set_effects(&mut s, Some(ShapeEffects::None));
    assert_eq!(printed(&mut s, 1.0), None, "an empty effect list");
    set_effects(&mut s, shadow(None, Some(2_700_000), srgb(0x000000, Some(40000))));
    assert_eq!(printed(&mut s, 1.0), None, "no distance");
    set_effects(&mut s, shadow(Some(0), Some(2_700_000), srgb(0x000000, Some(40000))));
    assert_eq!(printed(&mut s, 1.0), None, "distance 0");
    set_effects(&mut s, shadow(Some(38100), Some(2_700_000), None));
    assert_eq!(printed(&mut s, 1.0), None, "no colour");
}

/// Guard: without a shadow the rectangle prints its red fill and nothing green.
#[test]
fn a_shape_without_a_shadow_prints_no_shadow_fill() {
    let mut s = session();
    let text = page_stream(&mut s);
    let red = filled_paths(&text, "1 0 0 rg\n");
    assert_eq!(red.len(), 1, "{red:?}");
    assert_eq!(sorted(red[0].clone()).len(), 4, "the rectangle's four corners: {red:?}");
    assert!(filled_paths(&text, "0 1 0 rg\n").is_empty());
}

/// The shadow is the shape's own outline, moved, painted before the shape.
#[test]
fn the_shadow_prints_as_the_moved_outline_under_the_shape() {
    let mut s = session();
    set_effects(&mut s, shadow(Some(38100), Some(2_700_000), srgb(0x00FF00, None)));
    let text = page_stream(&mut s);
    let red = filled_paths(&text, "1 0 0 rg\n");
    let green = filled_paths(&text, "0 1 0 rg\n");
    assert_eq!(red.len(), 1, "{red:?}");
    assert_eq!(green.len(), 1, "one shadow path: {green:?}");
    assert!(text.find("0 1 0 rg\n").unwrap() < text.find("1 0 0 rg\n").unwrap(), "the shadow is painted first");
    let (red, green) = (sorted(red[0].clone()), sorted(green[0].clone()));
    assert_eq!(red.len(), 4);
    assert_eq!(green.len(), 4, "{green:?}");
    for (r, g) in red.iter().zip(&green) {
        // PDF space has y up: down the page is a smaller y.
        assert!(near(*g, (r.0 + 2.1213, r.1 - 2.1213)), "shape {r:?}, shadow {g:?}");
    }
}
