//! Outline Settings "Automatic styles" from `.xls` WsBool fApplyStyles ([MS-XLS] 2.4.351) and
//! `.xlsb` BrtWsProp fApplyStyles ([MS-XLSB] 2.4.875): bit 5 of the first flag byte in both.
use gridcraft_model::Sheet;

use super::xls_tests::{one_sheet_with, rec};
use super::xlsb_print_tests::{package, record, worksheet};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

fn xls_sheet(wsbool: u16) -> Sheet {
    let (wb, _) = read_xls(&one_sheet_with(&rec(0x0081, &wsbool.to_le_bytes()))).unwrap();
    (*wb.sheets[0]).clone()
}

/// BrtWsProp: three flag bytes, an empty `brtcolorTab`, rwSync, colSync and an empty strName.
fn xlsb_sheet(flags: u16) -> Sheet {
    let mut out = flags.to_le_bytes().to_vec();
    out.push(0);
    out.extend([0; 8]);
    out.extend(u32::MAX.to_le_bytes());
    out.extend(u32::MAX.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    let (wb, _) = read_xlsb(&package(&worksheet(&record(147, &out)))).unwrap();
    wb.sheet(0).unwrap().clone()
}

#[test]
fn xls_wsbool_apply_styles_set() {
    let sh = xls_sheet(0x04E1);
    assert!(sh.outline_apply_styles);
    assert!(sh.summary_below && sh.summary_right);
}

#[test]
fn xls_wsbool_apply_styles_clear() {
    assert!(!xls_sheet(0x04C1).outline_apply_styles);
}

#[test]
fn xls_apply_styles_survives_saving_as_xlsx() {
    let (wb, _) = read_xls(&one_sheet_with(&rec(0x0081, &0x04E1u16.to_le_bytes()))).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(back.sheet(0).unwrap().outline_apply_styles);
}

#[test]
fn xlsb_ws_prop_apply_styles_with_summaries() {
    let sh = xlsb_sheet(0x00E0);
    assert!(sh.outline_apply_styles);
    assert!(sh.summary_below && sh.summary_right);
}

#[test]
fn xlsb_ws_prop_apply_styles_alone() {
    let sh = xlsb_sheet(0x0020);
    assert!(sh.outline_apply_styles);
    assert!(!sh.summary_below && !sh.summary_right);
}

#[test]
fn xlsb_ws_prop_apply_styles_clear() {
    assert!(!xlsb_sheet(0x00C0).outline_apply_styles);
}
