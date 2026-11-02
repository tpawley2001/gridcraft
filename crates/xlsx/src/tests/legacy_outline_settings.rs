//! Outline settings (Data > Outline > Settings: summary rows below / summary columns right) from
//! `.xls` WsBool ([MS-XLS] 2.4.351) and `.xlsb` BrtWsProp ([MS-XLSB] 2.4.875), built in code.
use gridcraft_model::{Color, Sheet};

use super::xls_print_tests::setup;
use super::xls_tests::{one_sheet_with, rec};
use super::xlsb_print_tests::{package, page_setup, record, worksheet};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

/// A sheet with this WsBool and a Setup asking for one page wide, which applies only with
/// `fFitToPage`.
fn xls_sheet(wsbool: u16) -> Sheet {
    let records = [rec(0x0081, &wsbool.to_le_bytes()), setup(9, 100, (1, 0), 0, 0.3, 0.3)].concat();
    let (wb, _) = read_xls(&one_sheet_with(&records)).unwrap();
    (*wb.sheets[0]).clone()
}

/// BrtWsProp: three flag bytes, an RGB `brtcolorTab`, rwSync, colSync and an empty strName.
fn ws_prop(flags: u16, tab: Option<(u8, u8, u8)>) -> Vec<u8> {
    let mut out = flags.to_le_bytes().to_vec();
    out.push(0);
    match tab {
        Some((r, g, b)) => out.extend([2 << 1 | 1, 0, 0, 0, r, g, b, 255]),
        None => out.extend([0; 8]),
    }
    out.extend(u32::MAX.to_le_bytes());
    out.extend(u32::MAX.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    record(147, &out)
}

fn xlsb_sheet(flags: u16, tab: Option<(u8, u8, u8)>) -> Sheet {
    let (wb, _) = read_xlsb(&package(&worksheet(&ws_prop(flags, tab)))).unwrap();
    wb.sheet(0).unwrap().clone()
}

#[test]
fn xls_wsbool_default_keeps_summaries_below_and_right() {
    let sh = xls_sheet(0x04C1);
    assert!(sh.summary_below && sh.summary_right);
    assert!(sh.print.fit_to.is_none());
}

#[test]
fn xls_wsbool_rows_above() {
    let sh = xls_sheet(0x0481);
    assert!(!sh.summary_below && sh.summary_right);
}

#[test]
fn xls_wsbool_columns_left() {
    let sh = xls_sheet(0x0441);
    assert!(sh.summary_below && !sh.summary_right);
}

#[test]
fn xls_wsbool_both_off_with_fit_to_page() {
    let sh = xls_sheet(0x0501);
    assert!(!sh.summary_below && !sh.summary_right);
    assert_eq!(sh.print.fit_to, Some((1, 0)));
}

#[test]
fn xls_short_wsbool_keeps_the_defaults() {
    let (wb, _) = read_xls(&one_sheet_with(&rec(0x0081, &[0x01]))).unwrap();
    assert!(wb.sheets[0].summary_below && wb.sheets[0].summary_right);
}

#[test]
fn xls_rows_above_survive_saving_as_xlsx() {
    let (wb, _) = read_xls(&one_sheet_with(&rec(0x0081, &0x0481u16.to_le_bytes()))).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let sh = back.sheet(0).unwrap();
    assert!(!sh.summary_below && sh.summary_right);
}

#[test]
fn xlsb_ws_prop_summary_flags_clear() {
    let sh = xlsb_sheet(0x0000, Some((0x12, 0x34, 0x56)));
    assert!(!sh.summary_below && !sh.summary_right);
    assert_eq!(sh.tab_color, Some(Color::Rgb(0x123456)));
    assert!(sh.print.fit_to.is_none());
}

#[test]
fn xlsb_ws_prop_summary_flags_set() {
    let sh = xlsb_sheet(0x00C0, None);
    assert!(sh.summary_below && sh.summary_right);
    assert_eq!(sh.tab_color, None);
}

#[test]
fn xlsb_ws_prop_fit_to_page_still_read() {
    let body = [ws_prop(0x0100, None), page_setup(9, 0, 0, (1, 0), 0)].concat();
    let (wb, _) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let sh = wb.sheet(0).unwrap();
    assert_eq!(sh.print.fit_to, Some((1, 0)));
    assert!(!sh.summary_below && !sh.summary_right);
}
