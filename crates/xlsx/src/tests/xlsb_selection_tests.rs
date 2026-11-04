//! XLSB sheet selections (BrtSel): each sheet opens on its saved active cell. The files are
//! assembled byte by byte from the public MS-XLSB record layouts.
use gridcraft_core::CellRef;
use gridcraft_model::Sheet;

use super::{make_zip, xlsb_layout_tests::sheet as layout_sheet};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

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
    data.extend(wide("Sheet1"));
    out.extend(record(156, &data));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}

/// BrtBeginWsView for workbook view 0.
fn view() -> Vec<u8> {
    let mut data = 0x001du16.to_le_bytes().to_vec();
    data.extend([0; 16]);
    data.extend(100u16.to_le_bytes());
    data.extend([0; 6]);
    data.extend(0u32.to_le_bytes());
    record(137, &data)
}

/// BrtEndWsView.
fn end_view() -> Vec<u8> {
    record(138, &[])
}

/// BrtPane: xSplit, ySplit, topLeft row and column, pnnAct, flags (fFrozen).
fn pane(x: f64, y: f64, top: u32, left: u32, active: u32, flags: u8) -> Vec<u8> {
    let mut out = x.to_le_bytes().to_vec();
    out.extend(y.to_le_bytes());
    out.extend(top.to_le_bytes());
    out.extend(left.to_le_bytes());
    out.extend(active.to_le_bytes());
    out.push(flags);
    record(151, &out)
}

/// BrtSel: pnn, rwAct, colAct, dwRfxAct, cRfx and one UncheckedRfX per range.
fn sel(pnn: u32, row: u32, col: u32) -> Vec<u8> {
    let mut out = pnn.to_le_bytes().to_vec();
    out.extend(row.to_le_bytes());
    out.extend(col.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(row.to_le_bytes());
    out.extend(row.to_le_bytes());
    out.extend(col.to_le_bytes());
    out.extend(col.to_le_bytes());
    record(152, &out)
}

fn open(head: &[u8]) -> (Sheet, ReadReport) {
    let bytes = make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book()),
        ("xl/_rels/workbook.bin.rels", RELS.as_bytes()),
        ("xl/worksheets/sheet1.bin", &layout_sheet(head, &[])),
    ]);
    let (wb, report) = read_xlsb(&bytes).expect("xlsb opens");
    (wb.sheets[0].as_ref().clone(), report)
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).expect("cell reference")
}

/// Saves the sheet as XLSX and reads it back: the view survives the round trip.
fn roundtrip(s: &Sheet) -> Sheet {
    let mut wb = gridcraft_model::Workbook::new();
    wb.sheets.clear();
    let mut sheet = s.clone();
    sheet.name = "Data".into();
    wb.sheets.push(Box::new(sheet).into());
    let (back, _) = read_xlsx(&write_xlsx(&wb).expect("xlsx saves")).expect("xlsx opens");
    back.sheets.first().map(|s| s.as_ref().clone()).expect("one sheet")
}

#[test]
fn xlsb_selection_sets_the_active_cell() {
    let (s, _) = open(&[view(), sel(3, 4, 2), end_view()].concat());
    assert_eq!(s.view_active, at("C5"));
    assert_eq!(roundtrip(&s).view_active, at("C5"));
}

#[test]
fn xlsb_selection_of_the_active_pane_wins() {
    let (s, _) = open(&[view(), pane(0.0, 1.0, 1, 0, 2, 0x01), sel(3, 0, 0), sel(2, 9, 3), end_view()].concat());
    assert_eq!(s.freeze, Some((1, 0)));
    assert_eq!(s.view_active, at("D10"));
    assert_eq!(roundtrip(&s).view_active, at("D10"));
}

#[test]
fn xlsb_selection_outside_the_sheet_is_ignored() {
    let (s, _) = open(&[view(), sel(3, 2_000_000, 0), end_view()].concat());
    assert_eq!(s.view_active, at("A1"));
    assert_eq!(roundtrip(&s).view_active, at("A1"));
}

#[test]
fn xlsb_truncated_selection_is_skipped_with_one_warning() {
    // A BrtSel whose payload stops after six bytes: pnn, rwAct and half of colAct.
    let short = record(152, &[3, 0, 0, 0, 4, 0]);
    let (s, report) = open(&[view(), short.clone(), short, end_view()].concat());
    assert_eq!(s.view_active, at("A1"));
    assert_eq!(report.warnings.iter().filter(|w| w.contains("Malformed XLSB selection")).count(), 1, "{:?}", report.warnings);
    assert_eq!(roundtrip(&s).view_active, at("A1"));
}

#[test]
fn xlsb_selection_in_a_later_view_is_ignored() {
    let (s, _) = open(&[view(), end_view(), view(), sel(3, 4, 2), end_view()].concat());
    assert_eq!(s.view_active, at("A1"));
    assert_eq!(roundtrip(&s).view_active, at("A1"));
}
