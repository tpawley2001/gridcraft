//! Legacy `.xls` workbooks encrypted with Excel's default password (`VelvetSweatshop`) open
//! without a prompt ([MS-XLS] 2.2.10, 2.4.117; [MS-OFFCRYPTO] 2.3.5 RC4 CryptoAPI, 2.3.6 RC4).
//! Every workbook is built and encrypted here, straight from the spec; no Excel-produced file.

use sha1::{Digest, Sha1};

use super::xls_tests::{bof, cat, cell, compound, eof, rec, v};
use crate::read_xls;
use crate::xls_crypto::md5;
use gridcraft_core::Value;

const DEFAULT: &str = "VelvetSweatshop";
const SALT: [u8; 16] = [7, 1, 9, 3, 200, 17, 66, 5, 250, 33, 90, 12, 1, 0, 128, 77];
const VERIFIER: [u8; 16] = [11, 22, 33, 44, 55, 66, 77, 88, 99, 110, 121, 132, 143, 154, 165, 176];

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// RC4 keystream of `n` bytes.
fn rc4(key: &[u8], n: usize) -> Vec<u8> {
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    (0..n)
        .map(|_| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            s[s[i as usize].wrapping_add(s[j as usize]) as usize]
        })
        .collect()
}

fn xor(data: &[u8], ks: &[u8]) -> Vec<u8> {
    data.iter().zip(ks).map(|(a, b)| a ^ b).collect()
}

#[derive(Clone, Copy)]
pub(super) enum Scheme {
    /// RC4 CryptoAPI with this key size in bits (40 or 128).
    CryptoApi(u32),
    /// Office binary document RC4 (version 1.1, MD5).
    Rc4,
}

/// The RC4 key for a 1024-byte block.
fn block_key(scheme: Scheme, password: &str, block: u32) -> Vec<u8> {
    match scheme {
        Scheme::CryptoApi(bits) => {
            let h0 = Sha1::digest(cat(&[&SALT, &utf16(password)]));
            let h = Sha1::digest(cat(&[&h0, &block.to_le_bytes()]));
            let mut key = h[..(bits / 8) as usize].to_vec();
            if bits == 40 {
                key.resize(16, 0);
            }
            key
        }
        Scheme::Rc4 => {
            let h0 = md5(&utf16(password));
            let unit = cat(&[&h0[..5], &SALT]);
            let h1 = md5(&unit.repeat(16));
            md5(&cat(&[&h1[..5], &block.to_le_bytes()])).to_vec()
        }
    }
}

/// The FilePass payload for the scheme and password.
fn filepass(scheme: Scheme, password: &str) -> Vec<u8> {
    let ks = rc4(&block_key(scheme, password, 0), 16 + 20);
    match scheme {
        Scheme::CryptoApi(bits) => {
            let hash = Sha1::digest(VERIFIER);
            let enc = xor(&cat(&[&VERIFIER, &hash]), &ks);
            let csp = utf16("Microsoft Base Cryptographic Provider v1.0\0");
            let header = cat(&[
                &4u32.to_le_bytes(),      // Flags: fCryptoAPI
                &0u32.to_le_bytes(),      // SizeExtra
                &0x6801u32.to_le_bytes(), // AlgID: RC4
                &0x8004u32.to_le_bytes(), // AlgIDHash: SHA-1
                &bits.to_le_bytes(),      // KeySize
                &1u32.to_le_bytes(),      // ProviderType: PROV_RSA_FULL
                &0u32.to_le_bytes(),
                &0u32.to_le_bytes(),
                &csp,
            ]);
            cat(&[
                &1u16.to_le_bytes(), // wEncryptionType: RC4
                &4u16.to_le_bytes(),
                &2u16.to_le_bytes(), // version 4.2
                &4u32.to_le_bytes(), // flags
                &(header.len() as u32).to_le_bytes(),
                &header,
                &16u32.to_le_bytes(),
                &SALT,
                &enc[..16],
                &20u32.to_le_bytes(),
                &enc[16..],
            ])
        }
        Scheme::Rc4 => {
            let hash = md5(&VERIFIER);
            let enc = xor(&cat(&[&VERIFIER, &hash]), &ks);
            cat(&[&1u16.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes(), &SALT, &enc[..16], &enc[16..32]])
        }
    }
}

/// Encrypts a BIFF8 stream whose second record is a FILEPASS: record headers, BOF and FILEPASS stay
/// plain, so do the first 4 bytes (lbPlyPos) of each BOUNDSHEET8; the keystream counts every byte
/// and re-keys at each 1024-byte block of the stream.
fn encrypt(stream: &[u8], scheme: Scheme, password: &str) -> Vec<u8> {
    let mut plain_mask = vec![true; stream.len()];
    let mut at = 0;
    while at + 4 <= stream.len() {
        let id = u16::from_le_bytes([stream[at], stream[at + 1]]);
        let len = u16::from_le_bytes([stream[at + 2], stream[at + 3]]) as usize;
        let body = at + 4..at + 4 + len;
        let skip = match id {
            0x0809 | 0x002F | 0x0194 | 0x0195 | 0x00E1 | 0x0196 | 0x0138 => len,
            0x0085 => 4,
            _ => 0,
        };
        for i in body.clone().skip(skip) {
            plain_mask[i] = false;
        }
        at = body.end;
    }
    let blocks = stream.len().div_ceil(1024);
    let ks: Vec<u8> = (0..blocks).flat_map(|b| rc4(&block_key(scheme, password, b as u32), 1024)).collect();
    stream.iter().enumerate().map(|(i, &b)| if plain_mask[i] { b } else { b ^ ks[i] }).collect()
}

fn number(r: u16, c: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&cell(r, c, 0), &n.to_le_bytes()]))
}

fn boundsheet(pos: u32, name: &str) -> Vec<u8> {
    rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, name.len() as u8, 0], name.as_bytes()]))
}

/// A workbook stream: BOF, FILEPASS, one BOUNDSHEET8 per sheet, EOF, then each sheet with
/// `values` (A1 = values[0], A2 = values[1], ...). The first sheet gets `pad` extra numbers in
/// column C so the stream spans several 1024-byte blocks.
fn workbook(fp: &[u8], sheets: &[&[f64]], pad: u16) -> Vec<u8> {
    let names: Vec<String> = (1..=sheets.len()).map(|i| format!("Sheet{i}")).collect();
    let bodies: Vec<Vec<u8>> = sheets
        .iter()
        .enumerate()
        .map(|(s, vals)| {
            let mut b = bof(0x0010);
            for (r, &x) in vals.iter().enumerate() {
                b.extend(number(r as u16, 0, x));
            }
            if s == 0 {
                for r in 0..pad {
                    b.extend(number(r, 2, f64::from(r) + 0.5));
                }
            }
            b.extend(eof());
            b
        })
        .collect();
    let globals_len = |n: &[String]| {
        let mut g = cat(&[&bof(0x0005), &rec(0x002F, fp)]);
        for name in n {
            g.extend(boundsheet(0, name));
        }
        g.extend(eof());
        g.len()
    };
    let mut pos = globals_len(&names) as u32;
    let mut globals = cat(&[&bof(0x0005), &rec(0x002F, fp)]);
    for (name, body) in names.iter().zip(&bodies) {
        globals.extend(boundsheet(pos, name));
        pos += body.len() as u32;
    }
    globals.extend(eof());
    let mut stream = globals;
    for b in &bodies {
        stream.extend_from_slice(b);
    }
    stream
}

pub(super) fn encrypted_book(scheme: Scheme, password: &str, sheets: &[&[f64]], pad: u16) -> Vec<u8> {
    let stream = workbook(&filepass(scheme, password), sheets, pad);
    compound("Workbook", &encrypt(&stream, scheme, password))
}

fn assert_opens(scheme: Scheme) {
    let file = encrypted_book(scheme, DEFAULT, &[&[42.0, -7.25]], 200);
    let (wb, report) = read_xls(&file).unwrap_or_else(|e| panic!("default-password workbook refused: {e}"));
    assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
    assert_eq!(v(&wb, 0, "A2"), Value::Number(-7.25));
    // Values past the first and second 1024-byte blocks.
    assert_eq!(v(&wb, 0, "C1"), Value::Number(0.5));
    assert_eq!(v(&wb, 0, "C200"), Value::Number(199.5));
    assert!(!report.warnings.iter().any(|w| w.to_lowercase().contains("password")), "{:?}", report.warnings);
}

#[test]
fn md5_rfc1321_vectors() {
    let cases: [(&[u8], &str); 7] = [
        (b"", "d41d8cd98f00b204e9800998ecf8427e"),
        (b"a", "0cc175b9c0f1b6a831c399e269772661"),
        (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
        (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        (b"abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b"),
        (b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "d174ab98d277d9f5a5611c2c9f419d9f"),
        (b"12345678901234567890123456789012345678901234567890123456789012345678901234567890", "57edf4a22be3c955ac49da2e2107b67a"),
    ];
    for (input, digest) in cases {
        assert_eq!(hex(&md5(input)), digest, "{:?}", String::from_utf8_lossy(input));
    }
}

#[test]
fn rc4_cryptoapi_default_password_opens() {
    assert_opens(Scheme::CryptoApi(128));
}

#[test]
fn rc4_cryptoapi_40bit_default_password_opens() {
    assert_opens(Scheme::CryptoApi(40));
}

#[test]
fn rc4_md5_default_password_opens() {
    assert_opens(Scheme::Rc4);
}

#[test]
fn boundsheet_offset_stays_plain() {
    let file = encrypted_book(Scheme::CryptoApi(128), DEFAULT, &[&[1.0], &[2.0, 3.0]], 120);
    let (wb, _) = read_xls(&file).unwrap();
    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(wb.sheets[1].name, "Sheet2");
    assert_eq!(v(&wb, 0, "A1"), Value::Number(1.0));
    assert_eq!(v(&wb, 1, "A2"), Value::Number(3.0));
}

#[test]
fn other_password_still_refused() {
    for scheme in [Scheme::CryptoApi(128), Scheme::Rc4] {
        let file = encrypted_book(scheme, "secret", &[&[42.0]], 10);
        let err = read_xls(&file).unwrap_err().to_string();
        assert!(err.contains("password-protected"), "{err}");
    }
}

#[test]
fn hostile_filepass_never_panics() {
    for scheme in [Scheme::CryptoApi(128), Scheme::Rc4] {
        let fp = filepass(scheme, DEFAULT);
        for cut in 0..fp.len() {
            let stream = workbook(&fp[..cut], &[&[1.0]], 0);
            assert!(read_xls(&compound("Workbook", &stream)).is_err(), "{cut}");
        }
    }
    // KeySize 0 and 4096 bits; version 9.9.
    let fp = filepass(Scheme::CryptoApi(128), DEFAULT);
    let key_size_at = 2 + 4 + 4 + 4 + 16;
    for bits in [0u32, 4096] {
        let mut bad = fp.clone();
        bad[key_size_at..key_size_at + 4].copy_from_slice(&bits.to_le_bytes());
        let _ = read_xls(&compound("Workbook", &workbook(&bad, &[&[1.0]], 0)));
    }
    let mut bad = fp;
    bad[2..6].copy_from_slice(&[9, 0, 9, 0]);
    assert!(read_xls(&compound("Workbook", &workbook(&bad, &[&[1.0]], 0))).is_err());
}
