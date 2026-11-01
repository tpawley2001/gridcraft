//! XLSB worksheet layout records (column widths, row heights, default sizes, views, panes and
//! tab colour), assembled byte by byte from the public MS-XLSB record layouts.
use gridcraft_core::CellRef;
use gridcraft_model::{Color, LineInfo, Sheet};

use super::{make_zip, minimal};
use crate::tables::{col_width_to_px, pt_to_px};
use crate::{ReadReport, read_xlsb, read_xlsx};

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
    data.extend(wide("Layout"));
    out.extend(record(156, &data));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}
/// BrtRowHdr: rw, ixfe, miyRw, flags, fPhShow byte and no column spans.
fn row(r: u32, twips: u16, flags: u16) -> Vec<u8> {
    let mut out = r.to_le_bytes().to_vec();
    out.extend(0u32.to_le_bytes());
    out.extend(twips.to_le_bytes());
    out.extend(flags.to_le_bytes());
    out.push(0);
    out.extend(0u32.to_le_bytes());
    record(0, &out)
}
/// BrtColInfo: colFirst, colLast, coldx, ixfe, flags.
fn col_info(first: u32, last: u32, width: u32, flags: u16) -> Vec<u8> {
    let mut out = first.to_le_bytes().to_vec();
    out.extend(last.to_le_bytes());
    out.extend(width.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(flags.to_le_bytes());
    out
}
fn col_infos(infos: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(390, &[]);
    for info in infos {
        out.extend(record(60, info));
    }
    out.extend(record(391, &[]));
    out
}
/// BrtBeginWsView with the given flags and wScale, for workbook view `book_view`, and an optional BrtPane.
fn view(flags: u16, zoom: u16, book_view: u32, pane: Option<Vec<u8>>) -> Vec<u8> {
    let mut data = flags.to_le_bytes().to_vec();
    data.extend([0; 16]);
    data.extend(zoom.to_le_bytes());
    data.extend([0; 6]);
    data.extend(book_view.to_le_bytes());
    let mut out = record(137, &data);
    if let Some(p) = pane {
        out.extend(record(151, &p));
    }
    out.extend(record(138, &[]));
    out
}
fn views(list: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(133, &[]);
    for v in list {
        out.extend(v);
    }
    out.extend(record(134, &[]));
    out
}
fn pane(x: f64, y: f64, top: u32, left: u32, flags: u8) -> Vec<u8> {
    let mut out = x.to_le_bytes().to_vec();
    out.extend(y.to_le_bytes());
    out.extend(top.to_le_bytes());
    out.extend(left.to_le_bytes());
    out.extend(3u32.to_le_bytes());
    out.push(flags);
    out
}
/// BrtWsFmtInfo: dxGCol, cchDefColWidth, miyDefRwHeight, flags and outline levels.
fn fmt_info(dx: u32, chars: u16, twips: u16) -> Vec<u8> {
    let mut out = dx.to_le_bytes().to_vec();
    out.extend(chars.to_le_bytes());
    out.extend(twips.to_le_bytes());
    out.extend([0; 4]);
    record(485, &out)
}
/// BrtWsProp with a BrtColor tab colour of the given type, index and RGB.
fn ws_prop(kind: u8, index: u8, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = vec![0xc0, 0x04, 0x00];
    out.extend([(kind << 1) | 1, index, 0, 0]);
    out.extend(rgb);
    out.push(0xff);
    out.extend(u32::MAX.to_le_bytes());
    out.extend(u32::MAX.to_le_bytes());
    out.extend(wide(""));
    record(147, &out)
}
/// A worksheet with `head` before the sheet data, `data` inside it.
fn sheet(head: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(head);
    out.extend(record(145, &[]));
    out.extend(data);
    out.extend(record(146, &[]));
    out.extend(record(130, &[]));
    out
}
fn open(head: &[u8], data: &[u8]) -> (Sheet, ReadReport) {
    let bytes = make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book()),
        ("xl/_rels/workbook.bin.rels", RELS.as_bytes()),
        ("xl/worksheets/sheet1.bin", &sheet(head, data)),
    ]);
    let (wb, report) = read_xlsb(&bytes).expect("xlsb opens");
    (wb.sheets[0].as_ref().clone(), report)
}

#[test]
fn xlsb_column_widths_hidden_and_outline() {
    let (s, _) = open(&col_infos(&[col_info(1, 3, 20 * 256, 0x0002), col_info(5, 5, 9 * 256, 0x0001), col_info(6, 6, 9 * 256, 0x0200)]), &[]);
    let wide = Some(col_width_to_px(20.0));
    for c in 1..=3 {
        assert_eq!(s.cols[&c].size, wide, "column {c}");
        assert!(!s.cols[&c].hidden);
    }
    assert!(!s.cols.contains_key(&0) && !s.cols.contains_key(&4));
    assert!(s.cols[&5].hidden);
    assert_eq!(s.cols[&6].outline, 2);
    assert!(!s.cols[&6].hidden);
}

#[test]
fn xlsb_row_heights_hidden_and_outline() {
    let data = [row(2, 600, 0x2000), row(4, 300, 0x1000), row(5, 300, 0x0100), row(6, 300, 0)].concat();
    let (s, _) = open(&[], &data);
    let r = s.rows[&2];
    assert_eq!(r.size, Some(pt_to_px(30.0)));
    assert!(r.custom && !r.hidden);
    assert!(s.rows[&4].hidden);
    assert_eq!(s.rows[&5].outline, 1);
    assert!(!s.rows[&5].hidden);
    // A 15 pt automatic height equals the default row height: nothing to keep.
    assert!(!s.rows.contains_key(&6));
}

#[test]
fn xlsb_default_row_height_and_column_width() {
    let (s, _) = open(&fmt_info(12 * 256, 8, 360), &[]);
    assert_eq!(s.default_row_height, pt_to_px(18.0));
    assert_eq!(s.default_col_width, col_width_to_px(12.0));
    // dxGCol unset: the base width in characters gives the default.
    let (s, _) = open(&fmt_info(u32::MAX, 10, 300), &[]);
    let (x, _) = read_xlsx(&minimal(r#"<sheetFormatPr baseColWidth="10" defaultRowHeight="15"/><sheetData/>"#, &[], "", "")).expect("xlsx opens");
    assert_eq!(s.default_col_width, x.sheets[0].default_col_width);
    assert_eq!(s.default_row_height, x.sheets[0].default_row_height);
}

#[test]
fn xlsb_frozen_pane_matches_xlsx_reader() {
    let (s, _) = open(&views(&[view(0x001c, 0, 0, Some(pane(1.0, 2.0, 2, 1, 0x01)))]), &[]);
    let body = r#"<sheetViews><sheetView workbookViewId="0"><pane xSplit="1" ySplit="2" topLeftCell="B3" activePane="bottomRight" state="frozen"/></sheetView></sheetViews><sheetData/>"#;
    let (x, _) = read_xlsx(&minimal(body, &[], "", "")).expect("xlsx opens");
    assert_eq!(s.freeze, Some((2, 1)));
    assert_eq!(s.freeze, x.sheets[0].freeze);
    assert_eq!(s.view_top_left, CellRef::new(2, 1));
    assert_eq!(s.view_top_left, x.sheets[0].view_top_left);
    // A split (unfrozen) pane is not a freeze.
    let (s, _) = open(&views(&[view(0x001c, 0, 0, Some(pane(1500.0, 900.0, 2, 1, 0)))]), &[]);
    assert_eq!(s.freeze, None);
}

#[test]
fn xlsb_sheet_view_options() {
    // fDspRwCol | fDspZeros | fRightToLeft | fDspGuts; gridlines off.
    let first = view(0x0008 | 0x0010 | 0x0020 | 0x0100, 85, 0, None);
    let second = view(0x0002 | 0x0004, 200, 1, Some(pane(3.0, 3.0, 3, 3, 0x01)));
    let (s, _) = open(&views(&[first, second]), &[]);
    assert!(!s.show_gridlines);
    assert!(s.show_headings && s.show_zeros && s.show_outline_symbols && s.right_to_left);
    assert!(!s.show_formulas);
    assert_eq!(s.zoom, 85);
    assert_eq!(s.freeze, None, "only the first workbook view's pane applies");
    let (s, _) = open(&views(&[view(0x0002, 0, 0, None)]), &[]);
    assert!(s.show_formulas && !s.show_gridlines);
    assert_eq!(s.zoom, 100);
    let (s, _) = open(&views(&[view(0, 5000, 0, None)]), &[]);
    assert_eq!(s.zoom, 400);
}

#[test]
fn xlsb_tab_colour() {
    let (s, _) = open(&ws_prop(2, 0, [0xff, 0, 0]), &[]);
    assert_eq!(s.tab_color, Some(Color::Rgb(0xff0000)));
    let (s, _) = open(&ws_prop(1, 10, [0, 0, 0]), &[]);
    assert_eq!(s.tab_color, Some(Color::Rgb(gridcraft_core::color::INDEXED[10])));
    let (s, _) = open(&ws_prop(3, 4, [0, 0, 0]), &[]);
    assert_eq!(s.tab_color, None);
}

#[test]
fn xlsb_truncated_column_info_is_skipped_with_a_warning() {
    let mut truncated = col_info(0, 0, 30 * 256, 0x0002);
    truncated.truncate(10);
    let (s, report) = open(&col_infos(&[col_info(1, 1, 20 * 256, 0x0002), truncated.clone(), truncated, col_info(2, 2, 0, 0x0001)]), &[]);
    assert!(!s.cols.contains_key(&0));
    assert_eq!(s.cols[&1].size, Some(col_width_to_px(20.0)));
    assert!(s.cols[&2].hidden);
    let warnings = report.warnings.iter().filter(|w| w.contains("column information")).count();
    assert_eq!(warnings, 1, "{:?}", report.warnings);
    // Truncated view, pane, format and property records don't stop the import either.
    let head =
        [record(147, &[0; 2]), record(485, &[0; 3]), views(&[[record(137, &[0; 4]), record(151, &[1; 5]), record(138, &[])].concat()])].concat();
    let (s, report) = open(&head, &[]);
    assert_eq!((s.freeze, s.tab_color, s.zoom), (None, None, 100));
    assert!(s.show_gridlines);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("Malformed XLSB")).count(), 3, "{:?}", report.warnings);
}

#[test]
fn xlsb_whole_sheet_column_range_is_clamped() {
    let (s, _) = open(&col_infos(&[col_info(0, u32::MAX, 3 * 256, 0x0003)]), &[]);
    assert_eq!(s.cols.len(), gridcraft_core::MAX_COLS as usize);
    assert!(s.cols.values().all(|c| c.hidden));
    // Repeated whole-sheet records stop at the expansion budget instead of looping on.
    let many: Vec<_> = (0..100).map(|_| col_info(0, u32::MAX, 3 * 256, 0x0002)).collect();
    let (s, report) = open(&col_infos(&many), &[]);
    assert_eq!(s.cols.len(), gridcraft_core::MAX_COLS as usize);
    assert!(s.cols.values().all(|c| *c != LineInfo::default()));
    assert!(report.warnings.iter().any(|w| w.contains("column information")), "{:?}", report.warnings);
    // A first column past the sheet is skipped.
    let (s, _) = open(&col_infos(&[col_info(u32::MAX, u32::MAX, 3 * 256, 0x0002)]), &[]);
    assert!(s.cols.is_empty());
}
