//! The app icon, drawn in code: a treasure chest whose front is a ledger grid, on a deep-sea
//! tile. Original geometry written for this fork; no image file is its source.
//!
//! [`rgba`] rasterises it at any size (4×4 supersampled), for the window icon, the packaging
//! icons (`cargo run -p gridcraft-ui-egui --example app_icon`) and the in-app picture
//! ([`texture`]).

/// Straight-alpha colour, 0–1.
type Rgba = [f32; 4];

const fn hex(c: u32) -> Rgba {
    [((c >> 16) & 0xFF) as f32 / 255.0, ((c >> 8) & 0xFF) as f32 / 255.0, (c & 0xFF) as f32 / 255.0, 1.0]
}

/// The spec palette.
const NAVY: Rgba = hex(0x1F3A4D);
const NAVY_DEEP: Rgba = hex(0x16262F);
const WAVE: Rgba = hex(0x2C5068);
const INK: Rgba = hex(0x2B1D0E);
const WOOD: Rgba = hex(0x8C6D46);
const WOOD_LIGHT: Rgba = hex(0xA5834F);
const WOOD_DARK: Rgba = hex(0x6E5233);
const BRASS: Rgba = hex(0xB8860B);
const GOLD: Rgba = hex(0xD4A017);
const PARCHMENT: Rgba = hex(0xF4E9D0);

/// Inside a rounded rectangle centred at (cx, cy) with half sizes (hw, hh) and corner radius r.
fn in_round_rect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> bool {
    let dx = ((x - cx).abs() - hw + r).max(0.0);
    let dy = ((y - cy).abs() - hh + r).max(0.0);
    (x - cx).abs() <= hw && (y - cy).abs() <= hh && dx * dx + dy * dy <= r * r
}

fn in_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    x >= x0 && x <= x1 && y >= y0 && y <= y1
}

fn in_ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let (u, v) = ((x - cx) / rx, (y - cy) / ry);
    u * u + v * v <= 1.0
}

/// The chest lid: the top half of an ellipse over the body, grown by `grow` for the outline.
fn in_lid(x: f32, y: f32, grow: f32) -> bool {
    y <= LID_BASE + grow && in_ellipse(x, y, 0.5, LID_BASE, 0.34 + grow, 0.22 + grow)
}

const LID_BASE: f32 = 0.47;
const BODY: (f32, f32, f32, f32) = (0.16, 0.47, 0.84, 0.84);
const LINE: f32 = 0.022;

/// The colour of the icon at (x, y) in the unit square (y down).
fn sample(x: f32, y: f32) -> Rgba {
    const CLEAR: Rgba = [0.0; 4];
    if !in_round_rect(x, y, 0.5, 0.5, 0.5, 0.5, 0.2) {
        return CLEAR;
    }
    // Sea tile with a lighter wave band near the bottom.
    let mut c = if y > 0.78 { NAVY_DEEP } else { NAVY };
    let wave = 0.80 + 0.018 * (x * 18.0).sin();
    if (y - wave).abs() < 0.012 {
        c = WAVE;
    }
    let (bx0, by0, bx1, by1) = BODY;
    // Outlines first: lid and body grown by the line width.
    if in_lid(x, y, LINE) || in_rect(x, y, bx0 - LINE, by0 - LINE, bx1 + LINE, by1 + LINE) {
        c = INK;
    }
    if in_lid(x, y, 0.0) {
        // Lid planks: a darker line every few hundredths.
        c = if ((y - 0.25) / 0.075).fract().abs() < 0.12 { WOOD_DARK } else { WOOD_LIGHT };
    }
    if in_rect(x, y, bx0, by0, bx1, by1) {
        c = WOOD;
    }
    // Brass straps over lid and body, and the rim band where they meet.
    let strap = |sx: f32| (x - sx).abs() < 0.032 && (in_lid(x, y, 0.0) || in_rect(x, y, bx0, by0, bx1, by1));
    if strap(0.27) || strap(0.73) {
        c = BRASS;
    }
    if in_rect(x, y, bx0, by0 - 0.02, bx1, by0 + 0.035) {
        c = if (y - by0).abs() < 0.006 { INK } else { GOLD };
    }
    // The ledger on the chest's front: a parchment sheet ruled into a grid with a gold header row.
    let (gx0, gy0, gx1, gy1) = (0.34, 0.565, 0.66, 0.79);
    if in_rect(x, y, gx0 - 0.012, gy0 - 0.012, gx1 + 0.012, gy1 + 0.012) {
        c = INK;
    }
    if in_rect(x, y, gx0, gy0, gx1, gy1) {
        c = if y < gy0 + 0.055 { GOLD } else { PARCHMENT };
        let cols = (gx1 - gx0) / 3.0;
        let rows = (gy1 - gy0) / 4.0;
        let on_col = ((x - gx0) / cols).fract() < 0.07 && x > gx0 + 0.01 && x < gx1 - 0.01;
        let on_row = ((y - gy0) / rows).fract() < 0.07 && y > gy0 + 0.01 && y < gy1 - 0.01;
        if on_col || on_row {
            c = INK;
        }
    }
    // The lock plate on the rim, with its keyhole.
    if in_round_rect(x, y, 0.5, by0 + 0.005, 0.06, 0.07, 0.02) {
        c = INK;
    }
    if in_round_rect(x, y, 0.5, by0 + 0.005, 0.045, 0.055, 0.015) {
        c = GOLD;
        if in_ellipse(x, y, 0.5, by0 - 0.01, 0.014, 0.014) || in_rect(x, y, 0.494, by0 - 0.01, 0.506, by0 + 0.03) {
            c = INK;
        }
    }
    c
}

/// The icon as `size`×`size` straight-alpha RGBA bytes (row-major). `size` is clamped to 1–2048.
pub fn rgba(size: u32) -> Vec<u8> {
    let size = size.clamp(1, 2048);
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size as usize) * (size as usize) * 4);
    let step = 1.0 / (size * SS) as f32;
    for py in 0..size {
        for px in 0..size {
            // Premultiplied accumulation, so transparent samples don't darken the edges.
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = ((px * SS + sx) as f32 + 0.5) * step;
                    let y = ((py * SS + sy) as f32 + 0.5) * step;
                    let [r, g, b, a] = sample(x, y);
                    acc[0] += r * a;
                    acc[1] += g * a;
                    acc[2] += b * a;
                    acc[3] += a;
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            let un = |v: f32| if a > 0.0 { (v / n / a).clamp(0.0, 1.0) } else { 0.0 };
            out.extend([un(acc[0]), un(acc[1]), un(acc[2]), a.clamp(0.0, 1.0)].map(|v| (v * 255.0).round() as u8));
        }
    }
    out
}

/// The icon as an egui texture, made once per context and kept in its memory.
pub fn texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::unique("plunder_ledger_app_icon");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    const SIZE: u32 = 128;
    let image = egui::ColorImage::from_rgba_unmultiplied([SIZE as usize, SIZE as usize], &rgba(SIZE));
    let t = ctx.load_texture("plunder_ledger_app_icon", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Paints the icon into `rect`.
pub fn paint(ui: &egui::Ui, rect: egui::Rect) {
    let t = texture(ui.ctx());
    ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_a_transparent_corner_and_an_opaque_middle() {
        let px = rgba(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        assert_eq!(px.get(3), Some(&0), "top-left corner is outside the rounded tile");
        let mid = (32 * 64 + 32) * 4;
        assert_eq!(px.get(mid + 3), Some(&255));
    }

    #[test]
    fn icon_size_is_clamped() {
        assert_eq!(rgba(0).len(), 4);
        assert_eq!(rgba(3).len(), 3 * 3 * 4);
    }
}
