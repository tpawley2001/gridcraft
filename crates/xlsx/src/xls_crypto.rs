//! Decryption of legacy `.xls` workbooks (FilePass: XOR obfuscation, RC4, RC4 CryptoAPI).
//!
//! Implemented from [MS-XLS] 2.2.10 (which bytes of the stream are encrypted) and 2.4.117
//! (FilePass), and [MS-OFFCRYPTO] 2.3.5 (RC4 CryptoAPI), 2.3.6 (RC4, MD5) and 2.3.7 (XOR
//! obfuscation, method 1). Record headers and the records that must stay readable before the
//! password is known are left as they are, so the decrypted stream has the same length and
//! record offsets (BoundSheet8.lbPlyPos) still point at the sheets.

use sha1::{Digest, Sha1};

/// Why a FilePass could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XlsCryptoError {
    /// The FilePass is well formed but the password does not match its verifier.
    WrongPassword,
    /// The FilePass is truncated, names an unknown scheme or has parameters out of range.
    Unsupported,
}

/// Bytes of the stream each RC4 key covers ([MS-XLS] 2.2.10).
const BLOCK: usize = 1024;

/// Verifies `password` against the FilePass payload `filepass` and returns the decrypted
/// `stream` (the whole Workbook or Book stream, same length, record headers untouched).
/// `biff5` selects the 4-byte BIFF5 FilePass (XOR only).
pub(crate) fn decrypt(filepass: &[u8], biff5: bool, password: &str, stream: &[u8]) -> Result<Vec<u8>, XlsCryptoError> {
    let mut r = Reader(filepass);
    if biff5 {
        return xor(&mut r, password, stream);
    }
    match r.u16()? {
        0 => xor(&mut r, password, stream),
        1 => match (r.u16()?, r.u16()?) {
            (1, 1) => rc4_md5(&mut r, password, stream),
            (2..=4, 2) => rc4_cryptoapi(&mut r, password, stream),
            _ => Err(XlsCryptoError::Unsupported),
        },
        _ => Err(XlsCryptoError::Unsupported),
    }
}

/// A bounds-checked little-endian reader over the FilePass payload.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], XlsCryptoError> {
        let head = self.0.get(..n).ok_or(XlsCryptoError::Unsupported)?;
        self.0 = self.0.get(n..).unwrap_or_default();
        Ok(head)
    }

    fn u16(&mut self) -> Result<u16, XlsCryptoError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]))
    }

    fn u32(&mut self) -> Result<u32, XlsCryptoError> {
        let b = self.take(4)?;
        let mut a = [0u8; 4];
        a.copy_from_slice(b);
        Ok(u32::from_le_bytes(a))
    }

    /// The remaining bytes must be exactly used up.
    fn end(&self) -> Result<(), XlsCryptoError> {
        if self.0.is_empty() { Ok(()) } else { Err(XlsCryptoError::Unsupported) }
    }
}

/// For each stream byte, whether it is encrypted ([MS-XLS] 2.2.10): never the record headers;
/// never the bodies of BOF, FilePass, UsrExcl, FileLock, InterfaceHdr, RRDInfo and RRDHead;
/// not the first 4 bytes (lbPlyPos) of BoundSheet8. Also yields each encrypted byte's record
/// body size, which the XOR method needs.
fn encrypted_ranges(stream: &[u8]) -> Vec<(std::ops::Range<usize>, usize)> {
    let mut out = vec![];
    let mut at = 0usize;
    while let Some(head) = stream.get(at..at.saturating_add(4)) {
        let id = u16::from_le_bytes([head.first().copied().unwrap_or(0), head.get(1).copied().unwrap_or(0)]);
        let len = usize::from(u16::from_le_bytes([head.get(2).copied().unwrap_or(0), head.get(3).copied().unwrap_or(0)]));
        let start = at + 4;
        let end = start.saturating_add(len).min(stream.len());
        let plain = match id {
            0x0809 | 0x002F | 0x0194 | 0x0195 | 0x00E1 | 0x0196 | 0x0138 => len,
            0x0085 => 4,
            _ => 0,
        };
        let from = start.saturating_add(plain).min(end);
        if from < end {
            out.push((from..end, len));
        }
        at = start.saturating_add(len);
    }
    out
}

/// Decrypts the encrypted bytes with RC4, re-keyed for every 1024-byte block of the stream;
/// the keystream position counts every byte, encrypted or not.
fn rc4_stream(stream: &[u8], key_for_block: impl Fn(u32) -> Vec<u8>) -> Vec<u8> {
    let mut out = stream.to_vec();
    let mut block: Option<(usize, Rc4, usize)> = None; // (block number, cipher, position in block)
    for (range, _) in encrypted_ranges(stream) {
        for i in range {
            let n = i / BLOCK;
            let pos = i % BLOCK;
            let reuse = matches!(&block, Some((b, _, p)) if *b == n && *p <= pos);
            if !reuse {
                let key = key_for_block(u32::try_from(n).unwrap_or(u32::MAX));
                block = Some((n, Rc4::new(&key), 0));
            }
            if let Some((_, cipher, p)) = block.as_mut() {
                cipher.skip(pos - *p);
                if let Some(b) = out.get_mut(i) {
                    *b ^= cipher.next();
                }
                *p = pos + 1;
            }
        }
    }
    out
}

/// RC4 (version 1.1, [MS-OFFCRYPTO] 2.3.6): MD5 keys, a 16-byte salt and verifier.
fn rc4_md5(r: &mut Reader<'_>, password: &str, stream: &[u8]) -> Result<Vec<u8>, XlsCryptoError> {
    let salt = r.take(16)?;
    let verifier = r.take(16)?;
    let verifier_hash = r.take(16)?;
    r.end()?;
    // H0 = MD5(UTF-16LE password); the intermediate key is MD5 of (H0[..5] + salt) x 16.
    let h0 = md5(&utf16(password));
    let mut unit = h0.get(..5).unwrap_or_default().to_vec();
    unit.extend_from_slice(salt);
    let h1 = md5(&unit.repeat(16));
    let base = h1.get(..5).unwrap_or_default().to_vec();
    let key_for_block = |block: u32| {
        let mut b = base.clone();
        b.extend_from_slice(&block.to_le_bytes());
        md5(&b).to_vec()
    };
    let mut check = Rc4::new(&key_for_block(0));
    let v: Vec<u8> = verifier.iter().map(|b| b ^ check.next()).collect();
    let h: Vec<u8> = verifier_hash.iter().map(|b| b ^ check.next()).collect();
    if md5(&v).as_slice() != h.as_slice() {
        return Err(XlsCryptoError::WrongPassword);
    }
    Ok(rc4_stream(stream, key_for_block))
}

/// RC4 CryptoAPI ([MS-OFFCRYPTO] 2.3.5): SHA-1 keys of KeySize bits (40-bit keys padded with
/// zeros to 128 bits), a header naming the algorithms and an EncryptionVerifier.
fn rc4_cryptoapi(r: &mut Reader<'_>, password: &str, stream: &[u8]) -> Result<Vec<u8>, XlsCryptoError> {
    let _flags = r.u32()?;
    let header_size = usize::try_from(r.u32()?).map_err(|_| XlsCryptoError::Unsupported)?;
    let mut h = Reader(r.take(header_size)?);
    let _header_flags = h.u32()?;
    let _size_extra = h.u32()?;
    let alg = h.u32()?;
    let alg_hash = h.u32()?;
    let key_bits = match h.u32()? {
        0 => 40,
        n @ 40..=128 if n % 8 == 0 => n,
        _ => return Err(XlsCryptoError::Unsupported),
    };
    if !matches!(alg, 0 | 0x6801) || !matches!(alg_hash, 0 | 0x8004) {
        return Err(XlsCryptoError::Unsupported);
    }
    if r.u32()? != 16 {
        return Err(XlsCryptoError::Unsupported);
    }
    let salt = r.take(16)?;
    let verifier = r.take(16)?;
    if r.u32()? != 20 {
        return Err(XlsCryptoError::Unsupported);
    }
    let verifier_hash = r.take(20)?;
    r.end()?;
    let h0 = Sha1::new().chain_update(salt).chain_update(utf16(password)).finalize();
    let key_len = usize::try_from(key_bits / 8).unwrap_or(16);
    let key_for_block = |block: u32| {
        let hash = Sha1::new().chain_update(h0).chain_update(block.to_le_bytes()).finalize();
        let mut key = hash.get(..key_len).unwrap_or_default().to_vec();
        if key_bits == 40 {
            key.resize(16, 0);
        }
        key
    };
    let mut check = Rc4::new(&key_for_block(0));
    let v: Vec<u8> = verifier.iter().map(|b| b ^ check.next()).collect();
    let hv: Vec<u8> = verifier_hash.iter().map(|b| b ^ check.next()).collect();
    if Sha1::digest(&v).as_slice() != hv.as_slice() {
        return Err(XlsCryptoError::WrongPassword);
    }
    Ok(rc4_stream(stream, key_for_block))
}

/// XOR obfuscation ([MS-OFFCRYPTO] 2.3.7, method 1): the FilePass holds the 16-bit key and
/// password verifier. Each encrypted byte is XORed with the 16-byte XOR array at (its stream
/// offset + its record's size) mod 16 and rotated right by 5 bits.
fn xor(r: &mut Reader<'_>, password: &str, stream: &[u8]) -> Result<Vec<u8>, XlsCryptoError> {
    let _key = r.u16()?;
    let verifier = r.u16()?;
    r.end()?;
    let bytes = password_bytes(password);
    if bytes.is_empty() || bytes.len() > 15 || gridcraft_model::password::legacy_hash(password) != verifier {
        return Err(XlsCryptoError::WrongPassword);
    }
    let array = xor_array(&bytes).ok_or(XlsCryptoError::WrongPassword)?;
    let mut out = stream.to_vec();
    for (range, size) in encrypted_ranges(stream) {
        for i in range {
            let x = array.get((i.wrapping_add(size)) % 16).copied().unwrap_or(0);
            if let Some(b) = out.get_mut(i) {
                *b = (*b ^ x).rotate_right(5);
            }
        }
    }
    Ok(out)
}

/// The password as single bytes: each UTF-16 unit's low byte, or its high byte when the low
/// byte is zero ([MS-OFFCRYPTO] 2.3.7.4).
fn password_bytes(password: &str) -> Vec<u8> {
    password
        .encode_utf16()
        .map(|u| {
            let [lo, hi] = u.to_le_bytes();
            if lo != 0 { lo } else { hi }
        })
        .collect()
}

const INITIAL_CODE: [u16; 15] =
    [0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9, 0x280C, 0xA96A, 0x4EC3];

const XOR_MATRIX: [u16; 105] = [
    0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09, 0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF, 0x4563, 0x8AC6, 0x05AD, 0x0B5A,
    0x16B4, 0x2D68, 0x5AD0, 0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40, 0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5, 0x6F45,
    0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A, 0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9, 0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4,
    0x7B68, 0xF6D0, 0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC, 0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10, 0xAA51, 0x4483,
    0x8906, 0x022D, 0x045A, 0x08B4, 0x1168, 0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C, 0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6,
    0x1DAD, 0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC, 0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4,
];

const PAD: [u8; 15] = [0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00];

/// The 16-bit XOR key for 1 to 15 password bytes ([MS-OFFCRYPTO] 2.3.7.2).
pub(crate) fn xor_key(password: &[u8]) -> Option<u16> {
    let mut key = *INITIAL_CODE.get(password.len().checked_sub(1)?)?;
    let mut element = XOR_MATRIX.len();
    for &c in password.iter().rev() {
        let mut c = c;
        for _ in 0..7 {
            element = element.checked_sub(1)?;
            if c & 0x40 != 0 {
                key ^= XOR_MATRIX.get(element)?;
            }
            c = c.wrapping_shl(1);
        }
    }
    Some(key)
}

/// The 16-byte XOR array for 1 to 15 password bytes ([MS-OFFCRYPTO] 2.3.7.2).
fn xor_array(password: &[u8]) -> Option<[u8; 16]> {
    let key = xor_key(password)?;
    let [lo, hi] = key.to_le_bytes();
    let xor_ror = |a: u8, b: u8| (a ^ b).rotate_right(1);
    let mut out = [0u8; 16];
    let len = password.len();
    let mut i = len;
    if len % 2 == 1 {
        *out.get_mut(i)? = xor_ror(PAD[0], hi);
        i -= 1;
        *out.get_mut(i)? = xor_ror(*password.get(len - 1)?, lo);
    }
    while i > 0 {
        i -= 1;
        *out.get_mut(i)? = xor_ror(*password.get(i)?, hi);
        i = i.checked_sub(1)?;
        *out.get_mut(i)? = xor_ror(*password.get(i)?, lo);
    }
    let mut i = 15usize;
    let mut pad = 15 - len;
    while pad > 0 {
        *out.get_mut(i)? = xor_ror(*PAD.get(pad)?, hi);
        i = i.checked_sub(1)?;
        pad -= 1;
        *out.get_mut(i)? = xor_ror(*PAD.get(pad)?, lo);
        // An even password length leaves an odd pad count: the loop ends after PadArray[0].
        if pad == 0 {
            break;
        }
        i = i.checked_sub(1)?;
        pad -= 1;
    }
    Some(out)
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The RC4 stream cipher.
struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    fn new(key: &[u8]) -> Rc4 {
        let mut s = [0u8; 256];
        for (k, v) in s.iter_mut().enumerate() {
            *v = k as u8;
        }
        if !key.is_empty() {
            let mut j = 0u8;
            for i in 0..256 {
                j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
                s.swap(i, usize::from(j));
            }
        }
        Rc4 { s, i: 0, j: 0 }
    }

    fn next(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[usize::from(self.i)]);
        self.s.swap(usize::from(self.i), usize::from(self.j));
        self.s[usize::from(self.s[usize::from(self.i)].wrapping_add(self.s[usize::from(self.j)]))]
    }

    fn skip(&mut self, n: usize) {
        for _ in 0..n {
            self.next();
        }
    }
}

/// MD5 digest (RFC 1321).
pub(crate) fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16,
        23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    // T[i] = floor(2^32 * |sin(i + 1)|), RFC 1321 section 3.4.
    const T: [u32; 64] = [
        0xD76A_A478,
        0xE8C7_B756,
        0x2420_70DB,
        0xC1BD_CEEE,
        0xF57C_0FAF,
        0x4787_C62A,
        0xA830_4613,
        0xFD46_9501,
        0x6980_98D8,
        0x8B44_F7AF,
        0xFFFF_5BB1,
        0x895C_D7BE,
        0x6B90_1122,
        0xFD98_7193,
        0xA679_438E,
        0x49B4_0821,
        0xF61E_2562,
        0xC040_B340,
        0x265E_5A51,
        0xE9B6_C7AA,
        0xD62F_105D,
        0x0244_1453,
        0xD8A1_E681,
        0xE7D3_FBC8,
        0x21E1_CDE6,
        0xC337_07D6,
        0xF4D5_0D87,
        0x455A_14ED,
        0xA9E3_E905,
        0xFCEF_A3F8,
        0x676F_02D9,
        0x8D2A_4C8A,
        0xFFFA_3942,
        0x8771_F681,
        0x6D9D_6122,
        0xFDE5_380C,
        0xA4BE_EA44,
        0x4BDE_CFA9,
        0xF6BB_4B60,
        0xBEBF_BC70,
        0x289B_7EC6,
        0xEAA1_27FA,
        0xD4EF_3085,
        0x0488_1D05,
        0xD9D4_D039,
        0xE6DB_99E5,
        0x1FA2_7CF8,
        0xC4AC_5665,
        0xF429_2244,
        0x432A_FF97,
        0xAB94_23A7,
        0xFC93_A039,
        0x655B_59C3,
        0x8F0C_CC92,
        0xFFEF_F47D,
        0x8584_5DD1,
        0x6FA8_7E4F,
        0xFE2C_E6E0,
        0xA301_4314,
        0x4E08_11A1,
        0xF753_7E82,
        0xBD3A_F235,
        0x2AD7_D2BB,
        0xEB86_D391,
    ];
    let mut state: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    for chunk in msg.chunks_exact(64) {
        let x: [u32; 16] = std::array::from_fn(|k| {
            let w = chunk.get(k * 4..k * 4 + 4).unwrap_or(&[0; 4]);
            u32::from_le_bytes([w[0], w[1], w[2], w[3]])
        });
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((b & d) | (c & !d), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let sum = a.wrapping_add(f).wrapping_add(T[i]).wrapping_add(x[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(S[i]));
        }
        state = [state[0].wrapping_add(a), state[1].wrapping_add(b), state[2].wrapping_add(c), state[3].wrapping_add(d)];
    }
    let mut out = [0u8; 16];
    for (o, w) in out.chunks_exact_mut(4).zip(state) {
        o.copy_from_slice(&w.to_le_bytes());
    }
    out
}
