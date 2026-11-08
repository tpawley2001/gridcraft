//! `.xls` list boxes, drop-downs, group boxes and labels ([MS-XLS] 2.4.181 Obj, 2.5.149 FtLbsData,
//! 2.5.146 LbsDropData, 2.5.142 FtGboData) open as form controls with their items, selection and
//! linked cell, and keep them through Save As `.xlsx`. Edit boxes are still reported.

use gridcraft_core::CellRef;
use gridcraft_model::{ControlKind, ListSelection};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{cat, rec, u16s};
use super::xls_shapes_tests::{shape, txo, xls};

const OT_EDIT_BOX: u16 = 0x000D;
const OT_LABEL: u16 = 0x000E;
const OT_LIST: u16 = 0x0012;
const OT_GROUP_BOX: u16 = 0x0013;
const OT_DROP_DOWN: u16 = 0x0014;

/// FtCmo flags: fLocked | fPrint | fAutoFill | fAutoLine.
const CONTROL_FLAGS: u16 = 0x6011;
/// FtCmo fUIObj: an arrow Excel made itself (AutoFilter, list validation).
const F_UI_OBJ: u16 = 0x0100;

/// FtCmo ([MS-XLS] 2.5.143).
fn cmo(ot: u16, id: u16, flags: u16) -> Vec<u8> {
    cat(&[&u16s(&[0x15, 0x12, ot, id, flags]), &[0; 12]])
}

/// FtSbs (2.5.163) as Excel writes it for a list: the list's own scroll state.
fn sbs() -> Vec<u8> {
    u16s(&[0x0C, 0x14, 0, 0, 0, 0, 100, 1, 10, 0, 0, 0])
}

/// ObjLinkFmla (2.5.200), ft 0x0E, holding one absolute PtgRef to (row, col), zero-based.
fn link(row: u16, col: u16) -> Vec<u8> {
    cat(&[&u16s(&[0x0E, 12, 5]), &[0; 4], &[0x24], &u16s(&[row, col]), &[0]])
}

/// FtLbsData (2.5.149). `area` = (first row, last row, first col, last col), zero-based, as an
/// absolute PtgArea; `drop` = LbsDropData's cLine (drop-downs only). cbFtLbsData holds the value
/// Excel writes (0x1FEE), which is not the real size.
fn lbs_data(area: Option<(u16, u16, u16, u16)>, c_lines: u16, i_sel: u16, flags: u16, drop: Option<u16>) -> Vec<u8> {
    let fmla = match area {
        // cbFmla 16: cce (2) + unused (4) + PtgArea (9) + 1 padding byte.
        Some((r1, r2, c1, c2)) => cat(&[&u16s(&[16, 9]), &[0; 4], &[0x25], &u16s(&[r1, r2, c1, c2]), &[0]]),
        None => u16s(&[0]),
    };
    // LbsDropData: flags, cLine, dxMin, then an empty XLUnicodeString (cch 0, fHighByte 0) + padding.
    let drop = drop.map(|lines| cat(&[&u16s(&[0, lines, 0, 0]), &[0, 0]])).unwrap_or_default();
    cat(&[&u16s(&[0x13, 0x1FEE]), &fmla, &u16s(&[c_lines, i_sel, flags, 0]), &drop])
}

/// FtGboData (2.5.142): accel, reserved, flags (bit 0 fNo3d).
fn gbo_data(no_3d: bool) -> Vec<u8> {
    u16s(&[0x0F, 0x06, 0, 0, u16::from(no_3d)])
}

fn obj(parts: &[&[u8]]) -> Vec<u8> {
    rec(0x005D, &cat(&[&cat(parts), &u16s(&[0, 0])]))
}

/// A list box over `area` linked to (row, col), placed at (col, row) .. (col + 2, row + 4).
fn list_box(id: u16, area: Option<(u16, u16, u16, u16)>, linked: (u16, u16), i_sel: u16, flags: u16, at: (u16, u16)) -> Vec<u8> {
    let o = obj(&[&cmo(OT_LIST, id, CONTROL_FLAGS), &sbs(), &link(linked.0, linked.1), &lbs_data(area, 4, i_sel, flags, None)]);
    cat(&[&shape(201, &[], at.0, at.1, at.0 + 2, at.1 + 4), &o])
}

/// A drop-down over A1:A3 linked to $C$1, item 3 picked, showing 5 lines when open.
fn drop_down() -> Vec<u8> {
    let o = obj(&[&cmo(OT_DROP_DOWN, 2, CONTROL_FLAGS), &sbs(), &link(0, 2), &lbs_data(Some((0, 2, 0, 0)), 3, 3, 0, Some(5))]);
    cat(&[&shape(201, &[], 3, 0, 5, 1), &o])
}

fn no_form_control_warning(warnings: &[String]) -> bool {
    !warnings.iter().any(|w| w.contains("Form controls"))
}

#[test]
fn a_list_box_opens_with_its_items_link_and_selected_item() {
    let (wb, report) = read_xls(&xls(&list_box(1, Some((0, 3, 4, 4)), (0, 1), 2, 0, (1, 2)))).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert!(sheet.shapes.is_empty(), "a list box is a control, not a shape");
    assert_eq!(sheet.controls.len(), 1, "{:?}", report.warnings);
    let c = &sheet.controls[0];
    assert_eq!(c.kind, ControlKind::ListBox);
    assert_eq!(c.input_range.as_deref(), Some("$E$1:$E$4"));
    assert_eq!(c.link.as_deref(), Some("$B$1"));
    assert_eq!(c.value, 2, "iSel is the 1-based selected item, not FtSbs's value");
    assert_eq!(c.selection, ListSelection::Single);
    assert!(c.three_d);
    assert!(c.locked && c.print);
    assert_eq!(c.anchor.cell, CellRef::new(2, 1), "top-left of the drawing anchor (B3)");
    assert!(no_form_control_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn list_selection_type_and_flat_shading_come_from_the_flags() {
    // wListSelType is bits 4-5 of FtLbsData's flags; bit 3 is fNo3d.
    let records =
        cat(&[&list_box(1, Some((0, 3, 4, 4)), (0, 1), 0, 0x0008 | (1 << 4), (0, 0)), &list_box(2, Some((0, 3, 4, 4)), (0, 2), 0, 2 << 4, (0, 6))]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    let controls = &wb.sheet(0).unwrap().controls;
    let got: Vec<_> = controls.iter().map(|c| (c.kind, c.selection, c.three_d, c.value)).collect();
    assert_eq!(
        got,
        [(ControlKind::ListBox, ListSelection::Multi, false, 0), (ControlKind::ListBox, ListSelection::Extend, true, 0)],
        "{:?}",
        report.warnings
    );
    assert_ne!(controls[0].id, controls[1].id);
}

#[test]
fn a_list_box_without_an_input_range_has_none() {
    let (wb, report) = read_xls(&xls(&list_box(1, None, (0, 1), 0, 0, (0, 0)))).unwrap();
    let c = &wb.sheet(0).unwrap().controls[0];
    assert_eq!((c.kind, c.input_range.clone(), c.link.clone()), (ControlKind::ListBox, None, Some("$B$1".to_string())), "{:?}", report.warnings);
}

#[test]
fn a_drop_down_opens_with_its_items_pick_and_drop_lines() {
    let (wb, report) = read_xls(&xls(&drop_down())).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.controls.len(), 1, "{:?}", report.warnings);
    let c = &sheet.controls[0];
    assert_eq!(c.kind, ControlKind::DropDown);
    assert_eq!(c.input_range.as_deref(), Some("$A$1:$A$3"));
    assert_eq!(c.link.as_deref(), Some("$C$1"));
    assert_eq!((c.value, c.drop_lines), (3, 5));
    assert_eq!(c.anchor.cell, CellRef::new(0, 3), "D1");
    assert!(no_form_control_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn a_drop_down_cut_off_before_its_drop_data_keeps_eight_lines() {
    // FtLbsData ends after idEdit: no LbsDropData and no FtEnd.
    let lbs = lbs_data(Some((0, 2, 0, 0)), 3, 1, 0, None);
    let o = rec(0x005D, &cat(&[&cmo(OT_DROP_DOWN, 1, CONTROL_FLAGS), &sbs(), &link(0, 2), &lbs]));
    let (wb, report) = read_xls(&xls(&cat(&[&shape(201, &[], 0, 0, 2, 1), &o]))).unwrap();
    let c = &wb.sheet(0).unwrap().controls[0];
    assert_eq!((c.kind, c.value, c.drop_lines), (ControlKind::DropDown, 1, 8), "{:?}", report.warnings);
}

#[test]
fn a_truncated_list_obj_opens_without_panicking() {
    // FtLbsData's ObjFmla claims 16 bytes but the record ends after 3 of them.
    let o = rec(0x005D, &cat(&[&cmo(OT_LIST, 1, CONTROL_FLAGS), &u16s(&[0x13, 0x1FEE, 16, 9]), &[0]]));
    let (wb, _) = read_xls(&xls(&cat(&[&shape(201, &[], 0, 0, 2, 4), &o]))).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert!(sheet.shapes.is_empty());
    assert!(sheet.controls.iter().all(|c| c.input_range.is_none()));
}

#[test]
fn group_boxes_and_labels_open_with_their_captions() {
    let group = obj(&[&cmo(OT_GROUP_BOX, 1, CONTROL_FLAGS), &gbo_data(true)]);
    let label = obj(&[&cmo(OT_LABEL, 2, CONTROL_FLAGS)]);
    let records = cat(&[&shape(201, &[], 0, 0, 4, 6), &group, &txo("Size"), &shape(201, &[], 5, 0, 7, 1), &label, &txo("Total:")]);
    let (wb, report) = read_xls(&xls(&records)).unwrap();
    let sheet = wb.sheet(0).unwrap();
    let got: Vec<_> = sheet.controls.iter().map(|c| (c.kind, c.text.as_str())).collect();
    assert_eq!(got, [(ControlKind::GroupBox, "Size"), (ControlKind::Label, "Total:")], "{:?}", report.warnings);
    assert!(!sheet.controls[0].three_d, "FtGboData fNo3d set");
    assert_eq!(sheet.controls[1].anchor.cell, CellRef::new(0, 5), "F1");
    assert!(sheet.shapes.is_empty());
    assert!(no_form_control_warning(&report.warnings), "{:?}", report.warnings);
}

#[test]
fn an_autofilter_arrow_is_not_a_drop_down_control() {
    let o = obj(&[&cmo(OT_DROP_DOWN, 1, CONTROL_FLAGS | F_UI_OBJ), &sbs(), &lbs_data(None, 0, 0, 0, Some(8))]);
    let (wb, _) = read_xls(&xls(&cat(&[&shape(201, &[], 0, 0, 1, 1), &o]))).unwrap();
    assert!(wb.sheet(0).unwrap().controls.is_empty());
}

#[test]
fn an_edit_box_is_still_reported() {
    let o = obj(&[&cmo(OT_EDIT_BOX, 1, CONTROL_FLAGS)]);
    let (wb, report) = read_xls(&xls(&cat(&[&shape(201, &[], 0, 0, 2, 1), &o]))).unwrap();
    assert!(wb.sheet(0).unwrap().controls.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("Form controls in .xls files are not imported yet.")), "{:?}", report.warnings);
}

#[test]
fn list_boxes_and_drop_downs_survive_save_as_xlsx() {
    let records = cat(&[&list_box(1, Some((0, 3, 4, 4)), (0, 1), 2, 1 << 4, (0, 2)), &drop_down()]);
    let (wb, _) = read_xls(&xls(&records)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let controls = &back.sheet(0).unwrap().controls;
    let got: Vec<_> = controls.iter().map(|c| (c.kind, c.input_range.clone(), c.link.clone(), c.value)).collect();
    assert_eq!(
        got,
        [
            (ControlKind::ListBox, Some("$E$1:$E$4".to_string()), Some("$B$1".to_string()), 2),
            (ControlKind::DropDown, Some("$A$1:$A$3".to_string()), Some("$C$1".to_string()), 3),
        ]
    );
    assert_eq!(controls[0].selection, ListSelection::Multi);
    assert_eq!(controls[1].drop_lines, 5);
}
