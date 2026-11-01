//! `TEXT()` of negative values with elapsed-time formats, against Microsoft 365 Excel's answers:
//! Excel formats the absolute value and puts a minus sign in front.

use gridcraft_core::{DateSystem, Value};
use gridcraft_numfmt::text_function;

/// 2 h 3 min 4 s as a fraction of a day.
const SHORT: f64 = -7384.0 / 86400.0;
/// 3.14159 days (75:23:53.376); a workbook value, not pi.
const LONG: f64 = -314_159.0 / 100_000.0;

/// (value, format, Excel's result).
const CASES: &[(f64, &str, &str)] = &[
    (SHORT, "[h]:m:s.000", "-2:3:4.000"),
    (SHORT, "[m]:s.000", "-123:4.000"),
    (SHORT, "[s].000", "-7384.000"),
    (SHORT, "[hh]:mm:ss.0", "-02:03:04.0"),
    (SHORT, "[mm]:ss.0", "-123:04.0"),
    (SHORT, "[ss].000", "-7384.000"),
    (LONG, "[h]:m:s.000", "-75:23:53.376"),
    (LONG, "[m]:s.000", "-4523:53.376"),
    (LONG, "[s].000", "-271433.376"),
    (LONG, "[hh]:mm:ss.0", "-75:23:53.4"),
    (LONG, "[mm]:ss.0", "-4523:53.4"),
    (LONG, "[ss].000", "-271433.376"),
];

#[test]
fn negative_elapsed_times_get_a_leading_minus() {
    let mut bad = Vec::new();
    for &(v, code, want) in CASES {
        let got = text_function(&Value::Number(v), code, DateSystem::D1900);
        if got.as_deref() != Ok(want) {
            bad.push(format!("TEXT({v}, {code:?}): got {got:?}, Excel {want:?}"));
        }
    }
    assert!(bad.is_empty(), "{} of {} rows differ:\n{}", bad.len(), CASES.len(), bad.join("\n"));
}

#[test]
fn negative_values_with_clock_and_date_formats_stay_errors() {
    for code in ["h:mm:ss", "yyyy-mm-dd", "[h]:mm d"] {
        let got = text_function(&Value::Number(SHORT), code, DateSystem::D1900);
        assert!(got.is_err(), "TEXT({SHORT}, {code:?}) should stay an error, got {got:?}");
    }
}
