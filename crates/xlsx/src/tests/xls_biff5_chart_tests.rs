//! Excel 5.0/95 (BIFF5) embedded charts: the chart substream follows an OBJ record (ot 5) whose
//! fixed header carries the anchor. BIFF5 differs from BIFF8 in the Series record (8 bytes), the
//! BRAI formulas (BIFF5 PtgRef3d/PtgArea3d: ixals, 8 reserved bytes, itabFirst, itabLast, 16-bit
//! rows with the relative flags in bits 14/15, 8-bit columns), SeriesText (no flags byte; 8-bit
//! text in the workbook code page) and Text (26 bytes). Every workbook is assembled here from those
//! layouts in a `Book` stream; no Excel-produced file is used.

use gridcraft_core::CellRef;
use gridcraft_model::ChartKind;

use super::xls_tests::compound;
use crate::read_xls;

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

/// The 8-byte BIFF5 BOF.
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

/// A Book stream: globals (CodePage 1252, one BoundSheet "Sheet1") and one worksheet with `records`.
fn book(records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, 6], b"Sheet1"].concat());
    let head = [bof(0x0005), rec(0x0042, &1252u16.to_le_bytes())].concat();
    let pos = (head.len() + boundsheet(0).len() + eof().len()) as u32;
    let globals = [head, boundsheet(pos), eof()].concat();
    let body = [bof(0x0010), cells(), records.to_vec(), eof()].concat();
    compound("Book", &[globals, body].concat())
}

/// Sheet1 cells: A2:C5 numbers.
fn cells() -> Vec<u8> {
    let mut v = vec![];
    for r in 1..5u16 {
        for c in 0..3u16 {
            v.extend(rec(0x0203, &cat(&[&u16s(&[r, c, 0]), &f64::from(r * 10 + c).to_le_bytes()])));
        }
    }
    v
}

/// A BIFF5 chart OBJ (ot 5): cObj, ot, id, grbit, the anchor (colL, dxL, rwT, dyT, colR, dxR,
/// rwB, dyB), cbMacro 0 and 6 reserved bytes, then the 26 chart-specific bytes Excel 95 writes.
fn chart_obj(id: u16, anchor: [u16; 8]) -> Vec<u8> {
    let mut p = [&1u32.to_le_bytes()[..], &u16s(&[5, id, 0x0614]), &u16s(&anchor), &[0; 8]].concat();
    p.extend_from_slice(&[0; 26]);
    rec(0x005D, &p)
}

/// BIFF5 PtgArea3d (absolute) through this workbook's sheet `itab`; `flags` is OR-ed into both
/// row fields (0xC000 = column and row relative).
fn area3d(itab: i16, r1: u16, r2: u16, c1: u8, c2: u8, flags: u16) -> Vec<u8> {
    cat(&[&[0x3B], &(-1i16).to_le_bytes(), &[0; 8], &itab.to_le_bytes(), &itab.to_le_bytes(), &u16s(&[r1 | flags, r2 | flags]), &[c1, c2]])
}

/// BIFF5 PtgRef3d (absolute) through this workbook's sheet `itab`.
fn ref3d(itab: i16, r: u16, c: u8) -> Vec<u8> {
    cat(&[&[0x3A], &(-1i16).to_le_bytes(), &[0; 8], &itab.to_le_bytes(), &itab.to_le_bytes(), &u16s(&[r]), &[c]])
}

/// BRAI: `id` (0 name, 1 values, 2 categories) referring to `rgce` (literal when empty).
fn brai(id: u8, rgce: &[u8]) -> Vec<u8> {
    let rt = if rgce.is_empty() { 1 } else { 2 };
    rec(0x1051, &cat(&[&[id, rt], &u16s(&[0, 0, rgce.len() as u16]), rgce]))
}

/// BIFF5 SeriesText: id, cch, then 8-bit characters (no flags byte).
fn series_text(s: &[u8]) -> Vec<u8> {
    rec(0x100D, &cat(&[&u16s(&[0]), &[s.len() as u8], s]))
}

/// A BIFF5 series (8-byte Series record) with name B1/C1.., values and categories A2:A5.
fn series(col: u8, flags: u16) -> Vec<u8> {
    cat(&[
        &rec(0x1003, &u16s(&[1, 1, 4, 4])),
        &begin(),
        &brai(0, &ref3d(0, 0, col)),
        &brai(1, &area3d(0, 1, 4, col, col, flags)),
        &brai(2, &area3d(0, 1, 4, 0, 0, flags)),
        &brai(3, &[]),
        &rec(0x1045, &u16s(&[0])),
        &end(),
    ])
}

/// A 26-byte BIFF5 Text block linked to `link` (1 chart title, 2 value axis, 3 category axis).
fn text(link: u16, s: &[u8]) -> Vec<u8> {
    cat(&[&rec(0x1025, &[0; 26]), &begin(), &rec(0x1027, &u16s(&[link, 0, 0])), &series_text(s), &end()])
}

/// A BIFF5 chart substream: the series, `extra` (titles), one axis parent with two axes, a
/// clustered column group and a legend.
fn chart(series: &[u8], extra: &[u8]) -> Vec<u8> {
    cat(&[
        &bof(0x0020),
        &rec(0x1002, &u32s(&[0, 0, 360 << 16, 216 << 16])),
        &begin(),
        series,
        extra,
        &rec(0x1041, &cat(&[&u16s(&[0]), &[0; 16]])),
        &begin(),
        &rec(0x101D, &cat(&[&u16s(&[0]), &[0; 16]])),
        &begin(),
        &end(),
        &rec(0x101D, &cat(&[&u16s(&[1]), &[0; 16]])),
        &begin(),
        &end(),
        &rec(0x1014, &cat(&[&[0; 16], &u16s(&[0, 0])])),
        &begin(),
        &rec(0x1017, &u16s(&[0, 150, 0])),
        &rec(0x1015, &cat(&[&u32s(&[1500, 3600, 1000, 300]), &[0, 1], &u16s(&[0x0001])])),
        &end(),
        &end(),
        &end(),
        &eof(),
    ])
}

fn two_series_chart() -> Vec<u8> {
    chart(&cat(&[&series(1, 0), &series(2, 0)]), &text(1, b"Quarterly sales"))
}

fn skipped(warnings: &[String]) -> bool {
    warnings.iter().any(|w| w.contains("skipped") || w.contains("can't be imported"))
}

#[test]
fn biff5_embedded_chart_opens_with_series_title_and_obj_anchor() {
    let file = book(&cat(&[&chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]), &two_series_chart()]));
    let (wb, report) = read_xls(&file).unwrap();
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.charts.len(), 1, "{:?}", report.warnings);
    let ch = &sheet.charts[0];
    assert_eq!(ch.kind, ChartKind::ColumnClustered);
    assert_eq!(ch.title.as_deref(), Some("Quarterly sales"));
    assert_eq!(ch.anchor.cell, CellRef::new(1, 4), "top-left cell is the OBJ's (rwT, colL)");
    assert!(ch.anchor.width > 0.0 && ch.anchor.height > 0.0, "{:?}", ch.anchor);
    assert_eq!(ch.series.len(), 2);
    assert_eq!(ch.series[0].name.as_deref(), Some("Sheet1!$B$1"));
    assert_eq!(ch.series[0].values, "Sheet1!$B$2:$B$5");
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$A$2:$A$5"));
    assert_eq!(ch.series[1].name.as_deref(), Some("Sheet1!$C$1"));
    assert_eq!(ch.series[1].values, "Sheet1!$C$2:$C$5");
    assert!(!skipped(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn biff5_relative_flags_in_row_fields_are_not_part_of_the_row() {
    let ch = chart(&series(1, 0xC000), &[]);
    let (wb, report) = read_xls(&book(&cat(&[&chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]), &ch]))).unwrap();
    let charts = &wb.sheets[0].charts;
    assert_eq!(charts.len(), 1, "{:?}", report.warnings);
    assert_eq!(charts[0].series[0].values.replace('$', ""), "Sheet1!B2:B5");
    assert_eq!(charts[0].series[0].categories.as_deref().map(|c| c.replace('$', "")).as_deref(), Some("Sheet1!A2:A5"));
}

#[test]
fn biff5_titles_are_decoded_in_the_workbook_code_page() {
    // 0xF1 is n-tilde and 0xB7 a middle dot in code page 1252.
    let extra = cat(&[&text(1, b"Ma\xF1ana \xB7 total"), &text(2, b"Voltage (nV)")]);
    let ch = chart(&series(1, 0), &extra);
    let (wb, report) = read_xls(&book(&cat(&[&chart_obj(1, [0, 0, 6, 0, 5, 0, 20, 0]), &ch]))).unwrap();
    let charts = &wb.sheets[0].charts;
    assert_eq!(charts.len(), 1, "{:?}", report.warnings);
    assert_eq!(charts[0].title.as_deref(), Some("Ma\u{F1}ana \u{B7} total"));
}

#[test]
fn several_biff5_charts_keep_their_order_and_places() {
    let recs = cat(&[
        &chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]),
        &two_series_chart(),
        &chart_obj(2, [0, 0, 20, 0, 6, 0, 35, 0]),
        &chart(&series(2, 0), &text(1, b"Second")),
    ]);
    let (wb, report) = read_xls(&book(&recs)).unwrap();
    let got: Vec<_> = wb.sheets[0].charts.iter().map(|c| (c.title.clone(), c.anchor.cell)).collect();
    assert_eq!(
        got,
        [(Some("Quarterly sales".to_string()), CellRef::new(1, 4)), (Some("Second".to_string()), CellRef::new(20, 0))],
        "{:?}",
        report.warnings
    );
}

#[test]
fn biff5_import_summary_no_longer_says_charts_are_not_imported() {
    let file = book(&cat(&[&chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]), &two_series_chart()]));
    let (_, report) = read_xls(&file).unwrap();
    let summary = report.warnings.iter().find(|w| w.contains("Excel 5.0/95")).expect("BIFF5 import summary");
    // Charts are listed among what was read, not among what is not imported.
    let (read, not_read) = summary.split_once("were read").expect("summary lists what was read");
    assert!(read.contains("charts") && !not_read.contains("harts"), "{summary}");
}

#[test]
fn truncated_or_garbled_biff5_chart_records_never_panic() {
    let full = two_series_chart();
    // Cut the substream at every record boundary and inside records.
    for cut in (8..full.len()).step_by(7) {
        let _ = read_xls(&book(&cat(&[&chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]), &full[..cut]])));
    }
    // A BRAI whose cce claims more bytes than the record holds; a short Series; an empty SeriesText.
    let bad =
        cat(&[&rec(0x1003, &u16s(&[1, 1])), &begin(), &rec(0x1051, &cat(&[&[1, 2], &u16s(&[0, 0, 200]), &[0x3B, 0xFF]])), &rec(0x100D, &[]), &end()]);
    let (wb, _) = read_xls(&book(&cat(&[&chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]), &chart(&bad, &[])]))).unwrap();
    // The cells survive whatever happens to the chart.
    assert!(wb.sheets[0].cell(CellRef::new(1, 1)).is_some());
    // An external-workbook reference (ixals >= 0) is left out, not a panic.
    let mut ext = area3d(0, 1, 4, 1, 1, 0);
    ext[1..3].copy_from_slice(&1i16.to_le_bytes());
    let ser = cat(&[&rec(0x1003, &u16s(&[1, 1, 4, 4])), &begin(), &brai(1, &ext), &end()]);
    let _ = read_xls(&book(&cat(&[&chart_obj(1, [4, 0, 1, 0, 10, 0, 15, 0]), &chart(&ser, &[])])));
}
