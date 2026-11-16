//! Unit tests of the EMF interpreter. Each EMF has the frame (0, 0)-(1000, 500) in 0.01 mm on a
//! 1000 x 1000 pixel, 100 x 100 mm device: the frame is 100 x 50 device pixels.

use super::*;

fn i32s(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn i16s(v: &[i16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn rec(ty: u32, body: &[u8]) -> Vec<u8> {
    [&ty.to_le_bytes()[..], &((body.len() + 8) as u32).to_le_bytes(), body].concat()
}
fn emf(records: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = records.concat();
    let mut head = i32s(&[0, 0, 99, 49, 0, 0, 1000, 500]);
    head.extend(u32s(&[SIGNATURE, 0x0001_0000, 0, 0, 0, 0, 0, 0]));
    head.extend(i32s(&[1000, 1000, 100, 100]));
    [rec(1, &head), body, rec(14, &i32s(&[0, 16, 20]))].concat()
}
fn brush(ih: i32, style: i32, colorref: u32) -> Vec<u8> {
    rec(39, &[i32s(&[ih, style]), u32s(&[colorref, 0])].concat())
}
fn select(ih: u32) -> Vec<u8> {
    rec(37, &u32s(&[ih]))
}
fn poly16(ty: u32, pts: &[i16]) -> Vec<u8> {
    rec(ty, &[i32s(&[0, 0, 0, 0]), u32s(&[(pts.len() / 2) as u32]), i16s(pts)].concat())
}
/// A path's subpaths, closed flag, fill, stroke and fill rule.
type PathView = (Vec<Vec<[f32; 2]>>, bool, Option<[u8; 3]>, Option<([u8; 3], f32)>, bool);

fn paths(d: &Drawing) -> Vec<PathView> {
    d.items
        .iter()
        .filter_map(|i| match i {
            Item::Path { subpaths, closed, fill, stroke, even_odd } => Some((subpaths.clone(), *closed, *fill, *stroke, *even_odd)),
            Item::Image { .. } => None,
        })
        .collect()
}
fn bbox(sub: &[Vec<[f32; 2]>]) -> [f32; 4] {
    sub.iter().flatten().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}
fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.02)
}
const NULL_PEN: u32 = 0x8000_0008;
const NULL_BRUSH: u32 = 0x8000_0005;

#[test]
fn ellipse_is_flattened_inside_its_box() {
    let d = parse_emf(&emf(&[rec(42, &i32s(&[0, 0, 100, 50]))])).unwrap();
    let p = paths(&d);
    assert_eq!(p.len(), 1);
    let pts = &p[0].0[0];
    assert!(pts.len() >= 32, "{} points", pts.len());
    assert!(near(bbox(&p[0].0), [0.0, 0.0, 1.0, 1.0]));
    // Every point lies on the ellipse centred at (0.5, 0.5) with radii 0.5.
    assert!(pts.iter().all(|q| (((q[0] - 0.5) / 0.5).powi(2) + ((q[1] - 0.5) / 0.5).powi(2) - 1.0).abs() < 1e-3));
    assert_eq!(p[0].2, Some([255, 255, 255]), "default white brush");
    let (black, w) = p[0].3.unwrap();
    assert_eq!(black, [0, 0, 0]);
    assert!((w - 0.01).abs() < 1e-6, "1 device pixel of a 100-pixel frame: {w}");
}

#[test]
fn polybezier_is_flattened_through_its_end_points() {
    let d = parse_emf(&emf(&[poly16(85, &[0, 0, 0, 50, 100, 50, 100, 0])])).unwrap();
    let p = paths(&d);
    assert_eq!(p.len(), 1);
    let (sub, closed, fill, stroke, _) = &p[0];
    assert!(!closed && fill.is_none() && stroke.is_some());
    let pts = &sub[0];
    assert!(pts.len() > 4, "flattened");
    assert_eq!(pts.first(), Some(&[0.0, 0.0]));
    assert_eq!(pts.last(), Some(&[1.0, 0.0]));
    // The curve's midpoint is at (50, 37.5) device pixels.
    let mid = pts[pts.len() / 2];
    assert!((mid[0] - 0.5).abs() < 0.01 && (mid[1] - 0.75).abs() < 0.01, "{mid:?}");
}

#[test]
fn fillpath_fills_the_figures_built_between_beginpath_and_endpath() {
    let d = parse_emf(&emf(&[
        brush(1, 0, 0x0000_FF00),
        select(1),
        rec(19, &u32s(&[2])), // WINDING
        rec(59, &[]),
        rec(27, &i32s(&[10, 10])),
        rec(54, &i32s(&[90, 10])),
        rec(54, &i32s(&[90, 40])),
        rec(61, &[]),
        rec(43, &i32s(&[20, 20, 30, 30])),
        rec(60, &[]),
        rec(62, &i32s(&[0, 0, 0, 0])),
        // Nothing left to fill: the path was consumed.
        rec(62, &i32s(&[0, 0, 0, 0])),
    ]))
    .unwrap();
    let p = paths(&d);
    assert_eq!(p.len(), 1, "{p:?}");
    let (sub, closed, fill, stroke, even_odd) = &p[0];
    assert!(*closed && stroke.is_none() && !even_odd);
    assert_eq!(*fill, Some([0, 255, 0]));
    assert_eq!(sub.len(), 2, "a triangle and a rectangle");
    assert_eq!(sub[0].len(), 3);
    assert!(near(bbox(&sub[0..1]), [0.1, 0.2, 0.9, 0.8]));
}

#[test]
fn strokepath_keeps_open_figures_open() {
    let d =
        parse_emf(&emf(&[rec(59, &[]), rec(27, &i32s(&[0, 0])), poly16(89, &[50, 0, 50, 50]), rec(60, &[]), rec(64, &i32s(&[0, 0, 0, 0]))])).unwrap();
    let p = paths(&d);
    assert_eq!(p.len(), 1);
    assert!(!p[0].1 && p[0].2.is_none() && p[0].3.is_some());
    assert_eq!(p[0].0[0], vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0]]);
}

#[test]
fn restoredc_brings_back_the_saved_brush_and_transform() {
    let d = parse_emf(&emf(&[
        select(NULL_PEN),
        brush(1, 0, 0x0000_00FF),
        select(1),
        rec(33, &[]),
        select(NULL_BRUSH),
        rec(35, &[2.0f32, 0.0, 0.0, 2.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()),
        rec(34, &i32s(&[-1])),
        rec(43, &i32s(&[0, 0, 50, 25])),
    ]))
    .unwrap();
    let p = paths(&d);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].2, Some([255, 0, 0]), "red brush restored");
    assert!(near(bbox(&p[0].0), [0.0, 0.0, 0.5, 0.5]), "identity world transform restored");
}

#[test]
fn world_transform_scales_pen_widths_and_points() {
    let xf: Vec<u8> = [2.0f32, 0.0, 0.0, 2.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let pen = rec(38, &[u32s(&[1, 0]), i32s(&[5, 0]), u32s(&[0x00FF_0000])].concat());
    let d = parse_emf(&emf(&[rec(35, &xf), pen, select(1), select(NULL_BRUSH), rec(43, &i32s(&[0, 0, 25, 10]))])).unwrap();
    let p = paths(&d);
    assert!(near(bbox(&p[0].0), [0.0, 0.0, 0.5, 0.4]));
    let (c, w) = p[0].3.unwrap();
    assert_eq!(c, [0, 0, 255]);
    assert!((w - 0.1).abs() < 1e-6, "5 logical units x 2 = 10 of 100 device pixels: {w}");
}

#[test]
fn eight_bpp_palette_dib_decodes_bottom_up() {
    // A 2 x 2 8-bpp DIB with a 2-entry palette (red, blue): bottom row [1, 0], top row [0, 1].
    let bmi = [i32s(&[40, 2, 2]), i16s(&[1, 8]), u32s(&[0, 0, 0, 0, 2, 0]), vec![0, 0, 255, 0, 255, 0, 0, 0]].concat();
    let bits = vec![1, 0, 0, 0, 0, 1, 0, 0];
    let fixed = 80u32;
    let body = [
        i32s(&[0, 0, 0, 0, 10, 5, 0, 0, 2, 2]),
        u32s(&[fixed, bmi.len() as u32, fixed + bmi.len() as u32, bits.len() as u32, 0, 0x00CC_0020]),
        i32s(&[50, 25]),
        bmi,
        bits,
    ]
    .concat();
    let d = parse_emf(&emf(&[rec(81, &body)])).unwrap();
    let Some(Item::Image { rect, rgba, size }) = d.items.first() else { panic!("{:?}", d.items) };
    assert_eq!(*size, [2, 2]);
    assert_eq!(rgba, &[255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255]);
    assert!(near(*rect, [0.1, 0.1, 0.5, 0.5]), "{rect:?}");
}

#[test]
fn bitblt_without_a_bitmap_fills_with_the_brush() {
    let blt = |rop: u32| {
        let mut b = [i32s(&[0, 0, 0, 0, 10, 10, 20, 10]), u32s(&[rop]), i32s(&[0, 0])].concat();
        b.extend([1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()));
        b.extend(u32s(&[0, 0, 0, 0, 0, 0]));
        rec(76, &b)
    };
    let d = parse_emf(&emf(&[brush(1, 0, 0x00FF_0000), select(1), blt(0x00F0_0021), blt(0x0000_0042), blt(0x005A_0049)])).unwrap();
    let p = paths(&d);
    assert_eq!(p.len(), 2, "PATCOPY and BLACKNESS draw, PATINVERT does not");
    assert_eq!(p[0].2, Some([0, 0, 255]));
    assert!(p[0].3.is_none());
    assert!(near(bbox(&p[0].0), [0.1, 0.2, 0.3, 0.4]));
    assert_eq!(p[1].2, Some([0, 0, 0]));
}

#[test]
fn hostile_counts_and_sizes_do_not_panic() {
    let d = parse_emf(&emf(&[
        rec(86, &[i32s(&[0, 0, 0, 0]), u32s(&[u32::MAX])].concat()),
        rec(91, &[i32s(&[0, 0, 0, 0]), u32s(&[u32::MAX, 3, 3])].concat()),
        rec(34, &i32s(&[i32::MIN])),
        rec(81, &[i32s(&[0; 10]), u32s(&[u32::MAX, u32::MAX, u32::MAX, u32::MAX, 0, 0x00CC_0020])].concat()),
        rec(36, &[0u8; 28]),
    ]))
    .unwrap();
    assert!(d.items.is_empty());
    // A record whose size runs past the end stops the picture without failing it.
    let mut bytes = emf(&[select(NULL_PEN), rec(43, &i32s(&[0, 0, 10, 10])), rec(43, &i32s(&[0, 0, 10, 10]))]);
    let n = bytes.len();
    // The second rectangle's size field (it sits before the 20-byte EOF record).
    bytes[n - 40..n - 36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(parse_emf(&bytes).unwrap().items.len(), 1);
    // A zero-sized frame is unusable.
    let mut zero = emf(&[]);
    zero[32..40].copy_from_slice(&i32s(&[0, 0]));
    assert!(parse_emf(&zero).is_none());
    for cut in 0..zero.len() {
        let _ = parse_emf(&zero[..cut]);
    }
}
