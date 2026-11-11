//! `.xls` filled radar charts: a chart group given by the RadarArea record ([MS-XLS] 2.4.216, 0x1040)
//! is Excel's filled radar, not the plain Radar record (2.4.215, 0x103E).

use gridcraft_model::ChartKind;

use crate::{read_xlsx, write_xlsx};

use super::xls_chart_tests::{chart, embedded, read, rec, series, u16s};

const RADAR: u16 = 0x103E;
const RADAR_AREA: u16 = 0x1040;

fn kind(group_id: u16, flags: u16) -> (ChartKind, gridcraft_model::Workbook) {
    let group = rec(group_id, &u16s(&[flags, 0]));
    let (wb, warnings) = read(&embedded(&chart(&series(1, &[]), &group, &[])));
    let k = wb.sheets[0].charts.first().unwrap_or_else(|| panic!("no chart: {warnings:?}")).kind;
    (k, wb)
}

fn kind_after_xlsx_save(wb: &gridcraft_model::Workbook) -> ChartKind {
    let (back, _) = read_xlsx(&write_xlsx(wb).unwrap()).unwrap();
    back.sheets[0].charts[0].kind
}

#[test]
fn xls_radar_area_record_opens_as_filled_radar() {
    let (k, _) = kind(RADAR_AREA, 0);
    assert_eq!(k, ChartKind::RadarFilled);
}

#[test]
fn xls_radar_area_with_axis_labels_flag_is_still_filled() {
    let (k, _) = kind(RADAR_AREA, 0x0001);
    assert_eq!(k, ChartKind::RadarFilled);
}

#[test]
fn xls_filled_radar_stays_filled_when_saved_as_xlsx() {
    let (_, wb) = kind(RADAR_AREA, 0);
    assert_eq!(kind_after_xlsx_save(&wb), ChartKind::RadarFilled);
}

#[test]
fn xls_plain_radar_record_is_still_a_line_radar() {
    let (k, wb) = kind(RADAR, 0);
    assert_eq!(k, ChartKind::Radar);
    assert_eq!(kind_after_xlsx_save(&wb), ChartKind::Radar);
}
