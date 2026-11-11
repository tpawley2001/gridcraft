//! XLSB sheet view mode: BrtBeginWsView's xlView (Normal, Page Break Preview, Page Layout), assembled
//! byte by byte from the public MS-XLSB record layout.
use gridcraft_model::{Sheet, SheetViewMode};

use super::make_zip;
use crate::{read_xlsb, read_xlsx, write_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/></Types>"#;

fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}
fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    out.extend(record(143, &[]));
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend(1u32.to_le_bytes());
    data.extend(wide("one"));
    data.extend(wide("View"));
    out.extend(record(156, &data));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}
/// BrtBeginWsView (30 bytes): flags, xlView, rwTop, colLeft, icvHdr, 3 reserved, wScale,
/// wScaleNormal, wScaleSLV, wScalePLV, iWbkView; then BrtEndWsView.
fn view(xl_view: u32, zoom: u16, book_view: u32) -> Vec<u8> {
    let mut data = 0x03dcu16.to_le_bytes().to_vec();
    data.extend(xl_view.to_le_bytes());
    data.extend(0u32.to_le_bytes());
    data.extend(0u32.to_le_bytes());
    data.extend([0x40, 0, 0, 0]);
    data.extend(zoom.to_le_bytes());
    data.extend(100u16.to_le_bytes());
    data.extend(0u16.to_le_bytes());
    data.extend(100u16.to_le_bytes());
    data.extend(book_view.to_le_bytes());
    assert_eq!(data.len(), 30);
    let mut out = record(137, &data);
    out.extend(record(138, &[]));
    out
}
fn open(views: &[Vec<u8>]) -> Sheet {
    let mut head = record(133, &[]);
    for v in views {
        head.extend(v);
    }
    head.extend(record(134, &[]));
    let mut sheet = record(129, &[]);
    sheet.extend(head);
    sheet.extend(record(145, &[]));
    sheet.extend(record(146, &[]));
    sheet.extend(record(130, &[]));
    let bytes = make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book()),
        ("xl/_rels/workbook.bin.rels", RELS.as_bytes()),
        ("xl/worksheets/sheet1.bin", &sheet),
    ]);
    let (wb, _) = read_xlsb(&bytes).expect("xlsb opens");
    wb.sheets[0].as_ref().clone()
}

#[test]
fn xlsb_normal_view_and_zoom_still_read() {
    let s = open(&[view(0, 100, 0)]);
    assert_eq!(s.view, SheetViewMode::Normal);
    assert_eq!(s.zoom, 100);
    assert!(s.show_gridlines && s.show_headings && s.show_zeros);
}

#[test]
fn xlsb_page_break_preview_view() {
    let s = open(&[view(1, 60, 0)]);
    assert_eq!(s.view, SheetViewMode::PageBreakPreview);
    assert_eq!(s.zoom, 60);
}

#[test]
fn xlsb_page_layout_view() {
    let s = open(&[view(2, 100, 0)]);
    assert_eq!(s.view, SheetViewMode::PageLayout);
    assert_eq!(s.zoom, 100);
}

#[test]
fn xlsb_unknown_view_reads_as_normal() {
    let s = open(&[view(7, 100, 0)]);
    assert_eq!(s.view, SheetViewMode::Normal);
    assert_eq!(s.zoom, 100);
}

#[test]
fn xlsb_only_the_first_workbook_view_applies() {
    let s = open(&[view(0, 100, 0), view(2, 100, 1)]);
    assert_eq!(s.view, SheetViewMode::Normal);
}

#[test]
fn xlsb_page_layout_view_kept_on_save_as_xlsx() {
    let s = open(&[view(2, 100, 0)]);
    let mut wb = gridcraft_model::Workbook::new();
    wb.sheets[0] = std::sync::Arc::new(s);
    let bytes = write_xlsx(&wb).expect("xlsx saves");
    let (back, _) = read_xlsx(&bytes).expect("xlsx reopens");
    assert_eq!(back.sheets[0].view, SheetViewMode::PageLayout);
    assert_eq!(back.sheets[0].zoom, 100);
}
