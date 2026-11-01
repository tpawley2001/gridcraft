//! Elapsed-time codes (`[h]`, `[m]`, `[s]`) mixed with plain codes in one section, against the
//! results Excel cached in Apache POI's `test-data/spreadsheet/ElapsedFormatTests.xlsx`
//! (sheet `Tests`: B = format, C = value, A = `=TEXT(C, B)`). The workbook is not committed.

use gridcraft_core::{DateSystem, Value};
use gridcraft_numfmt::text_function;

/// (format, value, Excel's result).
const CASES: &[(&str, f64, &str)] = &[
    ("[h]:m:s.000", 3.14159, "75:23:53.376"),
    ("[m]:s.000", 3.14159, "4523:53.376"),
    ("[s].000", 3.14159, "271433.376"),
    ("[hh]:mm:ss.0", 3.14159, "75:23:53.4"),
    ("[mm]:ss.0", 3.14159, "4523:53.4"),
    ("[ss].000", 3.14159, "271433.376"),
    ("s:m\" @ hour \"[hh]", 3.14159, "53:23 @ hour 75"),
    ("s\" secs and \"[m]\" mins\"", 3.14159, "53 secs and 4523 mins"),
    ("\"It was \"[h]\" [yes, \"h\"] hours and \"mm:ss", 3.14159, "It was 75 [yes, 75] hours and 23:53"),
    ("[s]\" [yes, \"ss\"] seconds\"", 3.14159, "271433 [yes, 271433] seconds"),
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
