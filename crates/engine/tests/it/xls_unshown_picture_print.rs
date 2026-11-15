//! A `.xls` picture object whose image is not imported (here the EMF preview of an embedded
//! object) still counts toward the printed range: with no print area the sheet prints from A1
//! to the last used cell or the far corner of any object with "Print object" on. Microsoft 365
//! Excel prints real files of this kind on one page more than their cells alone need.

use gridcraft_engine::Session;
use gridcraft_xlsx::testing::compound_file;
use serde_json::json;

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// An OfficeArt record: version and instance, type, then the body.
fn art(ver: u16, inst: u16, ty: u16, body: &[u8]) -> Vec<u8> {
    [&u16s(&[ver | (inst << 4), ty])[..], &(body.len() as u32).to_le_bytes(), body].concat()
}

/// An OfficeArtFBSE embedding `blip`.
fn fbse(blip: &[u8]) -> Vec<u8> {
    let mut head = vec![6, 6];
    head.extend([0x11; 16]);
    head.extend(u16s(&[0xFF]));
    head.extend((blip.len() as u32).to_le_bytes());
    head.extend(1u32.to_le_bytes());
    head.extend(0u32.to_le_bytes());
    head.extend([0, 0, 0, 0]);
    art(2, 6, 0xF007, &[head, blip.to_vec()].concat())
}

/// The globals' MsoDrawingGroup: blip 1 is a PNG, blip 2 an EMF ([MS-ODRAW] 2.2.24, 2.2.28).
fn drawing_group() -> Vec<u8> {
    let png = art(0, 0x6E0, 0xF01E, &[&[0xAB; 16][..], &[0xFF], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], b"not a real picture"].concat());
    let emf = art(0, 0x3D4, 0xF01A, &[&[0xCD; 16][..], &[0; 34], b"compressed metafile"].concat());
    let fdgg = art(0, 0, 0xF006, &[0; 16]);
    let store = art(0xF, 2, 0xF001, &[fbse(&png), fbse(&emf)].concat());
    rec(0x00EB, &art(0xF, 0, 0xF000, &[fdgg, store].concat()))
}

/// A one-sheet workbook: A1 = 1 and one picture object showing blip `pib`, anchored from the top
/// left of A8 to the top left of P20 (15 default columns wide), with "Print object" on or off.
fn workbook(pib: u32, print: bool) -> Vec<u8> {
    let bof = |dt: u16| rec(0x0809, &[&u16s(&[0x0600, dt, 0x0DBB, 0x07CC])[..], &[0; 8]].concat());
    let eof = rec(0x000A, &[]);
    let fsp = art(2, 75, 0xF00A, &[1025u32.to_le_bytes(), 0x0A00u32.to_le_bytes()].concat());
    let fopt = art(3, 1, 0xF00B, &[&u16s(&[0x4104])[..], &pib.to_le_bytes()].concat());
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 0, 0, 7, 0, 15, 0, 19, 0]));
    let data = art(0, 0, 0xF011, &[]);
    let drawing = rec(0x00EC, &art(0xF, 0, 0xF004, &[fsp, fopt, anchor, data].concat()));
    // FtCmo: a picture (ot 8), fPrint is 0x0010; then FtEnd.
    let obj = rec(0x005D, &[&u16s(&[0x15, 0x12, 8, 1, if print { 0x0010 } else { 0 }])[..], &[0; 12], &u16s(&[0, 0])].concat());
    let a1 = rec(0x0203, &[&u16s(&[0, 0, 0])[..], &1f64.to_le_bytes()].concat());
    let sheet = [bof(0x0010), a1, drawing, obj, eof.clone()].concat();
    let globals =
        |pos: u32| [bof(0x0005), rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 6, 0], b"Sheet1"].concat()), drawing_group(), eof.clone()].concat();
    let len = globals(0).len() as u32;
    compound_file(&[("Workbook", &[globals(len), sheet].concat())])
}

fn pages(bytes: &[u8]) -> Vec<String> {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "objects.xls", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    let list = s.execute("file.printPreview", json!({})).unwrap()["pageList"].as_array().unwrap().clone();
    list.iter().map(|p| p["range"].as_str().unwrap().to_string()).collect()
}

#[test]
fn xls_png_picture_reaching_column_p_prints_on_two_pages() {
    // The guard: a picture that is imported already counts.
    // The file has no FONT records, so the default font is Calibri 11: a default column prints
    // 48pt × 1.0743 = 51.57pt (47 dots = 5.64pt a character, not 5.25pt), and the 511.2pt Letter
    // width holds 511.2 / 51.57 = 9.9, so 9 columns a page (A:I, then J:P), not 10.
    let p = pages(&workbook(1, true));
    assert_eq!(p, ["A1:I20", "J1:P20"]);
}

#[test]
fn xls_unshown_picture_prints_the_pages_an_imported_picture_does() {
    // 9 printed default columns a Letter page, as above.
    let p = pages(&workbook(2, true));
    assert_eq!(p, ["A1:I20", "J1:P20"]);
}

#[test]
fn xls_unshown_picture_with_print_object_off_does_not_extend_the_print() {
    let p = pages(&workbook(2, false));
    assert_eq!(p, ["A1"]);
}
