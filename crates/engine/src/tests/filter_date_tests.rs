//! AutoFilter date criteria: date-group ticks are evaluated against date serials, dynamic date
//! filters kept from files leave rows as they are.

use gridcraft_model::{DateGroup, DateGrouping, FilterCriterion};
use serde_json::json;

use super::s;
use crate::Session;

/// A1 header; A2..A5 = 2024-03-15, 2024-03-20, 2024-04-01, 2023-03-15 (1900 date system).
fn dates() -> Session {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A1", "values": [["D"], [45366], [45371], [45383], [45000]]})).unwrap();
    s.execute("selection.set", json!({"cell": "A1"})).unwrap();
    s.execute("data.filter", json!({})).unwrap();
    s
}

fn group(year: u16, month: Option<u8>, day: Option<u8>, grouping: DateGrouping) -> DateGroup {
    DateGroup { year, month, day, hour: None, minute: None, second: None, grouping }
}

/// Applies `crit` to column A and returns the hidden 0-based rows.
fn hidden_with(s: &mut Session, crit: FilterCriterion) -> Vec<u32> {
    let sh = std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().wb).sheet_mut(0).unwrap();
    sh.autofilter.as_mut().unwrap().criteria = vec![(0, crit)];
    s.execute("data.reapply", json!({})).unwrap();
    s.active().unwrap().wb.sheet(0).unwrap().rows.iter().filter(|(_, r)| r.hidden).map(|(k, _)| *k).collect()
}

fn groups(g: DateGroup) -> FilterCriterion {
    FilterCriterion::DateGroups { values: vec![], blanks: false, groups: vec![g] }
}

#[test]
fn month_group_keeps_that_month() {
    let mut s = dates();
    // Rows 4 and 5 (1-based) are 0-based rows 3 and 4.
    assert_eq!(hidden_with(&mut s, groups(group(2024, Some(3), None, DateGrouping::Month))), vec![3, 4]);
}

#[test]
fn day_group_keeps_that_day() {
    let mut s = dates();
    assert_eq!(hidden_with(&mut s, groups(group(2024, Some(3), Some(20), DateGrouping::Day))), vec![1, 3, 4]);
}

#[test]
fn year_group_keeps_that_year() {
    let mut s = dates();
    assert_eq!(hidden_with(&mut s, groups(group(2023, None, None, DateGrouping::Year))), vec![1, 2, 3]);
}

#[test]
fn dynamic_date_filter_leaves_rows_as_they_are() {
    let mut s = dates();
    assert_eq!(hidden_with(&mut s, groups(group(2023, None, None, DateGrouping::Year))), vec![1, 2, 3]);
    let dynamic = FilterCriterion::DynamicDate { kind: "thisMonth".into(), val: None, max_val: None, val_iso: None, max_val_iso: None };
    assert_eq!(hidden_with(&mut s, dynamic), vec![1, 2, 3]);
}

#[test]
fn month_group_missing_its_month_matches_nothing() {
    let mut s = dates();
    assert_eq!(hidden_with(&mut s, groups(group(2024, None, None, DateGrouping::Month))), vec![1, 2, 3, 4]);
}
