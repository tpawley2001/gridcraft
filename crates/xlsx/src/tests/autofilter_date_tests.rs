//! `.xlsx` AutoFilter date criteria (ECMA-376 Part 1 §18.3.2): `dateGroupItem` ticks inside
//! `filters` and `dynamicFilter` date types are read, kept and written back.

use gridcraft_model::{AutoFilter, DateGroup, DateGrouping, FilterCriterion};

use crate::tests::minimal;
use crate::{read_xlsx, write_xlsx};

fn read(column: &str) -> (FilterCriterion, Vec<String>) {
    let body = format!(r#"<sheetData/><autoFilter ref="A1:A5"><filterColumn colId="0">{column}</filterColumn></autoFilter>"#);
    let (wb, report) = read_xlsx(&minimal(&body, &[], "", "")).unwrap();
    let af: AutoFilter = wb.sheets[0].autofilter.clone().expect("the sheet has an AutoFilter");
    assert_eq!(af.criteria.len(), 1, "{:?}", report.warnings);
    (af.criteria[0].1.clone(), report.warnings)
}

/// Writes and re-reads a workbook holding `column`'s criterion; returns it and the sheet XML.
fn round_trip(column: &str) -> (FilterCriterion, FilterCriterion, String) {
    let body = format!(r#"<sheetData/><autoFilter ref="A1:A5"><filterColumn colId="0">{column}</filterColumn></autoFilter>"#);
    let (wb, _) = read_xlsx(&minimal(&body, &[], "", "")).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    let before = wb.sheets[0].autofilter.clone().unwrap().criteria[0].1.clone();
    let after = back.sheets[0].autofilter.clone().unwrap().criteria[0].1.clone();
    (before, after, sheet)
}

fn group(year: u16, month: Option<u8>, grouping: DateGrouping) -> DateGroup {
    DateGroup { year, month, day: None, hour: None, minute: None, second: None, grouping }
}

const CASES: &[&str] = &[
    r#"<filters><dateGroupItem year="2024" month="3" dateTimeGrouping="month"/></filters>"#,
    r#"<filters blank="1"><filter val="x"/><dateGroupItem year="2023" dateTimeGrouping="year"/></filters>"#,
    r#"<filters><filter val="a"/></filters>"#,
    r#"<filters><dateGroupItem year="2024" month="3" day="20" hour="13" minute="5" second="59" dateTimeGrouping="second"/></filters>"#,
    r#"<dynamicFilter type="thisMonth"/>"#,
    r#"<dynamicFilter type="today" val="45366" maxVal="45367" valIso="2024-03-15T00:00:00" maxValIso="2024-03-16T00:00:00"/>"#,
    r#"<dynamicFilter type="aboveAverage"/>"#,
];

#[test]
fn date_group_month() {
    let (c, _) = read(CASES[0]);
    assert_eq!(c, FilterCriterion::DateGroups { values: vec![], blanks: false, groups: vec![group(2024, Some(3), DateGrouping::Month)] });
}

#[test]
fn date_group_with_values_and_blanks() {
    let (c, _) = read(CASES[1]);
    assert_eq!(c, FilterCriterion::DateGroups { values: vec!["x".into()], blanks: true, groups: vec![group(2023, None, DateGrouping::Year)] });
}

#[test]
fn plain_values_are_unchanged() {
    let (c, _) = read(CASES[2]);
    assert_eq!(c, FilterCriterion::Values { values: vec!["a".into()], blanks: false });
}

#[test]
fn dynamic_date_without_values() {
    let (c, w) = read(CASES[4]);
    assert_eq!(c, FilterCriterion::DynamicDate { kind: "thisMonth".into(), val: None, max_val: None, val_iso: None, max_val_iso: None });
    assert!(!w.iter().any(|w| w.contains("filter")), "{w:?}");
}

#[test]
fn dynamic_date_keeps_every_field() {
    let (c, _) = read(CASES[5]);
    assert_eq!(
        c,
        FilterCriterion::DynamicDate {
            kind: "today".into(),
            val: Some(45366.0),
            max_val: Some(45367.0),
            val_iso: Some("2024-03-15T00:00:00".into()),
            max_val_iso: Some("2024-03-16T00:00:00".into()),
        }
    );
}

#[test]
fn above_average_is_unchanged() {
    let (c, _) = read(CASES[6]);
    assert_eq!(c, FilterCriterion::AboveAverage(true));
}

#[test]
fn date_group_without_year_is_skipped_with_one_warning() {
    let (c, w) = read(
        r#"<filters><dateGroupItem month="3" dateTimeGrouping="month"/><dateGroupItem year="2024" dateTimeGrouping="fortnight"/><dateGroupItem year="x" dateTimeGrouping="year"/><dateGroupItem year="2022" dateTimeGrouping="year"/></filters>"#,
    );
    assert_eq!(c, FilterCriterion::DateGroups { values: vec![], blanks: false, groups: vec![group(2022, None, DateGrouping::Year)] });
    assert_eq!(w.iter().filter(|w| w.contains("date-group")).count(), 1, "{w:?}");
}

#[test]
fn unknown_dynamic_type_is_dropped_with_a_warning() {
    let body = r#"<sheetData/><autoFilter ref="A1:A5"><filterColumn colId="0"><dynamicFilter type="nextDecade"/></filterColumn></autoFilter>"#;
    let (wb, report) = read_xlsx(&minimal(body, &[], "", "")).unwrap();
    assert!(wb.sheets[0].autofilter.as_ref().unwrap().criteria.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("not supported")), "{:?}", report.warnings);
}

#[test]
fn every_case_round_trips() {
    for case in CASES {
        let (before, after, sheet) = round_trip(case);
        assert_eq!(before, after, "{case}\n{sheet}");
        if let (Some(f), Some(d)) = (sheet.find("<filter "), sheet.find("<dateGroupItem")) {
            assert!(f < d, "filter before dateGroupItem: {sheet}");
        }
    }
    let (_, _, sheet) = round_trip(CASES[1]);
    assert!(sheet.contains(r#"<filters blank="1"><filter val="x"/><dateGroupItem year="2023" dateTimeGrouping="year"/></filters>"#), "{sheet}");
    let (_, _, sheet) = round_trip(CASES[4]);
    assert!(sheet.contains(r#"<dynamicFilter type="thisMonth"/>"#), "{sheet}");
}
