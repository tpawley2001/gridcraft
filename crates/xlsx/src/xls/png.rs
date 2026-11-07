//! Device-independent bitmaps from `.xls` IMDATA records ([MS-XLS] 2.4.140) as PNG files: the
//! 24-bit BITMAPCOREINFO that Excel 5.0/95 writes (and a 24-bit BITMAPINFOHEADER), encoded as an
//! 8-bit RGB PNG with stored (uncompressed) deflate blocks.

/// Pixels a bitmap may have at most (64 Mi).
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
/// Bytes a stored deflate block holds at most.
const MAX_STORED: usize = 65_535;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, *s.get(3)?]))
}

/// A 24-bit device-independent bitmap (a 12-byte BITMAPCOREHEADER or a 40-byte
/// BITMAPINFOHEADER, then the pixel rows) as a PNG. `None` for any other header, bit count or
/// compression, a zero or oversized size, or rows running past the data.
pub(super) fn bitmap_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    let (width, height, bottom_up, pixels) = match u32_at(dib, 0)? {
        12 => {
            if u16_at(dib, 8)? != 1 || u16_at(dib, 10)? != 24 {
                return None;
            }
            (u32::from(u16_at(dib, 4)?), u32::from(u16_at(dib, 6)?), true, 12usize)
        }
        40 => {
            let w = i32::try_from(u32_at(dib, 4)?).ok()?;
            let h = u32_at(dib, 8)? as i32;
            // Planes 1, 24 bits per pixel, BI_RGB (no compression).
            if u16_at(dib, 12)? != 1 || u16_at(dib, 14)? != 24 || u32_at(dib, 16)? != 0 {
                return None;
            }
            // A colour table (biClrUsed entries of 4 bytes) may come before the pixels.
            let colors = usize::try_from(u32_at(dib, 32)?).ok()?;
            let pixels = colors.checked_mul(4)?.checked_add(40)?;
            (u32::try_from(w).ok()?, h.unsigned_abs(), h > 0, pixels)
        }
        _ => return None,
    };
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return None;
    }
    let (w, h) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    let row_bytes = w.checked_mul(3)?;
    let stride = row_bytes.checked_add(3)? / 4 * 4;
    let end = stride.checked_mul(h)?.checked_add(pixels)?;
    let rows = dib.get(pixels..end)?;
    // PNG scanlines: a filter byte (0, none) then R, G, B per pixel, top row first.
    let mut raw = Vec::with_capacity(row_bytes.checked_add(1)?.checked_mul(h)?);
    for y in 0..h {
        let src = if bottom_up { h - 1 - y } else { y };
        let start = src.checked_mul(stride)?;
        let row = rows.get(start..start.checked_add(row_bytes)?)?;
        raw.push(0);
        for px in row.chunks_exact(3) {
            if let [b, g, r] = px {
                raw.extend_from_slice(&[*r, *g, *b]);
            }
        }
    }
    Some(encode_rgb(width, height, &raw))
}

/// An 8-bit RGB PNG (colour type 2) from its filtered scanlines.
fn encode_rgb(width: u32, height: u32, scanlines: &[u8]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    // Bit depth 8, colour type 2 (RGB), deflate, adaptive filtering, no interlace.
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(scanlines));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A zlib stream (RFC 1950) of stored deflate blocks (RFC 1951 3.2.4).
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let blocks = data.len() / MAX_STORED + 1;
    let mut out = Vec::with_capacity(data.len() + blocks * 5 + 6);
    // CMF: deflate with a 32 KiB window; FLG: no dictionary, check bits making CMF*256+FLG a
    // multiple of 31.
    out.extend_from_slice(&[0x78, 0x01]);
    let mut pieces = data.chunks(MAX_STORED).peekable();
    if pieces.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(piece) = pieces.next() {
        out.push(u8::from(pieces.peek().is_none()));
        let len = u16::try_from(piece.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(piece);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

/// Appends a PNG chunk: length, type, data and the CRC-32 of the type and data.
fn chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let crc = crc32(out.get(start..).unwrap_or_default());
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let table = crc_table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c = table.get(((c ^ u32::from(b)) & 0xFF) as usize).copied().unwrap_or(0) ^ (c >> 8);
    }
    !c
}

fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (n, slot) in (0u32..).zip(table.iter_mut()) {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_and_adler_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(&[]), 1);
    }

    #[test]
    fn stored_blocks_split_at_65535_bytes() {
        let data = vec![7u8; 70_000];
        let z = zlib_stored(&data);
        assert_eq!(z.len(), 2 + 5 + 65_535 + 5 + (70_000 - 65_535) + 4);
        assert_eq!(z[2], 0, "first block is not final");
        assert_eq!(&z[3..5], &65_535u16.to_le_bytes());
        assert_eq!(z[2 + 5 + 65_535], 1, "last block is final");
        assert_eq!(zlib_stored(&[]), [0x78, 0x01, 1, 0, 0, 0xFF, 0xFF, 0, 0, 0, 1]);
    }

    #[test]
    fn info_header_top_down_bitmap_converts() {
        // 1 x 2, top-down (negative height): red then blue.
        let mut dib = 40u32.to_le_bytes().to_vec();
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&(-2i32).to_le_bytes());
        dib.extend_from_slice(&[1, 0, 24, 0]);
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        let png = bitmap_to_png(&dib).unwrap();
        // IDAT data starts after signature (8), IHDR (25) and the IDAT length and type (8);
        // then the zlib header (2) and the stored block header (5).
        let at = 8 + 25 + 8 + 2 + 5;
        assert_eq!(&png[at..at + 8], &[0, 255, 0, 0, 0, 0, 0, 255]);
    }

    #[test]
    fn other_bitmaps_are_refused() {
        let mut core = 12u32.to_le_bytes().to_vec();
        core.extend_from_slice(&[1, 0, 1, 0, 1, 0, 32, 0, 0, 0, 0, 0]);
        assert!(bitmap_to_png(&core).is_none(), "32 bits per pixel");
        assert!(bitmap_to_png(&[]).is_none());
        assert!(bitmap_to_png(&64u32.to_le_bytes()).is_none());
    }
}
