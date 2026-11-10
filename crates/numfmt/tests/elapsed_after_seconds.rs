//! An elapsed hours/minutes code (`[h]`, `[m]`) that follows a plain seconds code (`s`) in the
//! same section. Excel 365 shows the elapsed total of (seconds-of-the-minute + 0.5 days), not of
//! the value: `TEXT(3.14159, "s:m"" @ hour ""[hh]")` = `53:23 @ hour 1293`. Expected results are
//! Excel 365's cached answers after a full recalculation; no workbook is committed.

use gridcraft_core::{DateSystem, Value};
use gridcraft_numfmt::text_function;

/// 3.14159 days (271433.376 seconds).
const V: f64 = 314_159.0 / 100_000.0;
/// 2 hours, 3 minutes, 4 seconds.
const T: f64 = 2.0 / 24.0 + 3.0 / 1440.0 + 4.0 / 86400.0;

const AT_HOUR: &str = "s:m\" @ hour \"[hh]";
const SECS_MINS: &str = "s\" secs and \"[m]\" mins\"";

/// (format, value, Excel's result).
const CASES: &[(&str, f64, &str)] = &[
    (AT_HOUR, V, "53:23 @ hour 1293"),
    (SECS_MINS, V, "53 secs and 77581 mins"),
    (AT_HOUR, T, "4:3 @ hour 108"),
    (SECS_MINS, T, "4 secs and 6480 mins"),
    (AT_HOUR, 0.0, "0:0 @ hour 12"),
    (SECS_MINS, 0.0, "0 secs and 720 mins"),
    (AT_HOUR, -T, "-4:3 @ hour 108"),
    (SECS_MINS, -T, "-4 secs and 6480 mins"),
    (AT_HOUR, -V, "-53:23 @ hour 1293"),
    (SECS_MINS, -V, "-53 secs and 77581 mins"),
    // An elapsed code before the seconds code is the plain elapsed total (unchanged).
    ("[h]:m:s.000", V, "75:23:53.376"),
    ("[m]:s.000", T, "123:4.000"),
];

#[test]
fn elapsed_code_after_seconds_matches_excel() {
    let mut bad = Vec::new();
    for &(code, v, want) in CASES {
        let got = text_function(&Value::Number(v), code, DateSystem::D1900).unwrap_or_else(|e| format!("{e:?}"));
        if got != want {
            bad.push(format!("TEXT({v}, {code:?}) = {got:?}, Excel {want:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
