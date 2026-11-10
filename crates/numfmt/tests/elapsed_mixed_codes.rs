//! Elapsed-time codes (`[h]`, `[m]`, `[s]`) mixed with plain codes in one section, against the
//! results Excel cached in Apache POI's `test-data/spreadsheet/ElapsedFormatTests.xlsx`
//! (sheet `Tests`: B = format, C = value, A = `=TEXT(C, B)`), recalculated in Excel 365 (the `s` ... `[hh]`
//! and `s` ... `[m]` rows changed there; see `elapsed_after_seconds.rs`). The workbook is not committed.

use gridcraft_core::{DateSystem, Value};
use gridcraft_numfmt::text_function;

/// The workbook's value 3.14159 (days), not an approximation of pi.
const V: f64 = 314_159.0 / 100_000.0;

/// (format, value, Excel's result).
const CASES: &[(&str, f64, &str)] = &[
    ("[h]:m:s.000", V, "75:23:53.376"),
    ("[m]:s.000", V, "4523:53.376"),
    ("[s].000", V, "271433.376"),
    ("[hh]:mm:ss.0", V, "75:23:53.4"),
    ("[mm]:ss.0", V, "4523:53.4"),
    ("[ss].000", V, "271433.376"),
    ("s:m\" @ hour \"[hh]", V, "53:23 @ hour 1293"),
    ("s\" secs and \"[m]\" mins\"", V, "53 secs and 77581 mins"),
    ("\"It was \"[h]\" [yes, \"h\"] hours and \"mm:ss", V, "It was 75 [yes, 75] hours and 23:53"),
    ("[s]\" [yes, \"ss\"] seconds\"", V, "271433 [yes, 271433] seconds"),
    ("[h]:m:s.000", 0.085_462_962_962_963, "2:3:4.000"),
];

#[test]
fn elapsed_codes_match_excel() {
    let mut bad = Vec::new();
    for &(code, v, want) in CASES {
        let got = text_function(&Value::Number(v), code, DateSystem::D1900).unwrap_or_else(|e| format!("{e:?}"));
        if got != want {
            bad.push(format!("TEXT({v}, {code:?}) = {got:?}, Excel {want:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
