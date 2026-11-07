//! Excel 5.0/95 (BIFF5) pictures: an OBJ record with object type 8 carries the anchor and the
//! name, and the IMDATA record after it ([MS-XLS] 2.4.140) carries the image, here a 24-bit
//! bitmap. Every workbook is assembled here from the BIFF5 record layouts in a `Book` stream; no
//! Excel-produced file is used.

use gridcraft_core::CellRef;

use super::xls_tests::compound;
use crate::{read_xls, read_xlsx, write_xlsx};

const FORMAT_WARNING: &str = "use a format that isn't imported";

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(vers: u16, dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[vers, dt, 0x0DBB, 0x07CC]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

/// A Book stream: globals (CodePage 1252, one BoundSheet) and one worksheet with `records`.
fn book(records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 5], b"Sheet"].concat());
    let head = [bof(0x0500, 0x0005), rec(0x0042, &1252u16.to_le_bytes())].concat();
    let pos = (head.len() + boundsheet(0).len() + eof().len()) as u32;
    let globals = [head, boundsheet(pos), eof()].concat();
    let body = [bof(0x0500, 0x0010), records.to_vec(), eof()].concat();
    compound("Book", &[globals, body].concat())
}

/// A BIFF5 picture OBJ (ot 8), laid out as Excel 95 writes it: cObj, ot, id, grbit, the anchor
/// (colL, dxL, rwT, dyT, colR, dxR, rwB, dyB), cbMacro 0, 6 reserved bytes, 10 colour/line
/// bytes, cf, 4 reserved, cbPictFmla 0, 2 reserved, grbit, 4 reserved, then the name (cch and
/// 8-bit characters), padded to an even length.
fn picture_obj(id: u16, anchor: [u16; 8], cf: u16, name: &[u8]) -> Vec<u8> {
    let mut p = [&1u32.to_le_bytes()[..], &u16s(&[8, id, 0x0614]), &u16s(&anchor), &[0; 8]].concat();
    p.extend_from_slice(&[0x09, 0x09, 0x00, 0x00, 0x08, 0xFF, 0x01, 0x00, 0x00, 0x00]);
    p.extend(u16s(&[cf, 0, 0, 0, 0, 0, 0, 0]));
    assert_eq!(p.len(), 60);
    p.push(name.len() as u8);
    p.extend_from_slice(name);
    if !p.len().is_multiple_of(2) {
        p.push(0);
    }
    rec(0x005D, &p)
}

/// A 24-bit BITMAPCOREINFO: a 12-byte BITMAPCOREHEADER, then rows bottom first, each pixel
/// B, G, R and each row padded to 4 bytes. `rows` are given top first as RGB triples.
fn core_bitmap(rows: &[Vec<[u8; 3]>]) -> Vec<u8> {
    let w = rows[0].len() as u16;
    let h = rows.len() as u16;
    let mut d = [&12u32.to_le_bytes()[..], &u16s(&[w, h, 1, 24])].concat();
    for row in rows.iter().rev() {
        let mut line: Vec<u8> = row.iter().flat_map(|[r, g, b]| [*b, *g, *r]).collect();
        while !line.len().is_multiple_of(4) {
            line.push(0);
        }
        d.extend(line);
    }
    d
}

/// IMDATA: cf, env 1 (Windows), lcb, then the data, split into CONTINUE records after
/// `first` bytes of payload when the data is longer.
fn imdata(cf: u16, data: &[u8], first: usize) -> Vec<u8> {
    let payload = [&u16s(&[cf, 1])[..], &(data.len() as u32).to_le_bytes(), data].concat();
    let mut out = rec(0x007F, &payload[..first.min(payload.len())]);
    for chunk in payload.get(first..).unwrap_or_default().chunks(2064) {
        out.extend(rec(0x003C, chunk));
    }
    out
}

/// The PNG chunks (type, data), checking the signature and every chunk's CRC-32.
fn png_chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    fn crc32(bytes: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for b in bytes {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
    let mut at = 8;
    let mut out = vec![];
    while at < png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let ty: [u8; 4] = png[at + 4..at + 8].try_into().unwrap();
        let data = png[at + 8..at + 8 + len].to_vec();
        let crc = u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap());
        assert_eq!(crc, crc32(&png[at + 4..at + 8 + len]), "CRC of chunk {}", String::from_utf8_lossy(&ty));
        out.push((ty, data));
        at += 12 + len;
    }
    out
}

/// (width, height, bit depth, colour type) from IHDR; the PNG must end with IEND and hold IDAT.
fn png_header(png: &[u8]) -> (u32, u32, u8, u8) {
    let chunks = png_chunks(png);
    assert_eq!(&chunks[0].0, b"IHDR");
    assert_eq!(&chunks.last().unwrap().0, b"IEND");
    assert!(chunks.iter().any(|(t, d)| t == b"IDAT" && !d.is_empty()), "no IDAT");
    let d = &chunks[0].1;
    (u32::from_be_bytes(d[0..4].try_into().unwrap()), u32::from_be_bytes(d[4..8].try_into().unwrap()), d[8], d[9])
}

fn red_green_blue_white(w: usize) -> Vec<Vec<[u8; 3]>> {
    vec![vec![[255, 0, 0]; w], vec![[0, 255, 0]; w], vec![[0, 0, 255]; w], vec![[255, 255, 255]; w]]
}

#[test]
fn bitmap_picture_opens_as_a_png_at_its_place_with_its_name() {
    // 3 pixels wide: 9 bytes per row, padded to 12.
    let bmp = core_bitmap(&red_green_blue_white(3));
    let recs = [picture_obj(35, [3, 123, 15, 0, 4, 1005, 16, 192], 9, b"Logo"), imdata(9, &bmp, 2000)].concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let images = &wb.sheet(0).unwrap().images;
    assert_eq!(images.len(), 1, "{:?}", report.warnings);
    let img = &images[0];
    assert_eq!(img.mime, "image/png");
    assert_eq!(png_header(&img.data), (3, 4, 8, 2), "3x4, 8-bit RGB");
    assert_eq!(img.name.as_deref(), Some("Logo"));
    assert_eq!(img.anchor.cell, CellRef::new(15, 3), "top-left cell is (rwT, colL)");
    assert!(img.anchor.width > 0.0 && img.anchor.height > 0.0, "{:?}", img.anchor);
    assert!(!report.warnings.iter().any(|w| w.contains(FORMAT_WARNING)), "{:?}", report.warnings);
}

#[test]
fn large_bitmap_split_over_continue_records_opens() {
    // 233 x 40 pixels: 699 bytes per row padded to 700, 28,012 bytes over several CONTINUEs,
    // the first record cut mid-row as Excel 95 does.
    let rows: Vec<Vec<[u8; 3]>> = (0..40).map(|y| (0..233).map(|x| [x as u8, y as u8, 7]).collect()).collect();
    let bmp = core_bitmap(&rows);
    assert_eq!(bmp.len(), 12 + 700 * 40);
    let recs = [picture_obj(1, [0, 0, 0, 0, 0, 512, 1, 0], 9, b"Banner"), imdata(9, &bmp, 2068)].concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let images = &wb.sheet(0).unwrap().images;
    assert_eq!(images.len(), 1, "{:?}", report.warnings);
    assert_eq!(png_header(&images[0].data), (233, 40, 8, 2));
    assert_eq!(images[0].anchor.cell, CellRef::new(0, 0));
}

#[test]
fn several_pictures_keep_their_order_names_and_places() {
    let bmp = core_bitmap(&red_green_blue_white(2));
    let recs = [
        picture_obj(1, [0, 0, 0, 0, 1, 0, 2, 0], 9, b"PHPExcel logo"),
        imdata(9, &bmp, 2000),
        picture_obj(2, [5, 0, 9, 0, 6, 0, 11, 0], 9, b"Terms and conditions"),
        imdata(9, &bmp, 2000),
    ]
    .concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let got: Vec<_> = wb.sheet(0).unwrap().images.iter().map(|i| (i.name.clone().unwrap_or_default(), i.anchor.cell)).collect();
    assert_eq!(
        got,
        [("PHPExcel logo".to_string(), CellRef::new(0, 0)), ("Terms and conditions".to_string(), CellRef::new(9, 5))],
        "{:?}",
        report.warnings
    );
}

#[test]
fn picture_survives_save_as_xlsx() {
    let bmp = core_bitmap(&red_green_blue_white(5));
    let recs = [picture_obj(7, [2, 0, 3, 0, 4, 0, 8, 0], 9, b"Logo"), imdata(9, &bmp, 2000)].concat();
    let (wb, _) = read_xls(&book(&recs)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let images = &back.sheet(0).unwrap().images;
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].data, wb.sheet(0).unwrap().images[0].data, "same PNG bytes after Save As .xlsx");
    assert_eq!(images[0].anchor.cell, CellRef::new(3, 2));
}

#[test]
fn metafile_picture_is_skipped_with_the_format_warning() {
    // cf 2: a Windows metafile (an empty one: 18-byte header + 3-word end record).
    let wmf = [&u16s(&[1, 9, 0x0300, 12, 0, 0, 3, 0, 0])[..], &u16s(&[3, 0, 0])].concat();
    let recs = [picture_obj(1, [0, 205, 0, 0, 0, 870, 5, 0], 2, b"Immagine 1 "), imdata(2, &wmf, 2000)].concat();
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    assert!(wb.sheet(0).unwrap().images.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains(FORMAT_WARNING)), "{:?}", report.warnings);
}

#[test]
fn hostile_imdata_never_panics_or_imports_garbage() {
    let good = core_bitmap(&red_green_blue_white(3));
    let mut huge = good.clone();
    huge[4..8].copy_from_slice(&u16s(&[0xFFFF, 0xFFFF])); // 65535 x 65535 claimed, 48 bytes given
    let mut zero = good.clone();
    zero[4..6].copy_from_slice(&0u16.to_le_bytes());
    let mut bits8 = good.clone();
    bits8[10..12].copy_from_slice(&8u16.to_le_bytes());
    let cases: Vec<Vec<u8>> = vec![
        imdata(9, &huge, 2000),
        imdata(9, &zero, 2000),
        imdata(9, &bits8, 2000),
        imdata(9, &good[..20], 2000), // rows cut short
        imdata(9, &[], 2000),
        rec(0x007F, &u16s(&[9, 1])),                                          // no lcb
        rec(0x007F, &[&u16s(&[9, 1])[..], &u32::MAX.to_le_bytes()].concat()), // lcb past the end
    ];
    for (i, im) in cases.iter().enumerate() {
        let recs = [picture_obj(1, [0, 0, 0, 0, 1, 0, 1, 0], 9, b"x"), im.clone()].concat();
        let (wb, _) = read_xls(&book(&recs)).unwrap_or_else(|e| panic!("case {i}: {e:?}"));
        assert!(wb.sheet(0).unwrap().images.is_empty(), "case {i} imported a picture");
    }
    // Short picture OBJ records (no name, cut anchor) and an IMDATA with no OBJ before it.
    let obj = picture_obj(1, [0, 0, 0, 0, 1, 0, 1, 0], 9, b"x");
    for cut in [8usize, 20, 30, 50, 60] {
        let short = rec(0x005D, &obj[4..4 + cut]);
        let recs = [short, imdata(9, &good, 2000)].concat();
        assert!(read_xls(&book(&recs)).is_ok(), "OBJ cut at {cut}");
    }
    let (wb, _) = read_xls(&book(&imdata(9, &good, 2000))).unwrap();
    assert!(wb.sheet(0).unwrap().images.is_empty(), "IMDATA without a picture OBJ is ignored");
}
