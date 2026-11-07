//! Legacy `.xls` workbooks obfuscated with XOR ([MS-XLS] 2.2.10, [MS-OFFCRYPTO] 2.3.7 method 1)
//! under Excel's default password (`VelvetSweatshop`) open without a prompt, BIFF8 and BIFF5.
//! The obfuscation here is written straight from the spec's pseudocode, apart from the reader.

use super::xls_tests::{bof, cat, cell, compound, eof, rec, u16s, v, xlstr};
use crate::read_xls;
use gridcraft_core::{CellRef, Value};

const DEFAULT: &str = "VelvetSweatshop";

const INITIAL_CODE: [u16; 15] =
    [0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9, 0x280C, 0xA96A, 0x4EC3];

const MATRIX: [[u16; 7]; 15] = [
    [0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09],
    [0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF],
    [0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0],
    [0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40],
    [0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5],
    [0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A],
    [0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9],
    [0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0],
    [0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC],
    [0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10],
    [0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168],
    [0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C],
    [0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD],
    [0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC],
    [0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4],
];

const PAD: [u8; 15] = [0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00];

/// CreateXorKey_Method1: the last character uses the last matrix row, bit 6 first.
pub(super) fn xor_key(pw: &[u8]) -> u16 {
    let mut key = INITIAL_CODE[pw.len() - 1];
    for (i, &ch) in pw.iter().enumerate() {
        let row = 15 - pw.len() + i;
        for (bit, m) in MATRIX[row].iter().enumerate() {
            if ch & (1 << bit) != 0 {
                key ^= m;
            }
        }
    }
    key
}

/// CreatePasswordVerifier_Method1: over (length, password bytes) in reverse, a 15-bit rotation.
pub(super) fn verifier(pw: &[u8]) -> u16 {
    let mut arr = vec![pw.len() as u8];
    arr.extend_from_slice(pw);
    let mut v: u16 = 0;
    for &b in arr.iter().rev() {
        let carry = u16::from(v & 0x4000 != 0);
        v = (((v << 1) & 0x7FFF) | carry) ^ u16::from(b);
    }
    v ^ 0xCE4B
}

/// CreateXorArray_Method1, with signed indexes as in the pseudocode.
fn xor_array(pw: &[u8]) -> [u8; 16] {
    let key = xor_key(pw);
    let (hi, lo) = ((key >> 8) as u8, key as u8);
    let xr = |a: u8, b: u8| (a ^ b).rotate_right(1);
    let mut out = [0u8; 16];
    let mut index = pw.len() as i32;
    if index % 2 == 1 {
        out[index as usize] = xr(PAD[0], hi);
        index -= 1;
        out[index as usize] = xr(pw[pw.len() - 1], lo);
    }
    while index > 0 {
        index -= 1;
        out[index as usize] = xr(pw[index as usize], hi);
        index -= 1;
        out[index as usize] = xr(pw[index as usize], lo);
    }
    let mut index = 15i32;
    let mut pad = 15 - pw.len() as i32;
    while pad > 0 {
        out[index as usize] = xr(PAD[pad as usize], hi);
        index -= 1;
        pad -= 1;
        out[index as usize] = xr(PAD[pad as usize], lo);
        index -= 1;
        pad -= 1;
    }
    out
}

/// Obfuscates every record body except BOF, FilePass and the other always-plain records, and
/// the lbPlyPos of each BoundSheet: rotate left 5, then XOR with the array at
/// (stream offset + record size) mod 16.
pub(super) fn obfuscate(stream: &[u8], pw: &str) -> Vec<u8> {
    let array = xor_array(pw.as_bytes());
    let mut out = stream.to_vec();
    let mut at = 0;
    while at + 4 <= stream.len() {
        let id = u16::from_le_bytes([stream[at], stream[at + 1]]);
        let size = u16::from_le_bytes([stream[at + 2], stream[at + 3]]) as usize;
        let plain = match id {
            0x0809 | 0x002F | 0x0194 | 0x0195 | 0x00E1 | 0x0196 | 0x0138 => size,
            0x0085 => 4,
            _ => 0,
        };
        for p in at + 4 + plain..at + 4 + size {
            out[p] = stream[p].rotate_left(5) ^ array[(p + size) % 16];
        }
        at += 4 + size;
    }
    out
}

pub(super) fn filepass8(pw: &str) -> Vec<u8> {
    u16s(&[0, xor_key(pw.as_bytes()), verifier(pw.as_bytes())])
}

/// BIFF8: Sheet1 A1 = 42, A2 = "hello" (shared string), B1 = A1*2 (cached 84), padded with
/// numbers in column D so the stream crosses several 1024-byte blocks.
pub(super) fn biff8_workbook(fp: &[u8], pad: u16) -> Vec<u8> {
    let mut sheet = bof(0x0010);
    sheet.extend(rec(0x0203, &cat(&[&cell(0, 0, 0), &42f64.to_le_bytes()])));
    sheet.extend(rec(0x00FD, &cat(&[&cell(1, 0, 0), &0u32.to_le_bytes()])));
    let rgce = cat(&[&[0x24], &u16s(&[0, 0xC000]), &[0x1E], &2u16.to_le_bytes(), &[0x05]]);
    sheet.extend(rec(0x0006, &cat(&[&cell(0, 1, 0), &84f64.to_le_bytes(), &[0; 6], &(rgce.len() as u16).to_le_bytes(), &rgce])));
    for r in 0..pad {
        sheet.extend(rec(0x0203, &cat(&[&cell(r, 3, 0), &(f64::from(r) + 0.25).to_le_bytes()])));
    }
    sheet.extend(eof());
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x002F, fp),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x00FC, &cat(&[&1u32.to_le_bytes(), &1u32.to_le_bytes(), &xlstr("hello")])),
            &eof(),
        ])
    };
    let pos = globals(0).len() as u32;
    cat(&[&globals(pos), &sheet])
}

fn biff5_bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[0x0500, dt, 0x0DBB, 0x07CC]))
}

/// BIFF5 `Book` stream: a 4-byte FilePass (key, verifier) and Sheet1 A1 = 42.
pub(super) fn biff5_book(fp: &[u8]) -> Vec<u8> {
    let sheet = cat(&[&biff5_bof(0x0010), &rec(0x0203, &cat(&[&cell(0, 0, 0), &42f64.to_le_bytes()])), &eof()]);
    let globals = |pos: u32| {
        cat(&[
            &biff5_bof(0x0005),
            &rec(0x002F, fp),
            &rec(0x0042, &1252u16.to_le_bytes()),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6], b"Sheet1"])),
            &eof(),
        ])
    };
    let pos = globals(0).len() as u32;
    cat(&[&globals(pos), &sheet])
}

#[test]
fn verifier_matches_the_published_legacy_hash() {
    // The same 16-bit verifier Excel writes for sheet protection: "password" hashes to 0x83AF.
    assert_eq!(verifier(b"password"), 0x83AF);
    assert_eq!(verifier(DEFAULT.as_bytes()), gridcraft_model::password::legacy_hash(DEFAULT));
}

#[test]
fn xor_default_password_opens_biff8() {
    let stream = biff8_workbook(&filepass8(DEFAULT), 160);
    assert!(stream.len() > 2048);
    let (wb, report) = read_xls(&compound("Workbook", &obfuscate(&stream, DEFAULT))).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
    assert_eq!(v(&wb, 0, "A2"), Value::Text("hello".into()));
    assert_eq!(v(&wb, 0, "B1"), Value::Number(84.0));
    let f = wb.sheets[0].cell(CellRef::parse("B1").unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone());
    assert_eq!(f.as_deref(), Some("A1*2"));
    assert_eq!(v(&wb, 0, "D160"), Value::Number(159.25));
    assert!(!report.warnings.iter().any(|w| w.to_lowercase().contains("password")), "{:?}", report.warnings);
}

#[test]
fn xor_default_password_opens_biff5() {
    let stream = biff5_book(&u16s(&[xor_key(DEFAULT.as_bytes()), verifier(DEFAULT.as_bytes())]));
    let (wb, report) = read_xls(&compound("Book", &obfuscate(&stream, DEFAULT))).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
    assert!(!report.warnings.iter().any(|w| w.to_lowercase().contains("password")), "{:?}", report.warnings);
}

#[test]
fn xor_other_password_still_refused() {
    let stream = biff8_workbook(&filepass8("secret"), 0);
    let err = read_xls(&compound("Workbook", &obfuscate(&stream, "secret"))).unwrap_err().to_string();
    assert!(err.contains("password-protected"), "{err}");
    let book = biff5_book(&u16s(&[xor_key(b"secret"), verifier(b"secret")]));
    let err = read_xls(&compound("Book", &obfuscate(&book, "secret"))).unwrap_err().to_string();
    assert!(err.contains("password-protected") && err.contains("BIFF5"), "{err}");
}

#[test]
fn xor_hostile_filepass_never_panics() {
    let fp = filepass8(DEFAULT);
    for cut in 0..fp.len() {
        let stream = biff8_workbook(&fp[..cut], 0);
        assert!(read_xls(&compound("Workbook", &stream)).is_err(), "{cut}");
    }
    // A FilePass that does not follow the BOF stays refused, even with the default password.
    let stream = biff8_workbook(&filepass8(DEFAULT), 0);
    let moved = cat(&[&stream[..20], &rec(0x0042, &1252u16.to_le_bytes()), &stream[20..]]);
    assert!(read_xls(&compound("Workbook", &moved)).is_err());
}
