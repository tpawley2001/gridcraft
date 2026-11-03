//! `.xls` picture import: the BStore of the globals' MsoDrawingGroup ([MS-XLS] 2.4.170,
//! [MS-ODRAW] 2.2.20) and picture shapes in a sheet's MsoDrawing records (2.4.168) claimed by a
//! picture Obj record (2.4.181, `ot` 8).

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Sheet, Workbook};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{bof, cat, cells, column_chart, eof, obj, rec, u16s};
use super::xls_tests::compound;

const FORMAT_WARNING: &str = "Some pictures in .xls files use a format that isn't imported (only PNG and JPEG are).";
const DAMAGED_WARNING: &str = "The pictures of this .xls file are damaged and were not imported.";

/// Bytes starting with the PNG signature (the reader doesn't decode them).
fn png() -> Vec<u8> {
    cat(&[&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], b"not really a picture"])
}

fn jpeg() -> Vec<u8> {
    cat(&[&[0xFF, 0xD8, 0xFF, 0xE0], b"jpeg body", &[0xFF, 0xD9]])
}

/// An OfficeArt record: version and instance, type, then the body.
fn art(ver: u16, inst: u16, ty: u16, body: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(body.len() as u32).to_le_bytes(), body])
}

/// A blip record of type `ty` and instance `inst` with `uids` UIDs.
fn blip(ty: u16, inst: u16, uids: usize, file: &[u8]) -> Vec<u8> {
    art(0, inst, ty, &cat(&[&vec![0xAB; 16 * uids], &[0xFF], file]))
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
    art(2, 6, 0xF007, &cat(&[&head, blip]))
}

/// An MsoDrawingGroup holding an OfficeArtDggContainer with an FDGG and a BStore of `entries`.
fn drawing_group(entries: &[Vec<u8>]) -> Vec<u8> {
    let fdgg = art(0, 0, 0xF006, &[0; 16]);
    let store = art(0xF, entries.len() as u16, 0xF001, &entries.concat());
    rec(0x00EB, &art(0xF, 0, 0xF000, &cat(&[&fdgg, &store])))
}

/// FOPT properties: simple `(opid, op)` pairs, then complex ones `(opid, data)`.
fn fopt(simple: &[(u16, u32)], complex: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut table = vec![];
    for (id, op) in simple {
        table.extend(u16s(&[*id]));
        table.extend(op.to_le_bytes());
    }
    for (id, data) in complex {
        table.extend(u16s(&[*id | 0x8000]));
        table.extend((data.len() as u32).to_le_bytes());
    }
    for (_, data) in complex {
        table.extend(data);
    }
    art(3, (simple.len() + complex.len()) as u16, 0xF00B, &table)
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

/// An MsoDrawing holding one shape container: FSP, the FOPT, a client anchor from
/// (col 1 + 256/1024, row 2) to (col 3, row 6 + 128/256) and client data.
fn shape(fopt: &[u8]) -> Vec<u8> {
    let fsp = art(2, 75, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 256, 2, 0, 3, 0, 6, 128]));
    let data = art(0, 0, 0xF011, &[]);
    rec(0x00EC, &art(0xF, 0, 0xF004, &cat(&[&fsp, fopt, &anchor, &data])))
}

fn picture(pib: u32) -> Vec<u8> {
    cat(&[&shape(&fopt(&[(0x4104, pib)], &[])), &obj(8)])
}

/// A one-sheet workbook whose globals end with `extra`.
fn book(extra: &[u8], sheet_records: &[u8]) -> Vec<u8> {
    let sheet = cat(&[&bof(0x0010), &cells(), sheet_records, &eof()]);
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
            extra,
            &eof(),
        ])
    };
    let len = globals(0).len() as u32;
    compound("Workbook", &cat(&[&globals(len), &sheet]))
}

fn read(file: &[u8]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xls(file).unwrap();
    (wb, report.warnings)
}

fn png_store() -> Vec<u8> {
    drawing_group(&[fbse(&blip(0xF01E, 0x6E0, 1, &png()))])
}

/// The x of a column plus a fraction of its width, the y of a row plus a fraction of its height.
fn at(sheet: &Sheet, col: u32, dx: f64, row: u32, dy: f64) -> (f64, f64) {
    (sheet.col_left(col) + dx * f64::from(sheet.col_width(col)), sheet.row_top(row) + dy * f64::from(sheet.row_height(row)))
}

#[test]
fn xls_png_picture_reads_bytes_and_anchor() {
    let (wb, warnings) = read(&book(&png_store(), &picture(1)));
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.images.len(), 1, "{warnings:?}");
    let image = &sheet.images[0];
    assert_eq!(*image.data, png());
    assert_eq!(image.mime, "image/png");
    assert!(image.id > 0);
    assert!(!image.hidden);
    let (x1, y1) = at(sheet, 1, 0.25, 2, 0.0);
    let (x2, y2) = at(sheet, 3, 0.0, 6, 0.5);
    assert_eq!(image.anchor.cell, CellRef::new(2, 1));
    assert!((f64::from(image.anchor.dx) - (x1 - sheet.col_left(1))).abs() < 0.01, "{:?}", image.anchor);
    assert!(image.anchor.dy.abs() < 0.01);
    assert!((f64::from(image.anchor.width) - (x2 - x1)).abs() < 0.01, "{:?}", image.anchor);
    assert!((f64::from(image.anchor.height) - (y2 - y1)).abs() < 0.01, "{:?}", image.anchor);
    assert!(!warnings.iter().any(|w| w.contains("Shapes") || w.contains("pictures in .xls")), "{warnings:?}");
}

#[test]
fn xls_jpeg_with_two_uids_skips_the_second_uid() {
    let store = drawing_group(&[fbse(&blip(0xF01D, 0x46B, 2, &jpeg()))]);
    let (wb, warnings) = read(&book(&store, &picture(1)));
    let images = &wb.sheets[0].images;
    assert_eq!(images.len(), 1, "{warnings:?}");
    assert_eq!(*images[0].data, jpeg());
    assert_eq!(images[0].mime, "image/jpeg");
}

#[test]
fn xls_two_pictures_share_one_store_entry() {
    let (wb, warnings) = read(&book(&png_store(), &cat(&[&picture(1), &picture(1)])));
    let images = &wb.sheets[0].images;
    assert_eq!(images.len(), 2, "{warnings:?}");
    assert_eq!(images[0].data, images[1].data);
    assert_ne!(images[0].id, images[1].id);
}

#[test]
fn xls_picture_name_alt_text_and_hidden_flag() {
    let props = fopt(&[(0x4104, 1), (0x03BF, 0x0002_0002)], &[(0x0380, utf16z("Logo")), (0x0381, utf16z("Company logo"))]);
    let (wb, warnings) = read(&book(&png_store(), &cat(&[&shape(&props), &obj(8)])));
    let images = &wb.sheets[0].images;
    assert_eq!(images.len(), 1, "{warnings:?}");
    assert_eq!(images[0].name.as_deref(), Some("Logo"));
    assert_eq!(images[0].alt, "Company logo");
    assert!(images[0].hidden);
    assert_eq!(*images[0].data, png());
}

fn picture_warnings(warnings: &[String]) -> usize {
    warnings.iter().filter(|w| w.contains("picture") && !w.starts_with("Imported from")).count()
}

#[test]
fn xls_picture_with_unknown_pib_is_skipped_with_one_warning() {
    let (wb, warnings) = read(&book(&png_store(), &cat(&[&picture(5), &picture(7)])));
    let sheet = &wb.sheets[0];
    assert!(sheet.images.is_empty());
    assert_eq!(warnings.iter().filter(|w| *w == FORMAT_WARNING).count(), 1, "{warnings:?}");
    assert_eq!(picture_warnings(&warnings), 1, "{warnings:?}");
    assert_eq!(sheet.cell(CellRef::new(1, 1)).map(|c| c.value.clone()), Some(Value::number(11.0)));
}

#[test]
fn xls_dib_picture_is_skipped_with_the_format_warning() {
    let store = drawing_group(&[fbse(&blip(0xF01F, 0x7A8, 1, &[0x28, 0, 0, 0]))]);
    let (wb, warnings) = read(&book(&store, &cat(&[&picture(1), &picture(1)])));
    assert!(wb.sheets[0].images.is_empty());
    assert_eq!(warnings.iter().filter(|w| *w == FORMAT_WARNING).count(), 1, "{warnings:?}");
    assert_eq!(picture_warnings(&warnings), 1, "{warnings:?}");
}

#[test]
fn xls_truncated_drawing_group_opens_without_pictures() {
    let full = png_store();
    // Cut the record mid-container, fixing up its BIFF length.
    let body = full[4..full.len() - 10].to_vec();
    let (wb, warnings) = read(&book(&rec(0x00EB, &body), &picture(1)));
    assert!(wb.sheets[0].images.is_empty());
    assert_eq!(warnings.iter().filter(|w| *w == DAMAGED_WARNING).count(), 1, "{warnings:?}");
    assert_eq!(picture_warnings(&warnings), 1, "{warnings:?}");
    assert_eq!(wb.sheets[0].cell(CellRef::new(1, 1)).map(|c| c.value.clone()), Some(Value::number(11.0)));
}

#[test]
fn xls_chart_next_to_a_picture() {
    let chart = cat(&[&shape(&fopt(&[], &[])), &obj(5), &column_chart()]);
    let (wb, warnings) = read(&book(&png_store(), &cat(&[&chart, &picture(1)])));
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.charts.len(), 1, "{warnings:?}");
    assert_eq!(sheet.images.len(), 1, "{warnings:?}");
    assert_ne!(sheet.charts[0].id, sheet.images[0].id);
}

#[test]
fn xls_picture_saves_to_xlsx() {
    let (wb, _) = read(&book(&png_store(), &picture(1)));
    let before = wb.sheets[0].images[0].clone();
    let bytes = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    let images = &back.sheets[0].images;
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].data, before.data);
    assert_eq!(images[0].mime, "image/png");
    assert_eq!(images[0].anchor.cell, before.anchor.cell);
    assert!((images[0].anchor.dx - before.anchor.dx).abs() < 1.0, "{:?} {:?}", images[0].anchor, before.anchor);
    assert!((images[0].anchor.width - before.anchor.width).abs() < 1.0, "{:?} {:?}", images[0].anchor, before.anchor);
    assert!((images[0].anchor.height - before.anchor.height).abs() < 1.0, "{:?} {:?}", images[0].anchor, before.anchor);
}
