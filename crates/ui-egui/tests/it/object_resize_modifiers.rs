//! Modifier keys while dragging an object's handles (Excel): Shift on a corner keeps the
//! proportions, Ctrl keeps the centre, Alt snaps the moved edges to cell gridlines.

use egui::Vec2;
use gridcraft_engine::Session;
use gridcraft_ui_egui::objects::{DragMods, Handle, resized, resized_with, snap_to_cells};
use serde_json::json;

const ORIG: [f32; 4] = [100.0, 100.0, 200.0, 150.0];
const MIN: Vec2 = Vec2::new(8.0, 8.0);

const RATIO: DragMods = DragMods { keep_ratio: true, from_center: false, snap_to_cells: false };
const CENTER: DragMods = DragMods { keep_ratio: false, from_center: true, snap_to_cells: false };
const BOTH: DragMods = DragMods { keep_ratio: true, from_center: true, snap_to_cells: false };

fn close(got: [f32; 4], want: [f32; 4], what: &str) {
    for i in 0..4 {
        assert!((got[i] - want[i]).abs() < 0.01, "{what}: got {got:?}, want {want:?}");
    }
}

#[test]
fn no_modifiers_resize_as_before() {
    for h in Handle::ALL {
        for d in [Vec2::new(30.0, -12.0), Vec2::new(-95.0, 60.0)] {
            assert_eq!(resized_with(ORIG, h, d, MIN, DragMods::default()), resized(ORIG, h, d, MIN), "{h:?} {d:?}");
        }
    }
}

#[test]
fn shift_on_a_corner_keeps_the_proportions() {
    let cases = [
        (Handle::BottomRight, (50.0, 10.0), [100.0, 100.0, 250.0, 175.0]),
        (Handle::TopLeft, (-10.0, -25.0), [50.0, 75.0, 200.0, 150.0]),
        (Handle::BottomRight, (-60.0, -10.0), [100.0, 100.0, 180.0, 140.0]),
        // Shrinking stops at the minimum size, still in proportion.
        (Handle::BottomRight, (-95.0, -45.0), [100.0, 100.0, 116.0, 108.0]),
    ];
    for (h, (dx, dy), want) in cases {
        close(resized_with(ORIG, h, Vec2::new(dx, dy), MIN, RATIO), want, &format!("{h:?} ({dx},{dy})"));
    }
}

#[test]
fn shift_on_an_edge_handle_changes_nothing() {
    for (h, d) in [(Handle::Right, Vec2::new(20.0, 5.0)), (Handle::Top, Vec2::new(7.0, -15.0))] {
        close(resized_with(ORIG, h, d, MIN, RATIO), resized(ORIG, h, d, MIN), &format!("{h:?}"));
    }
}

#[test]
fn ctrl_keeps_the_centre() {
    close(resized_with(ORIG, Handle::BottomRight, Vec2::new(20.0, 10.0), MIN, CENTER), [80.0, 90.0, 220.0, 160.0], "corner");
    close(resized_with(ORIG, Handle::Right, Vec2::new(20.0, 0.0), MIN, CENTER), [80.0, 100.0, 220.0, 150.0], "edge");
    let r = resized_with(ORIG, Handle::Left, Vec2::new(500.0, 0.0), MIN, CENTER);
    assert!((r[2] - r[0] - MIN.x).abs() < 0.01 && ((r[0] + r[2]) / 2.0 - 150.0).abs() < 0.01, "shrinks to the minimum around the centre: {r:?}");
    let r = resized_with(ORIG, Handle::Right, Vec2::new(400.0, 0.0), MIN, CENTER);
    assert!(r[0] >= 0.0, "never past the sheet's left edge: {r:?}");
}

#[test]
fn shift_and_ctrl_scale_around_the_centre() {
    close(resized_with(ORIG, Handle::BottomRight, Vec2::new(50.0, 10.0), MIN, BOTH), [50.0, 75.0, 250.0, 175.0], "both");
}

fn sheet_session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    // Uneven columns and rows, so the nearest gridline is not a fixed step.
    s.execute("home.columnWidth", json!({"cols": "B:B", "chars": 20})).unwrap();
    s.execute("home.rowHeight", json!({"rows": "3:3", "points": 40})).unwrap();
    s
}

#[test]
fn alt_snaps_a_move_to_the_nearest_gridlines() {
    let s = sheet_session();
    let sh = &s.active().unwrap().wb.sheets[0];
    let (cl, rt) = (|c| sh.col_left(c) as f32, |r| sh.row_top(r) as f32);
    // Just right of column C's left edge and just above row 4's top edge.
    let (x0, y0) = (cl(2) + 3.0, rt(3) - 2.0);
    let r = snap_to_cells(sh, [x0, y0, x0 + 70.0, y0 + 30.0], None);
    close(r, [cl(2), rt(3), cl(2) + 70.0, rt(3) + 30.0], "move keeps its size");
}

#[test]
fn alt_snaps_only_the_edges_a_handle_moves() {
    let s = sheet_session();
    let sh = &s.active().unwrap().wb.sheets[0];
    let (cl, rt) = (|c| sh.col_left(c) as f32, |r| sh.row_top(r) as f32);
    let r0 = [cl(1) + 5.0, rt(1) + 4.0, cl(3) - 6.0, rt(4) + 5.0];
    close(snap_to_cells(sh, r0, Some(Handle::BottomRight)), [r0[0], r0[1], cl(3), rt(4)], "bottom-right");
    close(snap_to_cells(sh, r0, Some(Handle::Left)), [cl(1), r0[1], r0[2], r0[3]], "left");
    close(snap_to_cells(sh, r0, Some(Handle::Top)), [r0[0], rt(1), r0[2], r0[3]], "top");
}
