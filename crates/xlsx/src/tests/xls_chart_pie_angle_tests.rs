//! `.xls` Pie chart group ([MS-XLS] 2.4.196 Pie: `anStart` u16, `pcDonut` u16, flags u16): the
//! first slice angle and the doughnut hole size are read into the chart and survive saving as
//! `.xlsx` (`c:firstSliceAng`, `c:holeSize`; ECMA-376 Part 1 §21.2.2.68, §21.2.2.82).

use gridcraft_model::{Chart, ChartKind};

use crate::{read_xlsx, write_xlsx};

use super::xls_chart_tests::{chart, embedded, read, rec, series, u16s};

/// The one chart of an embedded `.xls` chart whose group is a Pie record.
fn pie(an_start: u16, pc_donut: u16) -> Chart {
    let group = rec(0x1019, &u16s(&[an_start, pc_donut, 0]));
    let (wb, warnings) = read(&embedded(&chart(&series(1, &[]), &group, &[])));
    wb.sheets[0].charts.first().cloned().unwrap_or_else(|| panic!("no chart: {warnings:?}"))
}

#[test]
fn xls_pie_first_slice_angle_is_read() {
    // (anStart, expected first_slice_angle): 360 is the same position as 0.
    for (an_start, angle) in [(0u16, 0u16), (90, 90), (270, 270), (359, 359), (360, 0)] {
        let ch = pie(an_start, 0);
        assert_eq!(ch.kind, ChartKind::Pie);
        assert_eq!(ch.first_slice_angle, angle, "anStart {an_start}");
    }
}

#[test]
fn xls_doughnut_hole_size_and_angle_are_read() {
    // (pcDonut, expected hole_size): Excel's doughnut hole is 10-90%.
    for (pc_donut, hole) in [(50u16, 50u8), (10, 10), (75, 75), (90, 90), (5, 10), (95, 90)] {
        let ch = pie(45, pc_donut);
        assert_eq!(ch.kind, ChartKind::Doughnut, "pcDonut {pc_donut}");
        assert_eq!(ch.hole_size, hole, "pcDonut {pc_donut}");
        assert_eq!(ch.first_slice_angle, 45, "pcDonut {pc_donut}");
    }
}

#[test]
fn xls_pie_without_hole_keeps_the_default_hole_size() {
    // A plain pie has no hole; the unused field keeps the model default.
    assert_eq!(pie(30, 0).hole_size, 75);
}

#[test]
fn xls_doughnut_angle_and_hole_survive_saving_as_xlsx() {
    let group = rec(0x1019, &u16s(&[120, 40, 0]));
    let (wb, _) = read(&embedded(&chart(&series(1, &[]), &group, &[])));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let ch = &back.sheets[0].charts[0];
    assert_eq!(ch.kind, ChartKind::Doughnut);
    assert_eq!(ch.first_slice_angle, 120);
    assert_eq!(ch.hole_size, 40);
}
