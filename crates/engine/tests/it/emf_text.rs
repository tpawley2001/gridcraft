//! EMF text records ([MS-EMF] EXTCREATEFONTINDIRECTW, EXTTEXTOUTW, SETTEXTALIGN, SETTEXTCOLOR,
//! SETBKCOLOR) become `Item::Text` runs for print.
//!
//! Every EMF here has the frame (0, 0)-(1000, 500) in 0.01 mm on a device of 1000 x 1000 pixels
//! that is 100 x 100 mm, so one pixel is 0.1 mm and the frame is 100 x 50 pixels (MM_TEXT, so
//! logical units = device pixels). Relative x = px / 100, relative y = px / 50.

use gridcraft_engine::metafile::{Item, parse_emf};

fn i32s(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn rec(ty: u32, body: &[u8]) -> Vec<u8> {
    [&ty.to_le_bytes()[..], &((body.len() + 8) as u32).to_le_bytes(), body].concat()
}

/// A whole EMF: header, `records`, EOF.
fn emf(records: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = records.concat();
    let eof = rec(14, &i32s(&[0, 16, 20]));
    let total = 88 + body.len() + eof.len();
    let mut head = Vec::new();
    head.extend(i32s(&[0, 0, 99, 49]));
    head.extend(i32s(&[0, 0, 1000, 500]));
    head.extend(0x464D_4520u32.to_le_bytes());
    head.extend(0x0001_0000u32.to_le_bytes());
    head.extend((total as u32).to_le_bytes());
    head.extend(((records.len() + 2) as u32).to_le_bytes());
    head.extend([4, 0, 0, 0]);
    head.extend(i32s(&[0, 0, 0]));
    head.extend(i32s(&[1000, 1000]));
    head.extend(i32s(&[100, 100]));
    [rec(1, &head), body, eof].concat()
}

/// EXTCREATEFONTINDIRECTW: handle `ih`, LOGFONTW with `height`, `escapement` (tenths of a
/// degree), `weight`, `italic` and `face`.
fn font(ih: u32, height: i32, escapement: i32, weight: i32, italic: bool, face: &str) -> Vec<u8> {
    let mut b = ih.to_le_bytes().to_vec();
    b.extend(i32s(&[height, 0, escapement, escapement, weight]));
    b.extend([u8::from(italic), 0, 0, 1, 0, 0, 0, 0]); // italic, underline, strikeout, DEFAULT_CHARSET, ...
    let mut name = [0u16; 32];
    for (i, u) in face.encode_utf16().take(31).enumerate() {
        name[i] = u;
    }
    b.extend(name.iter().flat_map(|u| u.to_le_bytes()));
    rec(82, &b)
}

fn select(ih: u32) -> Vec<u8> {
    rec(37, &ih.to_le_bytes())
}

fn u32rec(ty: u32, v: u32) -> Vec<u8> {
    rec(ty, &v.to_le_bytes())
}

/// EXTTEXTOUTW at (`x`, `y`) with `options`, `rcl` and optional per-character advances.
fn text_out(x: i32, y: i32, s: &str, options: u32, rcl: [i32; 4], dx: Option<&[i32]>) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let str_bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    let pad = (4 - str_bytes.len() % 4) % 4;
    let off_string = 76u32;
    let off_dx = if dx.is_some() { off_string + (str_bytes.len() + pad) as u32 } else { 0 };
    let mut b = i32s(&[0, 0, 99, 49]); // rclBounds
    b.extend(1u32.to_le_bytes()); // GM_COMPATIBLE
    b.extend(1.0f32.to_le_bytes());
    b.extend(1.0f32.to_le_bytes());
    b.extend(i32s(&[x, y]));
    b.extend((units.len() as u32).to_le_bytes());
    b.extend(off_string.to_le_bytes());
    b.extend(options.to_le_bytes());
    b.extend(i32s(&rcl));
    b.extend(off_dx.to_le_bytes());
    b.extend(&str_bytes);
    b.extend(vec![0u8; pad]);
    if let Some(dx) = dx {
        b.extend(i32s(dx));
    }
    rec(84, &b)
}

const TA_BASELINE: u32 = 24;
const TA_RIGHT: u32 = 2;
const ETO_OPAQUE: u32 = 0x02;
const ETO_GLYPH_INDEX: u32 = 0x10;

#[derive(Debug)]
struct Run {
    origin: [f32; 2],
    size: f32,
    angle: f32,
    text: String,
    face: String,
    bold: bool,
    italic: bool,
    color: [u8; 3],
}

fn runs(items: &[Item]) -> Vec<Run> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Text { origin, size, angle, text, face, bold, italic, color } => Some(Run {
                origin: *origin,
                size: *size,
                angle: *angle,
                text: text.clone(),
                face: face.clone(),
                bold: *bold,
                italic: *italic,
                color: *color,
            }),
            _ => None,
        })
        .collect()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

fn one_run(records: &[Vec<u8>]) -> Run {
    let d = parse_emf(&emf(records)).expect("valid EMF");
    let mut r = runs(&d.items);
    assert_eq!(r.len(), 1, "expected one text run, got {r:?}");
    r.remove(0)
}

#[test]
fn baseline_aligned_run_keeps_font_colour_and_position() {
    let r = one_run(&[
        font(1, -20, 0, 700, false, "Arial"),
        select(1),
        u32rec(22, TA_BASELINE),
        u32rec(24, 0x0000_00FF), // COLORREF red
        text_out(10, 30, "Hi", 0, [0, 0, 0, 0], None),
    ]);
    assert_eq!(r.text, "Hi");
    assert_eq!(r.face, "Arial");
    assert!(r.bold && !r.italic);
    assert_eq!(r.color, [255, 0, 0]);
    assert!(close(r.origin[0], 0.10, 0.005) && close(r.origin[1], 0.60, 0.005), "{r:?}");
    assert!(close(r.size, 0.40, 0.005), "em 20 px / frame 50 px: {r:?}");
    assert!(close(r.angle, 0.0, 0.01));
}

#[test]
fn top_aligned_run_baseline_is_one_ascent_down() {
    // Default TA_TOP: baseline = 10 + 0.905 * 20 = 28.1 px -> 0.562.
    let r = one_run(&[font(1, -20, 0, 400, true, "Times New Roman"), select(1), text_out(10, 10, "x", 0, [0, 0, 0, 0], None)]);
    assert!(close(r.origin[1], 0.562, 0.02), "{r:?}");
    assert!(close(r.origin[0], 0.10, 0.005));
    assert!(!r.bold && r.italic);
    assert_eq!(r.color, [0, 0, 0], "default text colour is black");
}

#[test]
fn right_aligned_run_ends_at_reference_using_advances() {
    // "abc" with advances 10 + 10 + 10 = 30 px, right edge at x = 80 -> starts at 50 px = 0.5.
    let r = one_run(&[
        font(1, -20, 0, 400, false, "Arial"),
        select(1),
        u32rec(22, TA_RIGHT | TA_BASELINE),
        text_out(80, 30, "abc", 0, [0, 0, 0, 0], Some(&[10, 10, 10])),
    ]);
    assert!(close(r.origin[0], 0.50, 0.005), "{r:?}");
}

#[test]
fn escapement_rotates_the_run() {
    let r = one_run(&[font(1, -20, 900, 400, false, "Arial"), select(1), u32rec(22, TA_BASELINE), text_out(50, 40, "Up", 0, [0, 0, 0, 0], None)]);
    assert!(close(r.angle, 90.0, 0.5), "{r:?}");
    assert!(close(r.origin[0], 0.50, 0.005) && close(r.origin[1], 0.80, 0.005));
}

#[test]
fn positive_height_is_cell_height() {
    // Cell height 23 -> em 23 / 1.15 = 20 px -> 0.4 of the 50 px frame.
    let r = one_run(&[font(1, 23, 0, 400, false, "Arial"), select(1), u32rec(22, TA_BASELINE), text_out(10, 30, "a", 0, [0, 0, 0, 0], None)]);
    assert!(close(r.size, 0.40, 0.01), "{r:?}");
}

#[test]
fn opaque_option_fills_background_before_text() {
    let d = parse_emf(&emf(&[
        font(1, -20, 0, 400, false, "Arial"),
        select(1),
        u32rec(25, 0x0000_FF00), // background green
        u32rec(22, TA_BASELINE),
        text_out(10, 20, "bg", ETO_OPAQUE, [0, 0, 50, 25], None),
    ]))
    .expect("valid EMF");
    let fill_at = d.items.iter().position(|i| matches!(i, Item::Path { fill: Some([0, 255, 0]), .. }));
    let text_at = d.items.iter().position(|i| matches!(i, Item::Text { .. }));
    let (Some(f), Some(t)) = (fill_at, text_at) else { panic!("need a green fill and a text run: {:?}", d.items.len()) };
    assert!(f < t, "background first");
    if let Item::Path { subpaths, .. } = &d.items[f] {
        let xs: Vec<f32> = subpaths.iter().flatten().map(|p| p[0]).collect();
        let ys: Vec<f32> = subpaths.iter().flatten().map(|p| p[1]).collect();
        let max_x = xs.iter().copied().fold(f32::MIN, f32::max);
        let max_y = ys.iter().copied().fold(f32::MIN, f32::max);
        assert!(close(max_x, 0.5, 0.03) && close(max_y, 0.5, 0.03), "rect 0..50 x 0..25 px");
    }
}

#[test]
fn glyph_index_runs_and_bad_offsets_are_skipped() {
    let glyphs = parse_emf(&emf(&[font(1, -20, 0, 400, false, "Arial"), select(1), text_out(10, 30, "AB", ETO_GLYPH_INDEX, [0, 0, 0, 0], None)]))
        .expect("valid EMF");
    assert!(runs(&glyphs.items).is_empty());
    // offString pointing past the record: no run, no panic.
    let mut bad = text_out(10, 30, "AB", 0, [0, 0, 0, 0], None);
    bad[48..52].copy_from_slice(&10_000u32.to_le_bytes());
    let d = parse_emf(&emf(&[font(1, -20, 0, 400, false, "Arial"), select(1), bad])).expect("valid EMF");
    assert!(runs(&d.items).is_empty());
}

#[test]
fn restoredc_restores_the_selected_font() {
    let r = one_run(&[
        font(1, -20, 0, 400, false, "Arial"),
        font(2, -20, 0, 400, false, "Courier New"),
        select(1),
        rec(33, &[]),
        select(2),
        rec(34, &(-1i32).to_le_bytes()),
        u32rec(22, TA_BASELINE),
        text_out(10, 30, "a", 0, [0, 0, 0, 0], None),
    ]);
    assert_eq!(r.face, "Arial");
}
