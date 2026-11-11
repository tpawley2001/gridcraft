//! `.xls` sheet zoom (SCL) and view (WINDOW2 fSLV, PLV fPageLayoutView): each sheet opens at its
//! saved zoom and in its saved view, and keeps both through Save As .xlsx. The files are assembled
//! from the [MS-CFB] and [MS-XLS] structures.
use gridcraft_model::{SheetViewMode, Workbook};

use super::xls_tests::{cat, one_sheet_with, rec, u16s};
use crate::{read_xls, read_xlsx, write_xlsx};

/// WINDOW2 ([MS-XLS] 2.4.346), 18 bytes: grbit, rwTop, colLeft, icvHdr, reserved, wScaleSLV,
/// wScaleNormal, reserved.
fn window2(grbit: u16) -> Vec<u8> {
    rec(0x023E, &u16s(&[grbit, 0, 0, 64, 0, 0, 0, 0, 0]))
}

/// The grbit Excel writes for a plain selected sheet (gridlines, headings, zeros, outline symbols ...).
const PLAIN: u16 = 0x06B6;
/// WINDOW2 fSLV: Page Break Preview.
const F_SLV: u16 = 0x0800;

/// SCL ([MS-XLS] 2.4.247): nscl / dscl.
fn scl(n: u16, d: u16) -> Vec<u8> {
    rec(0x00A0, &u16s(&[n, d]))
}

/// PLV ([MS-XLS] 2.4.200): FrtHeader (rt, grbitFrt, 8 reserved bytes), wScalePLV, flags (bit 0
/// fPageLayoutView).
fn plv(page_layout: bool) -> Vec<u8> {
    rec(0x088B, &u16s(&[0x088B, 0, 0, 0, 0, 0, 100, u16::from(page_layout)]))
}

fn open(records: &[u8]) -> Workbook {
    read_xls(&one_sheet_with(records)).expect("xls opens").0
}

fn zoom_view(wb: &Workbook) -> (u16, SheetViewMode) {
    (wb.sheets[0].zoom, wb.sheets[0].view)
}

#[test]
fn no_scl_opens_at_100_percent_in_normal_view() {
    assert_eq!(zoom_view(&open(&window2(PLAIN))), (100, SheetViewMode::Normal));
}

#[test]
fn scl_sets_the_sheet_zoom() {
    for (n, d, want) in [(3, 4, 75), (13, 20, 65), (115, 100, 115), (2, 1, 200)] {
        let wb = open(&cat(&[&window2(PLAIN), &scl(n, d)]));
        assert_eq!(zoom_view(&wb), (want, SheetViewMode::Normal), "SCL {n}/{d}");
    }
    // Record order does not matter.
    assert_eq!(zoom_view(&open(&cat(&[&scl(3, 4), &window2(PLAIN)]))).0, 75);
}

#[test]
fn bad_scl_records_are_ignored() {
    for bad in [scl(1, 0), scl(1, 20), scl(9, 2), rec(0x00A0, &[3, 0])] {
        let wb = open(&cat(&[&window2(PLAIN), &bad]));
        assert_eq!(zoom_view(&wb), (100, SheetViewMode::Normal));
        assert_eq!(super::xls_tests::v(&wb, 0, "A1"), gridcraft_core::Value::Number(7.0));
    }
}

#[test]
fn window2_fslv_opens_in_page_break_preview() {
    let wb = open(&cat(&[&window2(PLAIN | F_SLV), &scl(3, 5)]));
    assert_eq!(zoom_view(&wb), (60, SheetViewMode::PageBreakPreview));
    // The other WINDOW2 display options still read as before.
    let sh = &wb.sheets[0];
    assert!(sh.show_gridlines && sh.show_headings && sh.show_zeros && !sh.show_formulas && !sh.right_to_left);
}

#[test]
fn plv_page_layout_view_opens_in_page_layout() {
    assert_eq!(zoom_view(&open(&cat(&[&window2(PLAIN), &plv(true)]))), (100, SheetViewMode::PageLayout));
    assert_eq!(zoom_view(&open(&cat(&[&window2(PLAIN), &plv(false)]))), (100, SheetViewMode::Normal));
}

#[test]
fn zoom_and_view_survive_save_as_xlsx() {
    let round = |wb: &Workbook| read_xlsx(&write_xlsx(wb).expect("xlsx saves")).expect("xlsx opens").0;
    let zoomed = open(&cat(&[&window2(PLAIN), &scl(3, 4)]));
    assert_eq!(zoom_view(&round(&zoomed)), (75, SheetViewMode::Normal));
    let preview = open(&window2(PLAIN | F_SLV));
    assert_eq!(zoom_view(&round(&preview)), (100, SheetViewMode::PageBreakPreview));
}
