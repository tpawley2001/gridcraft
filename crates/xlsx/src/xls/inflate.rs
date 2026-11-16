//! A small, bounded zlib (RFC 1950) / DEFLATE (RFC 1951) decoder for the compressed metafile
//! pictures of `.xls` files: stored, fixed-Huffman and dynamic-Huffman blocks. The output never
//! grows past the size the caller expects, and any malformed input gives `None`. The Adler-32
//! trailer is not checked.

/// Base lengths and extra bits of the length codes 257..=285 (RFC 1951 3.2.5).
const LENGTH_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
/// Base distances and extra bits of the distance codes 0..=29.
const DIST_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// The order in which a dynamic block lists the code length code lengths.
const CLEN_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
const MAX_BITS: usize = 15;

/// Inflates a zlib stream to exactly `expected` bytes; `None` when the header or a block is bad,
/// or the data inflates to any other length.
pub(super) fn zlib(data: &[u8], expected: usize) -> Option<Vec<u8>> {
    let (cmf, flg) = (*data.first()?, *data.get(1)?);
    // CM 8 (deflate), a window of at most 32 KB, the header check, no preset dictionary.
    if cmf & 0x0F != 8 || cmf >> 4 > 7 || (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 || flg & 0x20 != 0 {
        return None;
    }
    let out = inflate(data.get(2..)?, expected)?;
    (out.len() == expected).then_some(out)
}

/// Inflates raw DEFLATE data, giving up as soon as the output would pass `limit` bytes.
fn inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut bits = Bits { data, pos: 0, buf: 0, cnt: 0 };
    let mut out = Vec::with_capacity(limit.min(data.len().saturating_mul(4)));
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => stored(&mut bits, &mut out, limit)?,
            1 => {
                let (lit, dist) = fixed()?;
                codes(&mut bits, &mut out, limit, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = dynamic(&mut bits)?;
                codes(&mut bits, &mut out, limit, &lit, &dist)?;
            }
            _ => return None,
        }
        if last {
            return Some(out);
        }
    }
}

/// An LSB-first bit reader.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    cnt: u32,
}

impl Bits<'_> {
    fn take(&mut self, n: u32) -> Option<u32> {
        while self.cnt < n {
            let byte = *self.data.get(self.pos)?;
            self.pos += 1;
            self.buf |= u64::from(byte) << self.cnt;
            self.cnt += 8;
        }
        let v = u32::try_from(self.buf & ((1u64 << n) - 1)).ok()?;
        self.buf >>= n;
        self.cnt -= n;
        Some(v)
    }

    /// Drops the bits left in the current byte and gives back the whole buffered bytes.
    fn align(&mut self) {
        self.pos = self.pos.saturating_sub(usize::try_from(self.cnt / 8).unwrap_or(0));
        self.buf = 0;
        self.cnt = 0;
    }
}

fn stored(bits: &mut Bits<'_>, out: &mut Vec<u8>, limit: usize) -> Option<()> {
    bits.align();
    let header = bits.data.get(bits.pos..bits.pos.checked_add(4)?)?;
    let len = u16::from_le_bytes([*header.first()?, *header.get(1)?]);
    let nlen = u16::from_le_bytes([*header.get(2)?, *header.get(3)?]);
    if len != !nlen {
        return None;
    }
    let start = bits.pos.checked_add(4)?;
    let end = start.checked_add(usize::from(len))?;
    let block = bits.data.get(start..end)?;
    if out.len().checked_add(block.len())? > limit {
        return None;
    }
    out.extend_from_slice(block);
    bits.pos = end;
    Some(())
}

/// A canonical Huffman code: the number of codes of each length and the symbols in code order.
struct Huffman {
    counts: [u16; MAX_BITS + 1],
    symbols: Vec<u16>,
}

impl Huffman {
    /// `None` when the lengths over-subscribe the code space.
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0u16; MAX_BITS + 1];
        for &l in lengths {
            *counts.get_mut(usize::from(l))? += 1;
        }
        let mut left = 1i32;
        for &c in counts.iter().skip(1) {
            left = left * 2 - i32::from(c);
            if left < 0 {
                return None;
            }
        }
        let mut offsets = [0u16; MAX_BITS + 2];
        for len in 1..=MAX_BITS {
            let next = offsets.get(len)?.checked_add(*counts.get(len)?)?;
            *offsets.get_mut(len + 1)? = next;
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                let slot = offsets.get_mut(usize::from(l))?;
                *symbols.get_mut(usize::from(*slot))? = u16::try_from(sym).ok()?;
                *slot += 1;
            }
        }
        Some(Huffman { counts, symbols })
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for &count in self.counts.iter().skip(1) {
            code |= i32::try_from(bits.take(1)?).ok()?;
            let count = i32::from(count);
            if code - first < count {
                return self.symbols.get(usize::try_from(index + code - first).ok()?).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

fn fixed() -> Option<(Huffman, Huffman)> {
    let mut lengths = [0u8; 288];
    for (i, l) in lengths.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Some((Huffman::new(&lengths)?, Huffman::new(&[5u8; 30])?))
}

fn dynamic(bits: &mut Bits<'_>) -> Option<(Huffman, Huffman)> {
    let nlen = usize::try_from(bits.take(5)?).ok()? + 257;
    let ndist = usize::try_from(bits.take(5)?).ok()? + 1;
    let ncode = usize::try_from(bits.take(4)?).ok()? + 4;
    if nlen > 286 || ndist > 30 {
        return None;
    }
    let mut clen = [0u8; 19];
    for &i in CLEN_ORDER.iter().take(ncode) {
        *clen.get_mut(i)? = u8::try_from(bits.take(3)?).ok()?;
    }
    let clen = Huffman::new(&clen)?;
    let mut lengths = Vec::with_capacity(nlen + ndist);
    while lengths.len() < nlen + ndist {
        let sym = clen.decode(bits)?;
        let (value, repeat) = match sym {
            0..=15 => (u8::try_from(sym).ok()?, 1),
            16 => (*lengths.last()?, 3 + bits.take(2)?),
            17 => (0, 3 + bits.take(3)?),
            18 => (0, 11 + bits.take(7)?),
            _ => return None,
        };
        for _ in 0..repeat {
            if lengths.len() >= nlen + ndist {
                return None;
            }
            lengths.push(value);
        }
    }
    // The end-of-block code must exist.
    if lengths.get(256).copied().unwrap_or(0) == 0 {
        return None;
    }
    Some((Huffman::new(lengths.get(..nlen)?)?, Huffman::new(lengths.get(nlen..)?)?))
}

fn codes(bits: &mut Bits<'_>, out: &mut Vec<u8>, limit: usize, lit: &Huffman, dist: &Huffman) -> Option<()> {
    loop {
        let sym = lit.decode(bits)?;
        match sym {
            0..=255 => {
                if out.len() >= limit {
                    return None;
                }
                out.push(u8::try_from(sym).ok()?);
            }
            256 => return Some(()),
            _ => {
                let i = usize::from(sym - 257);
                let len = usize::from(*LENGTH_BASE.get(i)?) + usize::try_from(bits.take(u32::from(*LENGTH_EXTRA.get(i)?))?).ok()?;
                let d = usize::from(dist.decode(bits)?);
                let back = usize::from(*DIST_BASE.get(d)?) + usize::try_from(bits.take(u32::from(*DIST_EXTRA.get(d)?))?).ok()?;
                if back > out.len() || out.len().checked_add(len)? > limit {
                    return None;
                }
                let from = out.len() - back;
                for k in from..from + len {
                    let byte = *out.get(k)?;
                    out.push(byte);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::zlib;

    #[test]
    fn stored_block() {
        // zlib header, final stored block of 3 bytes, Adler-32.
        let z = [0x78, 0x01, 0x01, 0x03, 0x00, 0xFC, 0xFF, b'a', b'b', b'c', 0x02, 0x4D, 0x01, 0x27];
        assert_eq!(zlib(&z, 3).as_deref(), Some(&b"abc"[..]));
        assert_eq!(zlib(&z, 2), None);
        assert_eq!(zlib(&z, 4), None);
    }

    #[test]
    fn fixed_block_with_a_back_reference() {
        // "abcabcabcabc" from zlib level 9.
        let z = [0x78, 0xDA, 0x4B, 0x4C, 0x4A, 0x4E, 0x84, 0x21, 0x00, 0x1D, 0xE0, 0x04, 0x99];
        assert_eq!(zlib(&z, 12).as_deref(), Some(&b"abcabcabcabc"[..]));
    }

    #[test]
    fn bad_input_gives_none() {
        assert_eq!(zlib(&[], 0), None);
        assert_eq!(zlib(&[0x78], 0), None);
        // Header check fails.
        assert_eq!(zlib(&[0x78, 0x00, 0x03, 0x00], 0), None);
        // Preset dictionary.
        assert_eq!(zlib(&[0x78, 0xBB, 0, 0, 0, 0], 0), None);
        // Reserved block type.
        assert_eq!(zlib(&[0x78, 0x01, 0x07], 0), None);
        // Stored block whose NLEN isn't LEN's complement.
        assert_eq!(zlib(&[0x78, 0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 1, 2, 3], 3), None);
        // Cut off.
        assert_eq!(zlib(&[0x78, 0xDA, 0x4B, 0x4C], 12), None);
        // A distance that reaches before the output: fixed block, literal 'a', then length 3
        // distance 2.
        let z = [0x78, 0xDA, 0x4B, 0x04, 0x42, 0x00];
        assert_eq!(zlib(&z, 4), None);
    }

    #[test]
    fn never_grows_past_the_limit() {
        // 1,000 zero bytes in a fixed block (zlib level 9) asked for as 100 bytes.
        let z = [0x78, 0xDA, 0x63, 0x60, 0x18, 0x05, 0xA3, 0x60, 0x14, 0x0C, 0x77, 0x00, 0x00, 0x03, 0xE8, 0x00, 0x01];
        assert_eq!(zlib(&z, 100), None);
        assert_eq!(zlib(&z, 1000).map(|v| v.len()), Some(1000));
    }

    #[test]
    fn hostile_bytes_never_panic() {
        let mut seed = 0x1234_5678u32;
        for n in 0..2000usize {
            let mut v = vec![0x78, 0x9C];
            for _ in 0..(n % 64) {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                v.push((seed >> 16) as u8);
            }
            let _ = zlib(&v, n % 300);
        }
    }
}
