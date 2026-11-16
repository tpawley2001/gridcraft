//! `.xls` metafile pictures: EMF and WMF blips in the BStore ([MS-ODRAW] 2.2.24, 2.2.25, metafile
//! header 2.2.31) open as pictures, inflating their zlib-compressed data, and survive a save to `.xlsx`.

use gridcraft_model::Workbook;

use crate::{read_xlsx, write_xlsx};

use super::xls_chart_tests::{cat, obj, u16s};
use super::xls_picture_tests::{art, book, drawing_group, fbse, fopt, read};

const FORMAT_WARNING_PART: &str = "use a format that isn't imported";

/// A picture shape (FSP, FOPT with `pib`, anchor, client data) claimed by a picture Obj (`ot` 8).
fn picture(pib: u32) -> Vec<u8> {
    let fsp = art(2, 75, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 256, 2, 0, 3, 0, 6, 128]));
    let data = art(0, 0, 0xF011, &[]);
    let sp = art(0xF, 0, 0xF004, &cat(&[&fsp, &fopt(&[(0x4104, pib)], &[]), &anchor, &data]));
    cat(&[&super::xls_chart_tests::rec(0x00EC, &sp), &obj(8)])
}

#[allow(clippy::too_many_arguments)]
/// A metafile blip: header, one or two UIDs, the 34-byte OfficeArtMetafileHeader, then `stored`.
fn metafile_blip(ty: u16, inst: u16, uids: usize, cb_size: u32, bounds: [i32; 4], size: [i32; 2], compression: u8, stored: &[u8]) -> Vec<u8> {
    let mut head = vec![0xAB; 16 * uids];
    head.extend(cb_size.to_le_bytes());
    for v in bounds {
        head.extend(v.to_le_bytes());
    }
    for v in size {
        head.extend(v.to_le_bytes());
    }
    head.extend((stored.len() as u32).to_le_bytes());
    head.extend([compression, 0xFE]);
    art(0, inst, ty, &cat(&[&head, stored]))
}

fn store(blip: Vec<u8>) -> Vec<u8> {
    drawing_group(&[fbse(&blip)])
}

/// An EMF-looking payload: the EMR_HEADER record type, then filler.
fn emf_bytes() -> Vec<u8> {
    let mut v = vec![0x01, 0x00, 0x00, 0x00];
    for _ in 0..3 {
        v.extend(0u8..60);
    }
    v
}

/// `emf_bytes()` compressed by zlib at level 9 (fixed-Huffman block with back references).
const EMF_ZLIB: [u8; 73] = [
    0x78, 0xDA, 0x63, 0x64, 0x00, 0x02, 0x46, 0x26, 0x66, 0x16, 0x56, 0x36, 0x76, 0x0E, 0x4E, 0x2E, 0x6E, 0x1E, 0x5E, 0x3E, 0x7E, 0x01, 0x41, 0x21,
    0x61, 0x11, 0x51, 0x31, 0x71, 0x09, 0x49, 0x29, 0x69, 0x19, 0x59, 0x39, 0x79, 0x05, 0x45, 0x25, 0x65, 0x15, 0x55, 0x35, 0x75, 0x0D, 0x4D, 0x2D,
    0x6D, 0x1D, 0x5D, 0x3D, 0x7D, 0x03, 0x43, 0x23, 0x63, 0x13, 0x53, 0x33, 0x73, 0x0B, 0x4B, 0x2B, 0xEB, 0x81, 0xD2, 0x0B, 0x00, 0x84, 0x14, 0x14,
    0xC0,
];

/// Text whose zlib stream at level 9 uses a dynamic-Huffman block.
fn dynamic_text() -> Vec<u8> {
    (0..120u32).flat_map(|i| format!("row {i}: the quick brown fox {} jumps over {} lazy dogs; ", i * i % 97, i * 31 % 1000).into_bytes()).collect()
}

const DYNAMIC_ZLIB_HEX: &str = concat!(
    "78da8dd8499225450c04d0abe41142b304a761686628a8eea681d353cb527aca4ccb6ff6dd72889731f8ebcb97eb7c757dfae9c3f5d7e79fbffbf5fa",
    "f6f5e5cb1fd70f2fff5ce7fae5f3ef7f7ebc5efefef0faf6e3b76ffefbf7fafee5c78f5f5f6f7fb9e83944ef4342f7143fa7f47dcaf99e92e754bd4f",
    "95dc533adca1bf8f11eb3d67cf39b69633bbe7fc3927fd7ae9f75c0cefa43d1e53dc73f99cf3f63259f39eabe75cb6a1e32818f1c189f42147290315",
    "6e372a0a5868d2122d18e08506306fff7c17d403646830d3730264683023ed9daa83191ad078bf62011a1ad454536a0c6a6860236d18cd800d0d6ea2",
    "cd1396e0860737c4fdab07383cc0b176ab6e38cb0c70aa7d8a9e00870738daa806011c1ee05403100a727890a3ed1923400e0f72aadd6a1e90c3831c",
    "6be39802727890932de700870738dea866011c19e070f4891fe0c80027dbad96031c19e0587ba955b8420d70b819677023839b6c93bf011b19d878f3",
    "96a046a645aa2f6e046a645043ed82a4a04626357d590c6023139b16e4036c7460a3d9174660a3031be98318c0460736d4ae2807d8e8c0a63da2086e",
    "6da6e9a6afc50e6e7470936dfca5008e0e70a2415506383ac08976453580a3039c68cfa80970b436412380636773ab6600c768f3722c018ef166389c",
    "008ec906802bc8315d88f3c04db16d88c70138e69b8f2a04e0586c3ee3708063b99938a2008ed566aa4a06387e3693633ac071da4cc75900c77933ff",
    "17031c97cd825306705c372b5c25c871dbaca984c729df2ce20a6e3c36bb8600369e9b6d0a1d60e3b5d81791809a389b8d1805a809da6cfdf8809ae0",
    "cd669305d4846cb6b7eca02674b3a1e60235619b2dbc30b009df1c1ac4f0181e9b638a24c089dc1c8c94004ed4e628a60a72f26c0e7f9a20276973dc",
    "340239c99b03ae29c849599ca82d004eeae608ef07e0a46d4a03178093bea929dc014ec6a217f1c2fe2637454c30b8c9da343f61e0a6cea66a8a0237",
    "459b6e2b19dc146fcab4347053b269ef32014ee9a62c247053b6a8264b814df9a208ad0035158bd615cc546e6a57acfc6ad3bb3e347e67f132dffe85",
    "415a95af5810d35989212c89e9c8aa80c5a2988eae2a58ac8ae9d8aa84c5b2988eaf6a58ac8be9c4aa87c5e6efe4ae88c5eaefd4ae89454153697cab",
    "629f5a63da74b10fad31add62a7da88da7def8b63c3ef4c653717cab631f9ae3b13aeea7d587ea78ea8e6f85ec43774cb16b6491d0d41edf2b592434",
    "d5c7f74eb611fa1f1fe6d772",
);

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// A WMF payload starting at its METAHEADER (no placeable header), as Office stores it.
fn wmf_bytes() -> Vec<u8> {
    cat(&[&u16s(&[1, 9, 0x0300]), &[0x10; 30]])
}

/// The 22-byte placeable header for bounds (1158, -1706, 1210, -1654) and 165100 x 165100 EMUs:
/// 52 logical units across 165100 EMUs is 288 units per inch.
const WMF_PLACEABLE: [u8; 22] =
    [0xD7, 0xCD, 0xC6, 0x9A, 0x00, 0x00, 0x86, 0x04, 0x56, 0xF9, 0xBA, 0x04, 0x8A, 0xF9, 0x20, 0x01, 0x00, 0x00, 0x00, 0x00, 0xD1, 0x56];

fn format_warnings(warnings: &[String]) -> usize {
    warnings.iter().filter(|w| w.contains(FORMAT_WARNING_PART)).count()
}

fn only_image(wb: &Workbook, warnings: &[String]) -> (Vec<u8>, String) {
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.images.len(), 1, "{warnings:?}");
    assert_eq!(format_warnings(warnings), 0, "{warnings:?}");
    assert!(sheet.embedded_objects.is_empty(), "{:?}", sheet.embedded_objects);
    ((*sheet.images[0].data).clone(), sheet.images[0].mime.clone())
}

#[test]
fn xls_stored_emf_picture_opens_as_an_emf_image() {
    let emf = emf_bytes();
    let blip = metafile_blip(0xF01A, 0x3D4, 1, emf.len() as u32, [0, 0, 100, 50], [914400, 457200], 0xFE, &emf);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    let (data, mime) = only_image(&wb, &warnings);
    assert_eq!(mime, "image/x-emf");
    assert_eq!(data, emf);
}

#[test]
fn xls_compressed_emf_picture_is_inflated() {
    let emf = emf_bytes();
    let blip = metafile_blip(0xF01A, 0x3D4, 1, emf.len() as u32, [0, 0, 100, 50], [914400, 457200], 0x00, &EMF_ZLIB);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    let (data, mime) = only_image(&wb, &warnings);
    assert_eq!(mime, "image/x-emf");
    assert_eq!(data, emf);
}

#[test]
fn xls_emf_with_two_uids_and_a_dynamic_huffman_stream() {
    let text = dynamic_text();
    let z = unhex(DYNAMIC_ZLIB_HEX);
    assert_eq!(z[0], 0x78);
    let blip = metafile_blip(0xF01A, 0x3D5, 2, text.len() as u32, [0, 0, 10, 10], [91440, 91440], 0x00, &z);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    let (data, mime) = only_image(&wb, &warnings);
    assert_eq!(mime, "image/x-emf");
    assert_eq!(data.len(), 6814);
    assert_eq!(data, text);
}

#[test]
fn xls_wmf_picture_gets_a_placeable_header() {
    let wmf = wmf_bytes();
    let blip = metafile_blip(0xF01B, 0x216, 1, wmf.len() as u32, [1158, -1706, 1210, -1654], [165100, 165100], 0xFE, &wmf);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    let (data, mime) = only_image(&wb, &warnings);
    assert_eq!(mime, "image/x-wmf");
    assert_eq!(&data[..22], &WMF_PLACEABLE);
    assert_eq!(&data[22..], &wmf[..]);
}

#[test]
fn xls_corrupt_compressed_emf_is_skipped_with_the_format_warning() {
    let emf = emf_bytes();
    let mut bad = EMF_ZLIB.to_vec();
    bad[2] = 0xFF; // reserved block type 3
    let blip = metafile_blip(0xF01A, 0x3D4, 1, emf.len() as u32, [0, 0, 100, 50], [914400, 457200], 0x00, &bad);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    assert!(wb.sheets[0].images.is_empty());
    assert_eq!(format_warnings(&warnings), 1, "{warnings:?}");
}

#[test]
fn xls_emf_inflating_to_the_wrong_size_is_skipped() {
    // cbSize claims fewer bytes than the stream holds: never inflate past it.
    let blip = metafile_blip(0xF01A, 0x3D4, 1, 10, [0, 0, 100, 50], [914400, 457200], 0x00, &EMF_ZLIB);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    assert!(wb.sheets[0].images.is_empty());
    assert_eq!(format_warnings(&warnings), 1, "{warnings:?}");
}

#[test]
fn xls_pict_picture_is_still_skipped() {
    let blip = metafile_blip(0xF01C, 0x542, 1, 8, [0, 0, 10, 10], [91440, 91440], 0xFE, &[0; 8]);
    let (wb, warnings) = read(&book(&store(blip), &picture(1)));
    assert!(wb.sheets[0].images.is_empty());
    assert_eq!(format_warnings(&warnings), 1, "{warnings:?}");
}

#[test]
fn xls_emf_and_wmf_pictures_save_to_xlsx() {
    let emf = emf_bytes();
    let wmf = wmf_bytes();
    let blips = drawing_group(&[
        fbse(&metafile_blip(0xF01A, 0x3D4, 1, emf.len() as u32, [0, 0, 100, 50], [914400, 457200], 0x00, &EMF_ZLIB)),
        fbse(&metafile_blip(0xF01B, 0x216, 1, wmf.len() as u32, [1158, -1706, 1210, -1654], [165100, 165100], 0xFE, &wmf)),
    ]);
    let (wb, warnings) = read(&book(&blips, &cat(&[&picture(1), &picture(2)])));
    assert_eq!(wb.sheets[0].images.len(), 2, "{warnings:?}");
    let bytes = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    let mut got: Vec<(String, Vec<u8>)> = back.sheets[0].images.iter().map(|i| (i.mime.clone(), (*i.data).clone())).collect();
    got.sort();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], ("image/x-emf".to_string(), emf));
    assert_eq!(got[1].0, "image/x-wmf");
    assert_eq!(&got[1].1[..22], &WMF_PLACEABLE);
}
