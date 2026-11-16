//! EMF pictures ([MS-EMF]) are interpreted for print: shapes become paths in picture-relative
//! coordinates (0..1 across the header's frame), bitmaps become RGBA images.
//!
//! Every EMF here has the frame (0, 0)-(1000, 500) in 0.01 mm on a device of 1000 x 1000 pixels
//! that is 100 x 100 mm, so one pixel is 0.1 mm and the frame is 100 x 50 pixels.

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
    head.extend(i32s(&[0, 0, 99, 49])); // rclBounds (device pixels)
    head.extend(i32s(&[0, 0, 1000, 500])); // rclFrame (0.01 mm)
    head.extend(0x464D_4520u32.to_le_bytes()); // " EMF"
    head.extend(0x0001_0000u32.to_le_bytes()); // version
    head.extend((total as u32).to_le_bytes()); // nBytes
    head.extend(((records.len() + 2) as u32).to_le_bytes()); // nRecords
    head.extend([4, 0, 0, 0]); // nHandles, reserved
    head.extend(i32s(&[0, 0, 0])); // nDescription, offDescription, nPalEntries
    head.extend(i32s(&[1000, 1000])); // szlDevice
    head.extend(i32s(&[100, 100])); // szlMillimeters
    [rec(1, &head), body, eof].concat()
}

/// CREATEBRUSHINDIRECT: handle `ih`, BS_SOLID (0) or BS_NULL (1), COLORREF 0x00BBGGRR.
fn brush(ih: i32, style: i32, colorref: u32) -> Vec<u8> {
    [i32s(&[ih, style]), colorref.to_le_bytes().to_vec(), i32s(&[0])].concat()
}

fn select(ih: u32) -> Vec<u8> {
    rec(37, &ih.to_le_bytes())
}

const NULL_BRUSH: u32 = 0x8000_0005;
const NULL_PEN: u32 = 0x8000_0008;

/// A path's subpaths, fill and stroke.
type PathView = (Vec<Vec<[f32; 2]>>, Option<[u8; 3]>, Option<([u8; 3], f32)>);

fn paths(d: &[Item]) -> Vec<PathView> {
    d.iter()
        .filter_map(|i| match i {
            Item::Path { subpaths, fill, stroke, .. } => Some((subpaths.clone(), *fill, *stroke)),
            _ => None,
        })
        .collect()
}

fn bbox(sub: &[Vec<[f32; 2]>]) -> [f32; 4] {
    let pts = sub.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    [x0, y0, x1, y1]
}

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.02)
}

#[test]
fn non_emf_bytes_are_not_parsed() {
    assert!(parse_emf(b"\x89PNG\r\n\x1a\nnot a metafile at all, just some bytes padding it past the header size....").is_none());
    assert!(parse_emf(&[]).is_none());
}

#[test]
fn solid_red_rectangle_with_null_pen_is_a_filled_path_in_frame_units() {
    // Device pixels (10,10)-(50,30) of a 100 x 50 frame -> (0.1, 0.2)-(0.5, 0.6).
    let d = parse_emf(&emf(&[rec(39, &brush(1, 0, 0x0000_00FF)), select(1), select(NULL_PEN), rec(43, &i32s(&[10, 10, 50, 30]))]))
        .expect("an EMF parses");
    let p = paths(&d.items);
    assert_eq!(p.len(), 1, "one rectangle: {p:?}");
    assert_eq!(p[0].1, Some([255, 0, 0]), "COLORREF 0x000000FF is red");
    assert!(p[0].2.is_none(), "NULL_PEN: no outline");
    let b = bbox(&p[0].0);
    assert!(near(b, [0.1, 0.2, 0.5, 0.6]), "rectangle bounds {b:?}");
}

#[test]
fn null_brush_rectangle_has_only_the_default_black_outline() {
    let d = parse_emf(&emf(&[select(NULL_BRUSH), rec(43, &i32s(&[0, 0, 100, 50]))])).expect("an EMF parses");
    let p = paths(&d.items);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].1.is_none(), "NULL_BRUSH: no fill");
    assert_eq!(p[0].2.map(|s| s.0), Some([0, 0, 0]), "the default pen is black");
}

#[test]
fn anisotropic_window_and_viewport_map_logical_units_onto_the_frame() {
    // MM_ANISOTROPIC: logical 200 x 200 window onto the 100 x 50 pixel viewport, so the polygon
    // (0,0)-(200,200) covers the whole frame. Default brush is white, default pen black.
    let poly =
        [i32s(&[0, 0, 200, 200]), 4u32.to_le_bytes().to_vec(), [0i16, 0, 200, 0, 200, 200, 0, 200].iter().flat_map(|v| v.to_le_bytes()).collect()]
            .concat();
    let d = parse_emf(&emf(&[rec(17, &i32s(&[8])), rec(9, &i32s(&[200, 200])), rec(11, &i32s(&[100, 50])), rec(86, &poly)])).expect("an EMF parses");
    let p = paths(&d.items);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].1, Some([255, 255, 255]), "the default brush is white");
    assert!(p[0].2.is_some(), "the default pen draws");
    let b = bbox(&p[0].0);
    assert!(near(b, [0.0, 0.0, 1.0, 1.0]), "polygon bounds {b:?}");
}

#[test]
fn stretchdibits_24bpp_bitmap_becomes_an_rgba_image_over_its_destination() {
    // A 2 x 1 BI_RGB 24-bpp DIB (blue pixel, then green; rows padded to 4 bytes) stretched over
    // the whole 100 x 50 pixel frame.
    let bmi = [i32s(&[40, 2, 1]), [1u16, 24].iter().flat_map(|v| v.to_le_bytes()).collect(), i32s(&[0, 8, 0, 0, 0, 0])].concat();
    let bits: Vec<u8> = vec![255, 0, 0, 0, 255, 0, 0, 0];
    let fixed = 80u32;
    let body = [
        i32s(&[0, 0, 99, 49]),
        i32s(&[0, 0, 0, 0, 2, 1]),
        [fixed, bmi.len() as u32, fixed + bmi.len() as u32, bits.len() as u32, 0, 0x00CC_0020].iter().flat_map(|v| v.to_le_bytes()).collect(),
        i32s(&[100, 50]),
        bmi,
        bits,
    ]
    .concat();
    let d = parse_emf(&emf(&[rec(81, &body)])).expect("an EMF parses");
    let imgs: Vec<_> = d
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Image { rect, rgba, size } => Some((*rect, rgba.clone(), *size)),
            _ => None,
        })
        .collect();
    assert_eq!(imgs.len(), 1, "one bitmap");
    let (rect, rgba, size) = &imgs[0];
    assert_eq!(*size, [2, 1]);
    assert_eq!(rgba, &vec![0, 0, 255, 255, 0, 255, 0, 255], "DIB bytes are blue, green, red order");
    assert!(near(*rect, [0.0, 0.0, 1.0, 1.0]), "image rect {rect:?}");
}

#[test]
fn records_after_an_unknown_record_still_draw() {
    // Record type 70 (EMR_COMMENT, e.g. an EMF+ block) and an unknown type 9999 are skipped by size.
    let d = parse_emf(&emf(&[rec(70, &i32s(&[4, 0x2B46_4D45])), rec(9999, &[0; 12]), select(NULL_PEN), rec(43, &i32s(&[0, 0, 50, 25]))]))
        .expect("an EMF parses");
    let p = paths(&d.items);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(near(bbox(&p[0].0), [0.0, 0.0, 0.5, 0.5]));
}
