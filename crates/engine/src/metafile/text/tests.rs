//! Frame (0, 0)-(1000, 500) in 0.01 mm on a 1000 x 1000 pixel, 100 x 100 mm device: the frame is
//! 100 x 50 device pixels, so relative x = px / 100 and relative y = px / 50.

use super::super::{Item, parse_emf};

fn i32s(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn rec(ty: u32, body: &[u8]) -> Vec<u8> {
    [&ty.to_le_bytes()[..], &((body.len() + 8) as u32).to_le_bytes(), body].concat()
}

fn emf(records: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = records.concat();
    let mut head = i32s(&[0, 0, 99, 49, 0, 0, 1000, 500]);
    head.extend(0x464D_4520u32.to_le_bytes());
    head.extend(i32s(&[0x0001_0000, 0, 0, 4, 0, 0, 0, 1000, 1000, 100, 100]));
    [rec(1, &head), body, rec(14, &i32s(&[0, 16, 20]))].concat()
}

fn font(height: i32, face: &str) -> Vec<u8> {
    let mut b = 1u32.to_le_bytes().to_vec();
    b.extend(i32s(&[height, 0, 0, 0, 400]));
    b.extend([0u8; 8]);
    let mut name = [0u16; 32];
    for (i, u) in face.encode_utf16().take(31).enumerate() {
        name[i] = u;
    }
    b.extend(name.iter().flat_map(|u| u.to_le_bytes()));
    [rec(82, &b), rec(37, &1u32.to_le_bytes())].concat()
}

/// EXTTEXTOUTA (`ty` 83, `bytes` 8-bit) or EXTTEXTOUTW (`ty` 84, `bytes` UTF-16LE) of `n` characters.
fn text_out(ty: u32, x: i32, y: i32, bytes: &[u8], n: usize, dx: Option<&[i32]>) -> Vec<u8> {
    let pad = (4 - bytes.len() % 4) % 4;
    let off_dx = if dx.is_some() { 76 + (bytes.len() + pad) as u32 } else { 0 };
    let mut b = i32s(&[0, 0, 99, 49, 1]);
    b.extend(1.0f32.to_le_bytes());
    b.extend(1.0f32.to_le_bytes());
    b.extend(i32s(&[x, y]));
    b.extend((n as u32).to_le_bytes());
    b.extend(76u32.to_le_bytes());
    b.extend(i32s(&[0, 0, 0, 0, 0]));
    b.extend(off_dx.to_le_bytes());
    b.extend(bytes);
    b.extend(vec![0u8; pad]);
    if let Some(dx) = dx {
        b.extend(i32s(dx));
    }
    rec(ty, &b)
}

fn wide(s: &str) -> (Vec<u8>, usize) {
    let units: Vec<u16> = s.encode_utf16().collect();
    (units.iter().flat_map(|u| u.to_le_bytes()).collect(), units.len())
}

fn u32rec(ty: u32, v: u32) -> Vec<u8> {
    rec(ty, &v.to_le_bytes())
}

/// (origin, size, text) of every text run.
fn runs(records: &[Vec<u8>]) -> Vec<([f32; 2], f32, String)> {
    let d = parse_emf(&emf(records)).unwrap_or_default();
    d.items
        .iter()
        .filter_map(|i| match i {
            Item::Text { origin, size, text, .. } => Some((*origin, *size, text.clone())),
            _ => None,
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.005
}

#[test]
fn ansi_text_is_windows_1252() {
    let r = runs(&[font(-20, "Arial"), u32rec(22, 24), text_out(83, 10, 30, b"Caf\xE9 \x80\x01", 7, None)]);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].2, "Caf\u{e9} \u{20ac}", "control characters dropped");
    assert!(close(r[0].0[0], 0.1) && close(r[0].0[1], 0.6), "{r:?}");
}

#[test]
fn update_cp_draws_at_and_advances_the_current_position() {
    let (a, na) = wide("ab");
    let r = runs(&[
        font(-20, "Arial"),
        u32rec(18, 1),
        rec(27, &i32s(&[10, 30])),
        u32rec(22, 24 | 1),
        text_out(84, 0, 0, &a, na, Some(&[10, 10])),
        text_out(84, 0, 0, &a, na, Some(&[10, 10])),
    ]);
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(close(r[0].0[0], 0.1) && close(r[0].0[1], 0.6), "{r:?}");
    assert!(close(r[1].0[0], 0.3) && close(r[1].0[1], 0.6), "second run starts after the first's 20 units: {r:?}");
}

#[test]
fn anisotropic_mapping_scales_the_size_and_position() {
    // Window 200 x 200 onto viewport 100 x -100 (y flipped, half scale), origin moved to y = 50.
    let (a, na) = wide("x");
    let r = runs(&[
        u32rec(17, 8),
        rec(9, &i32s(&[200, 200])),
        rec(11, &i32s(&[100, -100])),
        rec(12, &i32s(&[0, 50])),
        font(-40, "Arial"),
        u32rec(22, 24),
        text_out(84, 20, 40, &a, na, None),
    ]);
    assert_eq!(r.len(), 1);
    assert!(close(r[0].1, 0.4), "em 40 logical = 20 px of the 50 px frame: {r:?}");
    assert!(close(r[0].0[0], 0.1) && close(r[0].0[1], 0.6), "(20, 40) -> (10, 30) px: {r:?}");
}

#[test]
fn opaque_background_mode_fills_the_text_cell_and_transparent_does_not() {
    let (a, na) = wide("ab");
    let fills = |mode: u32| {
        let d = parse_emf(&emf(&[font(-20, "Arial"), u32rec(18, mode), u32rec(25, 0x00FF_0000), text_out(84, 10, 10, &a, na, Some(&[10, 10]))]))
            .unwrap_or_default();
        d.items.iter().filter(|i| matches!(i, Item::Path { fill: Some([0, 0, 255]), .. })).count()
    };
    assert_eq!(fills(2), 1);
    assert_eq!(fills(1), 0);
}

#[test]
fn truncated_text_records_do_not_panic() {
    let (a, na) = wide("abc");
    let full = text_out(84, 10, 30, &a, na, Some(&[1, 2, 3]));
    for cut in 8..full.len() {
        let mut r = full.get(..cut).unwrap_or_default().to_vec();
        r[4..8].copy_from_slice(&(cut as u32).to_le_bytes());
        let _ = runs(&[font(-20, "Arial"), r]);
    }
    let _ = runs(&[rec(82, &[1, 0, 0, 0]), rec(37, &1u32.to_le_bytes()), full]);
}

#[test]
fn face_name_follows_the_eight_logfont_byte_fields() {
    let d = parse_emf(&emf(&[font(-20, "Courier New"), u32rec(18, 1), text_out(83, 10, 30, b"a", 1, None)])).unwrap_or_default();
    let faces: Vec<&str> = d.items.iter().filter_map(|i| if let Item::Text { face, .. } = i { Some(face.as_str()) } else { None }).collect();
    assert_eq!(faces, ["Courier New"]);
}
