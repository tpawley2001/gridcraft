//! Shared AutoFilter encodings from the public MS-XLS AutoFilter12 and MS-XLSB
//! BrtDynamicFilter / BrtAFilterDateGroupItem specifications.
//!
//! Sources: <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/8238f9cb-9797-4cff-9b8a-180d2b025c31>
//! and <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/664f9842-4f82-4275-ae01-fb86271ab4f0>.

use gridcraft_model::{DateGroup, DateGrouping, FilterCriterion};

/// Both binary formats use the AutoFilter12 cft table (also BrtDynamicFilter.cft).
pub(crate) fn dynamic(cft: u32, bounds: Option<(f64, f64)>) -> Option<FilterCriterion> {
    let kind = match cft {
        1 | 2 => return Some(FilterCriterion::AboveAverage(cft == 1)),
        0x08 => "tomorrow",
        0x09 => "today",
        0x0a => "yesterday",
        0x0b => "nextWeek",
        0x0c => "thisWeek",
        0x0d => "lastWeek",
        0x0e => "nextMonth",
        0x0f => "thisMonth",
        0x10 => "lastMonth",
        0x11 => "nextQuarter",
        0x12 => "thisQuarter",
        0x13 => "lastQuarter",
        0x14 => "nextYear",
        0x15 => "thisYear",
        0x16 => "lastYear",
        0x17 => "yearToDate",
        0x18 => "Q1",
        0x19 => "Q2",
        0x1a => "Q3",
        0x1b => "Q4",
        0x1c => "M1",
        0x1d => "M2",
        0x1e => "M3",
        0x1f => "M4",
        0x20 => "M5",
        0x21 => "M6",
        0x22 => "M7",
        0x23 => "M8",
        0x24 => "M9",
        0x25 => "M10",
        0x26 => "M11",
        0x27 => "M12",
        _ => return None,
    };
    let bounds = bounds.filter(|(lo, hi)| (0x08..=0x17).contains(&cft) && lo.is_finite() && hi.is_finite());
    Some(FilterCriterion::DynamicDate {
        kind: kind.into(),
        val: bounds.map(|(lo, _)| lo),
        max_val: bounds.map(|(_, hi)| hi),
        val_iso: None,
        max_val_iso: None,
    })
}

/// AF12DateInfo and BrtAFilterDateGroupItem share the fields and grouping numbers.
pub(crate) fn date_group(year: u16, month: u16, day: u32, time: [u16; 3], depth: u32) -> Option<DateGroup> {
    let [hour, minute, second] = time;
    let grouping = match depth {
        0 => DateGrouping::Year,
        1 => DateGrouping::Month,
        2 => DateGrouping::Day,
        3 => DateGrouping::Hour,
        4 => DateGrouping::Minute,
        5 => DateGrouping::Second,
        _ => return None,
    };
    if !(1..=9999).contains(&year)
        || month > 12
        || day > 31
        || hour > 23
        || minute > 59
        || second > 59
        || (depth >= 1 && month == 0)
        || (depth >= 2 && day == 0)
    {
        return None;
    }
    Some(DateGroup {
        year,
        month: (depth >= 1).then_some(month as u8),
        day: (depth >= 2).then_some(day as u8),
        hour: (depth >= 3).then_some(hour as u8),
        minute: (depth >= 4).then_some(minute as u8),
        second: (depth >= 5).then_some(second as u8),
        grouping,
    })
}

/// Promote a values filter only after a valid date tick has been read.
pub(crate) fn add_group(criterion: &mut FilterCriterion, group: DateGroup) {
    if let FilterCriterion::Values { values, blanks } = criterion {
        *criterion = FilterCriterion::DateGroups { values: std::mem::take(values), blanks: *blanks, groups: vec![] };
    }
    if let FilterCriterion::DateGroups { groups, .. } = criterion {
        groups.push(group);
    }
}
