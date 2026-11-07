//! Typed text "mmm-nn" (a month name, a separator, then a one- or two-digit number) is a day in
//! the current year when the number is a valid day of that month, as Microsoft Excel reads it:
//! `=DATEVALUE("FEB-01")` is 1 February of the current year (Excel's own recalculation of a
//! workbook made this way returned the serial of 1 Feb of the year it ran in), not 1 February
//! 2001. Forms whose number cannot be a day keep their month-and-year meaning.

use gridcraft_core::DateSystem;
use gridcraft_core::date::serial_from_ymd;
use gridcraft_core::parse::{current_year, parse_input};
use gridcraft_core::value::Value;

fn parsed(text: &str) -> (f64, Option<&'static str>) {
    let p = parse_input(text, DateSystem::D1900);
    match p.value {
        Value::Number(n) => (n, p.format),
        other => panic!("{text:?} should read as a date, got {other:?}"),
    }
}

fn ymd(y: i64, m: i64, d: i64) -> f64 {
    serial_from_ymd(DateSystem::D1900, y, m, d).expect("valid date")
}

#[test]
fn month_name_then_valid_day_is_this_year() {
    let y = current_year();
    let cases: &[(&str, i64, i64)] =
        &[("FEB-01", 2, 1), ("Feb-01", 2, 1), ("feb-1", 2, 1), ("Jan-31", 1, 31), ("December-25", 12, 25), ("Jun-29", 6, 29)];
    for &(text, m, d) in cases {
        let (n, format) = parsed(text);
        assert_eq!(n, ymd(y, m, d), "{text:?}");
        assert_eq!(format, Some("d-mmm"), "{text:?}");
    }
}

#[test]
fn month_name_then_year_still_reads_as_month_and_year() {
    let cases: &[(&str, i64, i64)] = &[("Mar-45", 1945, 3), ("Mar-2026", 2026, 3), ("Oct-99", 1999, 10), ("Jun-1999", 1999, 6)];
    for &(text, y, m) in cases {
        let (n, format) = parsed(text);
        assert_eq!(n, ymd(y, m, 1), "{text:?}");
        assert_eq!(format, Some("mmm-yy"), "{text:?}");
    }
}

#[test]
fn day_then_month_name_is_unchanged() {
    let y = current_year();
    let (n, format) = parsed("1-Feb");
    assert_eq!(n, ymd(y, 2, 1));
    assert_eq!(format, Some("d-mmm"));
}
