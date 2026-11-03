use super::xls_autofilter_tests::{cat, filter_database, filter_info, rec, u16s, xls_file};
fn read(body: &[u8]) -> (gridcraft_model::Workbook, crate::ReadReport) {
    crate::read_xls(&xls_file(&filter_database(1, [0, 9, 0, 0]), &cat(&[&filter_info(1), body]))).unwrap()
}
fn header(cft: u32, count: u32, dates: u32) -> Vec<u8> {
    rec(
        0x087e,
        &cat(&[
            &[0; 12],
            &u16s(&[0]),
            &[0; 8],
            &cft.to_le_bytes(),
            &count.to_le_bytes(),
            &dates.to_le_bytes(),
            &u16s(&[8]),
            &[0; 4],
            &u32::MAX.to_le_bytes(),
            &[0; 16],
        ]),
    )
}
fn dynamic_record(cft: u32) -> Vec<u8> {
    header(cft, 0, 0)
}
fn groups_record(date: &[u8], blanks: bool, value: bool) -> Vec<u8> {
    let mut out = header(0, u32::from(value) + u32::from(blanks), 1);
    if value {
        out.extend(rec(0x087f, &cat(&[&[0; 12], &[6, 2, 0, 0, 0, 0, 1, 1, 0, 0], &[0], b"x"])));
    }
    if blanks {
        out.extend(rec(0x087f, &cat(&[&[0; 12], &[12, 2, 0, 0, 0, 0, 0, 0, 0, 0]])));
    }
    out.extend(rec(0x087f, &cat(&[&[0; 12], date])));
    out
}
use crate::{read_xlsx, write_xlsx};
use gridcraft_model::{DateGroup, DateGrouping, FilterCriterion};

fn dynamic(kind: &str) -> FilterCriterion {
    FilterCriterion::DynamicDate { kind: kind.into(), val: None, max_val: None, val_iso: None, max_val_iso: None }
}
fn expected_group(year: u16, month: Option<u8>, blanks: bool, values: Vec<String>) -> FilterCriterion {
    FilterCriterion::DateGroups {
        values,
        blanks,
        groups: vec![DateGroup {
            year,
            month,
            day: None,
            hour: None,
            minute: None,
            second: None,
            grouping: if month.is_some() { DateGrouping::Month } else { DateGrouping::Year },
        }],
    }
}
fn date(year: u16, month: u16, grouping: u32) -> Vec<u8> {
    [year.to_le_bytes().to_vec(), month.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(), vec![0; 12], grouping.to_le_bytes().to_vec()].concat()
}
#[test]
fn date_filters_import() {
    for (cft, kind) in [(15, "thisMonth"), (24, "Q1"), (39, "M12"), (10, "yesterday")] {
        let (wb, report) = read(&dynamic_record(cft));
        assert_eq!(wb.sheets[0].autofilter.as_ref().unwrap().criteria, vec![(0, dynamic(kind))]);
        assert!(report.warnings.iter().all(|w| !w.contains("AutoFilter")), "{:?}", report.warnings);
    }
    let (wb, _) = read(&dynamic_record(1));
    assert_eq!(wb.sheets[0].autofilter.as_ref().unwrap().criteria, vec![(0, FilterCriterion::AboveAverage(true))]);
    for (year, month, grouping, blanks, values) in [(2026, 3, 1, false, vec!["x".into()]), (2025, 0, 0, true, vec![])] {
        let (wb, report) = read(&groups_record(&date(year, month, grouping), blanks, !values.is_empty()));
        assert_eq!(
            wb.sheets[0].autofilter.as_ref().unwrap().criteria,
            vec![(0, expected_group(year, (grouping == 1).then_some(month as u8), blanks, values))]
        );
        assert!(report.warnings.iter().all(|w| !w.contains("AutoFilter")), "{:?}", report.warnings);
    }
}
#[test]
fn invalid_dates_warn_once() {
    for body in [
        dynamic_record(999),
        groups_record(&date(2026, 13, 1), false, false),
        groups_record(&date(0, 3, 1), false, false),
        groups_record(&date(2026, 3, 9), false, false),
    ] {
        let (wb, report) = read(&body.repeat(2));
        assert!(wb.sheets[0].autofilter.as_ref().unwrap().criteria.is_empty());
        assert_eq!(report.warnings.iter().filter(|w| w.contains("AutoFilter")).count(), 1, "{:?}", report.warnings);
    }
}
#[test]
fn date_filters_roundtrip() {
    for (body, xml) in [
        (dynamic_record(15), r#"<dynamicFilter type="thisMonth"/>"#),
        (groups_record(&date(2026, 3, 1), false, true), r#"<dateGroupItem year="2026" month="3" dateTimeGrouping="month"/>"#),
    ] {
        let (wb, _) = read(&body);
        let bytes = write_xlsx(&wb).unwrap();
        let (back, _) = read_xlsx(&bytes).unwrap();
        assert_eq!(back.sheets[0].autofilter, wb.sheets[0].autofilter);
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut sheet = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
        assert!(sheet.contains(xml), "{sheet}");
    }
}

#[test]
fn date_filter_fields_and_invalid_items() {
    let mut complete = date(2026, 3, 5);
    complete[4..8].copy_from_slice(&17u32.to_le_bytes());
    complete[8..10].copy_from_slice(&14u16.to_le_bytes());
    complete[10..12].copy_from_slice(&25u16.to_le_bytes());
    complete[12..14].copy_from_slice(&36u16.to_le_bytes());
    for (depth, grouping) in
        [DateGrouping::Year, DateGrouping::Month, DateGrouping::Day, DateGrouping::Hour, DateGrouping::Minute, DateGrouping::Second]
            .into_iter()
            .enumerate()
    {
        complete[20..24].copy_from_slice(&(depth as u32).to_le_bytes());
        let (wb, report) = read(&groups_record(&complete, false, false));
        let want = DateGroup {
            year: 2026,
            month: (depth >= 1).then_some(3),
            day: (depth >= 2).then_some(17),
            hour: (depth >= 3).then_some(14),
            minute: (depth >= 4).then_some(25),
            second: (depth >= 5).then_some(36),
            grouping,
        };
        assert_eq!(
            wb.sheets[0].autofilter.as_ref().unwrap().criteria,
            vec![(0, FilterCriterion::DateGroups { values: vec![], blanks: false, groups: vec![want] })],
            "{:?}",
            report.warnings
        );
    }
    complete[20..24].copy_from_slice(&5u32.to_le_bytes());
    for (offset, bad) in [(0, 10000u16), (0, 0), (2, 13), (4, 32), (8, 24), (10, 60), (12, 60), (20, 6)] {
        let mut invalid = complete.clone();
        invalid[offset..offset + 2].copy_from_slice(&bad.to_le_bytes());
        let (wb, report) = read(&groups_record(&invalid, false, false));
        assert!(wb.sheets[0].autofilter.as_ref().unwrap().criteria.is_empty());
        assert_eq!(report.warnings.iter().filter(|w| w.contains("AutoFilter")).count(), 1, "{:?}", report.warnings);
    }
    for len in 0..24 {
        let (wb, report) = read(&groups_record(&complete[..len], false, false));
        assert!(wb.sheets[0].autofilter.as_ref().unwrap().criteria.is_empty());
        assert_eq!(report.warnings.iter().filter(|w| w.contains("AutoFilter")).count(), 1, "{:?}", report.warnings);
    }
}

#[test]
fn all_dynamic_date_filter_codes() {
    let names = [
        "tomorrow",
        "today",
        "yesterday",
        "nextWeek",
        "thisWeek",
        "lastWeek",
        "nextMonth",
        "thisMonth",
        "lastMonth",
        "nextQuarter",
        "thisQuarter",
        "lastQuarter",
        "nextYear",
        "thisYear",
        "lastYear",
        "yearToDate",
        "Q1",
        "Q2",
        "Q3",
        "Q4",
        "M1",
        "M2",
        "M3",
        "M4",
        "M5",
        "M6",
        "M7",
        "M8",
        "M9",
        "M10",
        "M11",
        "M12",
    ];
    for (offset, kind) in names.into_iter().enumerate() {
        let (wb, report) = read(&dynamic_record(offset as u32 + 8));
        assert_eq!(wb.sheets[0].autofilter.as_ref().unwrap().criteria, vec![(0, dynamic(kind))], "{:?}", report.warnings);
    }
    let (wb, _) = read(&dynamic_record(2));
    assert_eq!(wb.sheets[0].autofilter.as_ref().unwrap().criteria, vec![(0, FilterCriterion::AboveAverage(false))]);
}

#[test]
fn invalid_tick_keeps_other_ticks_and_values() {
    let body = [
        header(0, 1, 3),
        rec(0x087f, &cat(&[&[0; 12], &[6, 2, 0, 0, 0, 0, 1, 1, 0, 0], &[0], b"x"])),
        rec(0x087f, &cat(&[&[0; 12], &date(2026, 13, 1)])),
        rec(0x087f, &cat(&[&[0; 12], &date(2026, 3, 1)])),
        rec(0x087f, &cat(&[&[0; 12], &date(0, 0, 0)])),
    ]
    .concat();
    let (wb, report) = read(&body);
    assert_eq!(wb.sheets[0].autofilter.as_ref().unwrap().criteria, vec![(0, expected_group(2026, Some(3), false, vec!["x".into()]))]);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("AutoFilter")).count(), 1);
}
#[test]
fn truncated_header_and_missing_continuations() {
    let whole = header(15, 0, 0);
    for len in 0..whole.len() - 4 {
        let (wb, report) = read(&rec(0x087e, &whole[4..4 + len]));
        assert!(wb.sheets[0].autofilter.as_ref().unwrap().criteria.is_empty());
        assert_eq!(report.warnings.iter().filter(|w| w.contains("AutoFilter")).count(), 1);
    }
    let (wb, report) = read(&header(0, 0, 1));
    assert!(wb.sheets[0].autofilter.as_ref().unwrap().criteria.is_empty());
    assert_eq!(report.warnings.iter().filter(|w| w.contains("AutoFilter")).count(), 1);
}

#[test]
fn value_list_without_dates_stays_values() {
    let text = rec(0x087f, &cat(&[&[0; 12], &[6, 2, 0, 0, 0, 0, 1, 1, 0, 0], &[0], b"x"]));
    let number = rec(0x087f, &cat(&[&[0; 12], &[4, 2], &7.5f64.to_le_bytes()]));
    let blank = rec(0x087f, &cat(&[&[0; 12], &[12, 2, 0, 0, 0, 0, 0, 0, 0, 0]]));
    let (wb, report) = read(&[header(0, 3, 0), text, number, blank].concat());
    assert_eq!(
        wb.sheets[0].autofilter.as_ref().unwrap().criteria,
        vec![(0, FilterCriterion::Values { values: vec!["x".into(), "7.5".into()], blanks: true })]
    );
    assert!(report.warnings.iter().all(|w| !w.contains("AutoFilter")));
}
