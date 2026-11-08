//! `.xls` form controls ([MS-XLS] 2.4.181 Obj): check boxes, option buttons, scroll bars and spin
//! buttons open as form controls with their state, range and linked cell, and keep them through
//! Save As `.xlsx`. Other control kinds are still reported.

use gridcraft_core::CellRef;
use gridcraft_model::ControlKind;

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{cat, rec, u16s};
use super::xls_shapes_tests::{shape, txo, xls};

const OT_CHECK_BOX: u16 = 0x000B;
const OT_OPTION_BUTTON: u16 = 0x000C;
const OT_SPIN: u16 = 0x0010;
const OT_SCROLL_BAR: u16 = 0x0011;
const OT_LIST: u16 = 0x0012;

/// FtCmo flags: fLocked | fPrint | fAutoFill | fAutoLine (Excel's usual set for a new control).
const CONTROL_FLAGS: u16 = 0x6011;

/// FtCmo ([MS-XLS] 2.5.143).
fn cmo(ot: u16, id: u16, flags: u16) -> Vec<u8> {
    cat(&[&u16s(&[0x15, 0x12, ot, id, flags]), &[0; 12]])
}

/// FtCbls (2.5.135) and FtRbo (2.5.155): reserved bodies.
fn cbls() -> Vec<u8> {
    cat(&[&u16s(&[0x0A, 0x0C]), &[0; 12]])
}
fn rbo() -> Vec<u8> {
    cat(&[&u16s(&[0x0B, 0x06]), &[0; 6]])
}

/// FtSbs (2.5.163): iVal, iMin, iMax, dInc, dPage, fHoriz, then the flags (bit 3 fNo3d).
fn sbs(val: u16, min: u16, max: u16, inc: u16, page: u16, horiz: u16, flags: u16) -> Vec<u8> {
    u16s(&[0x0C, 0x14, 0, 0, val, min, max, inc, page, horiz, 0, flags])
}

/// ObjLinkFmla (2.5.200) of type `ft` holding one absolute PtgRef to (row, col), zero-based.
fn link(ft: u16, row: u16, col: u16) -> Vec<u8> {
    // cbFmla 12: cce (2) + unused (4) + PtgRef (5) + 1 padding byte.
    cat(&[&u16s(&[ft, 12, 5]), &[0; 4], &[0x24], &u16s(&[row, col]), &[0]])
}

/// FtCblsData (2.5.136): fChecked, accel, reserved, then bit 0 fNo3d.
fn cbls_data(checked: u16, no_3d: bool) -> Vec<u8> {
    u16s(&[0x12, 0x08, checked, 0, 0, u16::from(no_3d)])
}

/// FtRboData (2.5.156).
fn rbo_data(next: u16, first: bool) -> Vec<u8> {
    u16s(&[0x11, 0x04, next, u16::from(first)])
}

fn obj(parts: &[&[u8]]) -> Vec<u8> {
    rec(0x005D, &cat(&[&cat(parts), &u16s(&[0, 0])]))
}

fn check_box(id: u16, checked: u16, no_3d: bool, linked: Option<(u16, u16)>, caption: &str, col: u16, row: u16) -> Vec<u8> {
    let l = linked.map(|(r, c)| link(0x14, r, c)).unwrap_or_default();
    let o = obj(&[&cmo(OT_CHECK_BOX, id, CONTROL_FLAGS), &cbls(), &l, &cbls_data(checked, no_3d)]);
    cat(&[&shape(201, &[], col, row, col + 2, row + 1), &o, &txo(caption)])
}

fn scroll_bar() -> Vec<u8> {
    // Horizontal, value 25 in 0..=200, step 5, page 20, linked to $C$3, flat (fNo3d).
    let o = obj(&[&cmo(OT_SCROLL_BAR, 3, CONTROL_FLAGS), &sbs(25, 0, 200, 5, 20, 1, 0x0008), &link(0x0E, 2, 2)]);
    cat(&[&shape(201, &[], 4, 1, 8, 2), &o])
}

fn no_form_control_warning(warnings: &[String]) -> bool {
    !warnings.iter().any(|w| w.contains("Form controls"))
}

#[test]
fn a_checked_check_box_opens_with_its_caption_state_and_linked_cell() {
    let (wb, report) = read_xls(&xls(&check_box(1, 1, true, Some((1, 1)), "Paid", 1, 2))).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert!(sheet.shapes.is_empty(), "a check box is a control, not a shape");
    assert_eq!(sheet.controls.len(), 1, "{:?}", report.warnings);
    let c = &sheet.controls[0];
    assert_eq!(c.kind, ControlKind::CheckBox);
    assert_eq!(c.text, "Paid");
    assert_eq!(c.value, 1, "fChecked 1");
    assert_eq!(c.link.as_deref(), Some("$B$2"));
    assert!(!c.three_d, "fNo3d set");
    assert!(c.locked && c.print);
    assert_eq!(c.anchor.cell, CellRef::new(2, 1), "top-left of the drawing anchor (B3)");
    assert!(no_form_control_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn a_mixed_check_box_without_a_link_keeps_state_two_and_no_link() {
    let (wb, _) = read_xls(&xls(&check_box(1, 2, false, None, "Maybe", 0, 0))).unwrap();
    let c = &wb.sheet(0).unwrap().controls[0];
    assert_eq!((c.kind, c.value, c.link.clone(), c.three_d), (ControlKind::CheckBox, 2, None, true));
}

#[test]
fn option_buttons_open_with_their_captions_and_the_selected_one() {
    let opt = |id: u16, checked: u16, first: bool, next: u16, caption: &str, row: u16| {
        let o =
            obj(&[&cmo(OT_OPTION_BUTTON, id, CONTROL_FLAGS), &cbls(), &rbo(), &link(0x14, 0, 3), &cbls_data(checked, false), &rbo_data(next, first)]);
        cat(&[&shape(201, &[], 0, row, 2, row + 1), &o, &txo(caption)])
    };
    let records = cat(&[&opt(1, 0, true, 2, "Small", 0), &opt(2, 1, false, 1, "Large", 1)]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    let controls = &wb.sheet(0).unwrap().controls;
    let got: Vec<(ControlKind, &str, u32)> = controls.iter().map(|c| (c.kind, c.text.as_str(), c.value)).collect();
    assert_eq!(got, [(ControlKind::OptionButton, "Small", 0), (ControlKind::OptionButton, "Large", 1)], "{:?}", report.warnings);
    assert_eq!(controls[0].link.as_deref(), Some("$D$1"));
    assert_ne!(controls[0].id, controls[1].id);
    assert!(no_form_control_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn a_scroll_bar_opens_as_a_slider_with_its_range_and_link() {
    let (wb, report) = read_xls(&xls(&scroll_bar())).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.controls.len(), 1, "{:?}", report.warnings);
    let c = &sheet.controls[0];
    assert_eq!(c.kind, ControlKind::Slider);
    assert_eq!((c.value, c.min, c.max, c.step, c.page), (25, 0, 200, 5, 20));
    assert!(c.horizontal);
    assert!(!c.three_d, "fNo3d set");
    assert_eq!(c.link.as_deref(), Some("$C$3"));
    assert_eq!(c.anchor.cell, CellRef::new(1, 4), "E2");
    assert!(no_form_control_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn a_vertical_spin_button_opens_with_its_range() {
    let o = obj(&[&cmo(OT_SPIN, 4, CONTROL_FLAGS), &sbs(3, 1, 10, 1, 0, 0, 0), &link(0x0E, 0, 0)]);
    let (wb, report) = read_xls(&xls(&cat(&[&shape(201, &[], 2, 2, 3, 4), &o]))).unwrap();
    let c = &wb.sheet(0).unwrap().controls[0];
    assert_eq!(c.kind, ControlKind::Spin, "{:?}", report.warnings);
    assert_eq!((c.value, c.min, c.max, c.step), (3, 1, 10, 1));
    assert!(!c.horizontal);
    assert!(c.three_d);
    assert_eq!(c.link.as_deref(), Some("$A$1"));
}

#[test]
fn check_boxes_and_scroll_bars_survive_save_as_xlsx() {
    let records = cat(&[&check_box(1, 1, false, Some((1, 1)), "Paid", 1, 2), &scroll_bar()]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let controls = &back.sheet(0).unwrap().controls;
    let got: Vec<_> = controls.iter().map(|c| (c.kind, c.link.clone(), c.value)).collect();
    assert_eq!(got, [(ControlKind::CheckBox, Some("$B$2".to_string()), 1), (ControlKind::Slider, Some("$C$3".to_string()), 25)]);
    assert_eq!(controls[0].text, "Paid");
    assert_eq!((controls[1].min, controls[1].max, controls[1].step, controls[1].page), (0, 200, 5, 20));
}

#[test]
fn a_truncated_scroll_bar_obj_opens_without_panicking() {
    // FtSbs claims 20 bytes but the record ends after 6 of them, with no FtEnd.
    let o = rec(0x005D, &cat(&[&cmo(OT_SCROLL_BAR, 1, CONTROL_FLAGS), &u16s(&[0x0C, 0x14, 0, 0, 7])]));
    let (wb, _) = read_xls(&xls(&cat(&[&shape(201, &[], 0, 0, 2, 1), &o]))).unwrap();
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
}

#[test]
fn a_list_box_is_still_reported() {
    let o = obj(&[&cmo(OT_LIST, 1, CONTROL_FLAGS), &sbs(0, 0, 0, 1, 0, 0, 0)]);
    let (wb, report) = read_xls(&xls(&cat(&[&shape(201, &[], 0, 0, 2, 4), &o]))).unwrap();
    assert!(wb.sheet(0).unwrap().controls.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("Form controls in .xls files are not imported yet.")), "{:?}", report.warnings);
}
