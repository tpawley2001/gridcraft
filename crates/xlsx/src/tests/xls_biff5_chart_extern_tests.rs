//! Excel 5.0/95 (BIFF5) chart references through the chart substream's own EXTERNSHEET list: a
//! positive `ixals` in a chart's BRAI formula indexes the EXTERNSHEET records inside that chart
//! substream, not the workbook's. Every workbook is assembled here byte by byte; no
//! Excel-produced file is used.

use gridcraft_core::CellRef;

use super::xls_tests::compound;
use crate::{read_xls, read_xlsx, write_xlsx};

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &u16s(&[0x0500, dt, 0x0DBB, 0x07CC]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

fn begin() -> Vec<u8> {
    rec(0x1033, &[])
}

fn end() -> Vec<u8> {
    rec(0x1034, &[])
}

/// ExternCount then one ExternSheet record per encoded target.
fn externs(targets: &[&[u8]]) -> Vec<u8> {
    let mut v = rec(0x0016, &u16s(&[targets.len() as u16]));
    for t in targets {
        v.extend(rec(0x0017, &cat(&[&[t.len() as u8], t])));
    }
    v
}

/// A Book stream: globals (CodePage 1252, `globals_extra`, BoundSheets "Data" and "Chart") and
/// the two worksheets; "Data" holds A2:C5 numbers and "Chart" holds `records`.
fn book(globals_extra: &[u8], records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32, name: &[u8]| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, name.len() as u8], name].concat());
    let head = [bof(0x0005), rec(0x0042, &1252u16.to_le_bytes()), globals_extra.to_vec()].concat();
    let globals_len = head.len() + boundsheet(0, b"Data").len() + boundsheet(0, b"Chart").len() + eof().len();
    let data = [bof(0x0010), cells(), eof()].concat();
    let chart_sheet = [bof(0x0010), records.to_vec(), eof()].concat();
    let globals = [head, boundsheet(globals_len as u32, b"Data"), boundsheet((globals_len + data.len()) as u32, b"Chart"), eof()].concat();
    compound("Book", &[globals, data, chart_sheet].concat())
}

/// "Data" cells: A2:C5 numbers.
fn cells() -> Vec<u8> {
    let mut v = vec![];
    for r in 1..5u16 {
        for c in 0..3u16 {
            v.extend(rec(0x0203, &cat(&[&u16s(&[r, c, 0]), &f64::from(r * 10 + c).to_le_bytes()])));
        }
    }
    v
}

/// A BIFF5 chart OBJ (ot 5) with its anchor (colL, dxL, rwT, dyT, colR, dxR, rwB, dyB).
fn chart_obj(anchor: [u16; 8]) -> Vec<u8> {
    let mut p = [&1u32.to_le_bytes()[..], &u16s(&[5, 1, 0x0614]), &u16s(&anchor), &[0; 8]].concat();
    p.extend_from_slice(&[0; 26]);
    rec(0x005D, &p)
}

/// BIFF5 PtgArea3d through EXTERNSHEET entry `ixals` (sheet index `itab` for this workbook).
fn area3d(ixals: i16, itab: i16, r1: u16, r2: u16, c1: u8, c2: u8) -> Vec<u8> {
    cat(&[&[0x3B], &ixals.to_le_bytes(), &[0; 8], &itab.to_le_bytes(), &itab.to_le_bytes(), &u16s(&[r1, r2]), &[c1, c2]])
}

fn brai(id: u8, rgce: &[u8]) -> Vec<u8> {
    rec(0x1051, &cat(&[&[id, 2], &u16s(&[0, 0, rgce.len() as u16]), rgce]))
}

/// A one-series chart substream: `head` (its own EXTERNSHEET list) and `series` records.
fn chart(head: &[u8], series: &[u8]) -> Vec<u8> {
    cat(&[
        &bof(0x0020),
        head,
        &rec(0x1002, &u32s(&[0, 0, 360 << 16, 216 << 16])),
        &begin(),
        series,
        &rec(0x1041, &cat(&[&u16s(&[0]), &[0; 16]])),
        &begin(),
        &rec(0x1014, &cat(&[&[0; 16], &u16s(&[0, 0])])),
        &begin(),
        &rec(0x1017, &u16s(&[0, 150, 0])),
        &end(),
        &end(),
        &end(),
        &eof(),
    ])
}

/// The Series record (12 bytes, as Excel 95 writes it) with values and categories refs.
fn series(ixals: i16) -> Vec<u8> {
    cat(&[
        &rec(0x1003, &u16s(&[1, 1, 4, 4, 1, 0])),
        &begin(),
        &brai(1, &area3d(ixals, 0, 1, 4, 1, 1)),
        &brai(2, &area3d(ixals, 0, 1, 4, 0, 0)),
        &end(),
    ])
}

/// An encoded file path to `[OLD.XLS]<sheet>`, as an ExternSheet target.
fn encoded(sheet: &[u8]) -> Vec<u8> {
    cat(&[b"\x01\x02XLS\x03R&A\x03[OLD.XLS]", sheet])
}

#[test]
fn biff5_chart_resolves_its_own_externsheet_self_reference_to_the_local_sheet() {
    // The workbook's own list names "Chart" first; the chart's own list names `[OLD.XLS]Data`.
    let globals = externs(&[b"\x03Chart", b"\x03Data"]);
    let ch = chart(&externs(&[&encoded(b"data"), b"\x03Chart"]), &series(1));
    let (wb, report) = read_xls(&book(&globals, &cat(&[&chart_obj([4, 0, 1, 0, 10, 0, 15, 0]), &ch]))).unwrap();
    let charts = &wb.sheets[1].charts;
    assert_eq!(charts.len(), 1, "{:?}", report.warnings);
    assert_eq!(charts[0].series[0].values, "Data!$B$2:$B$5");
    assert_eq!(charts[0].series[0].categories.as_deref(), Some("Data!$A$2:$A$5"));
    assert_eq!(charts[0].anchor.cell, CellRef::new(1, 4));
    assert!(!report.warnings.iter().any(|w| w.contains("can't be imported")), "{:?}", report.warnings);

    // A plain sheet entry of the chart's list (#2 there, "Chart") is used, never the workbook's #2.
    let ch = chart(&externs(&[&encoded(b"data"), b"\x03Chart"]), &series(2));
    let (wb, _) = read_xls(&book(&globals, &cat(&[&chart_obj([4, 0, 1, 0, 10, 0, 15, 0]), &ch]))).unwrap();
    assert_eq!(wb.sheets[1].charts[0].series[0].values, "Chart!$B$2:$B$5");
}

#[test]
fn biff5_chart_reference_to_another_workbook_is_left_out() {
    // `[OLD.XLS]Elsewhere` names no sheet of this workbook: an external link, left out.
    let ch = chart(&externs(&[&encoded(b"Elsewhere")]), &series(1));
    let (wb, report) = read_xls(&book(&[], &cat(&[&chart_obj([4, 0, 1, 0, 10, 0, 15, 0]), &ch]))).unwrap();
    assert!(wb.sheets[1].charts.iter().all(|c| c.series.iter().all(|s| !s.values.contains("Elsewhere"))));
    assert!(report.warnings.iter().any(|w| w.contains("can't be imported")), "{:?}", report.warnings);
    // Without its own list, a chart's ixals > 0 does not fall back to the workbook's list.
    let ch = chart(&[], &series(1));
    let (wb, report) = read_xls(&book(&externs(&[b"\x03Data"]), &cat(&[&chart_obj([4, 0, 1, 0, 10, 0, 15, 0]), &ch]))).unwrap();
    assert!(wb.sheets[1].charts.is_empty(), "{:?}", report.warnings);
    assert!(wb.sheets[0].cell(CellRef::new(1, 1)).is_some());
}

#[test]
fn biff5_chart_without_series_opens_empty_at_its_anchor() {
    let ch = chart(&[], &[]);
    let (wb, report) = read_xls(&book(&[], &cat(&[&chart_obj([2, 0, 3, 0, 8, 0, 12, 0]), &ch]))).unwrap();
    let charts = &wb.sheets[1].charts;
    assert_eq!(charts.len(), 1, "{:?}", report.warnings);
    assert!(charts[0].series.is_empty());
    assert_eq!(charts[0].anchor.cell, CellRef::new(3, 2));
    // It saves as .xlsx and opens again.
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[1].charts.len(), 1);
}
