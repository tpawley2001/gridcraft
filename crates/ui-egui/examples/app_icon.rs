//! Writes every packaged app icon from the code-drawn artwork (`brand::icon`).
//!
//! `cargo run -p gridcraft-ui-egui --example app_icon -- assets/app-icon`
//!
//! Outputs (all committed, so packaging needs no tools): the 1024 px tile, the macOS 512 px Dock
//! icon (Apple's grid: an 824/1024 body with a transparent margin), the Linux hicolor PNGs, a
//! Windows `.ico` and a macOS `.icns` (both PNG-payload containers, packed here).

use gridcraft_ui_egui::brand::{APP_ID, icon};
use std::path::Path;

fn png(size: u32, rgba: Vec<u8>) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(size, size, rgba).expect("buffer size");
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).expect("encode png");
    out.into_inner()
}

/// The full tile at `size` px.
fn tile(size: u32) -> Vec<u8> {
    png(size, icon::rgba(size))
}

/// The tile on Apple's icon grid: the body is 824/1024 of the canvas, centred.
fn mac(size: u32) -> Vec<u8> {
    let body = (size as f32 * 824.0 / 1024.0).round() as u32;
    let off = (size - body) / 2;
    let src = icon::rgba(body);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..body {
        for x in 0..body {
            let s = ((y * body + x) * 4) as usize;
            let d = (((y + off) * size + x + off) * 4) as usize;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    png(size, out)
}

fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes.iter().map(|&s| (s, tile(s))).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend((images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (s, data) in &images {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        out.extend([dim, dim, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in images {
        out.extend(data);
    }
    out
}

fn icns() -> Vec<u8> {
    let mut body = Vec::new();
    for (kind, size) in [(b"icp4", 16), (b"icp5", 32), (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)] {
        let data = mac(size);
        body.extend(kind);
        body.extend(((data.len() + 8) as u32).to_be_bytes());
        body.extend(data);
    }
    let mut out = b"icns".to_vec();
    out.extend(((body.len() + 8) as u32).to_be_bytes());
    out.extend(body);
    out
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "assets/app-icon".into());
    let dir = Path::new(&dir);
    let write = |rel: &str, data: Vec<u8>| {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("create dir");
        }
        std::fs::write(&p, data).expect("write icon");
        eprintln!("wrote {}", p.display());
    };
    write("plunder-ledger-1024.png", tile(1024));
    write("plunder-ledger-macos-512.png", mac(512));
    for s in [16, 24, 32, 48, 64, 128, 256, 512] {
        write(&format!("hicolor/{s}x{s}/apps/{APP_ID}.png"), tile(s));
    }
    write("plunder-ledger.ico", ico(&[16, 20, 24, 32, 40, 48, 64, 128, 256]));
    write("plunder-ledger.icns", icns());
}
