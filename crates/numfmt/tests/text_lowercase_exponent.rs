//! `TEXT(value, format)` with a lowercase `e+`/`e-` exponent code: Microsoft 365 Excel returns
//! `#VALUE!` (measured with a full recalculation; see also `poi_number_format_approx.rs`). Uppercase
//! `E+`/`E-` and cell display formats are unaffected.

use gridcraft_core::{CellError, DateSystem, Value};
use gridcraft_numfmt::{NumberFormat, format_value, text_function};

fn text(v: Value, code: &str) -> Result<String, CellError> {
    text_function(&v, code, DateSystem::D1900)
}

#[test]
fn lowercase_exponent_codes_are_value_errors() {
    // (value, format) pairs measured in Excel 365: every one is #VALUE!.
    let cases: &[(f64, &str)] = &[
        (123_456.789, "|#.#|e-|#|"),
        (123_456.789, "|0|e+|0|"),
        (123_456.789, "|#,e-#|"),
        (123_456.789, "|?.?|e+|?|"),
        (0.0, "|#.#|e+|?|"),
        (-1.234_567_89e-5, "|#|e+|#|"),
        (1.234_567_8e142, "|#.#|e+|00000|"),
        (1.234_567_8e-142, "|#.#|e+|000,00|"),
        (314_159.0 / 100_000.0, "#,##0.00e+00"),
    ];
    for &(v, code) in cases {
        assert_eq!(text(Value::Number(v), code), Err(CellError::Value), "TEXT({v}, {code:?})");
    }
}

#[test]
fn uppercase_exponent_codes_still_format() {
    // Excel 365's answers for the same values.
    let cases: &[(f64, &str, &str)] = &[
        (123_456.789, "|#.#|E-|#|", "|1.2|E|5|"),
        (123_456.789, "|#.#|E+|#|", "|1.2|E|+5|"),
        (-1.234_567_89e-5, "|#.#|E-|#|", "-|1.2|E|-5|"),
        (0.0, "|#.#|E+|#|", "|0.|E|+0|"),
    ];
    for &(v, code, want) in cases {
        assert_eq!(text(Value::Number(v), code).as_deref(), Ok(want), "TEXT({v}, {code:?})");
    }
}

#[test]
fn quoted_or_escaped_e_is_literal_text() {
    // Scope of the rule, not an Excel measurement: a quoted or backslash-escaped `e` is literal text,
    // so these formats stay valid.
    assert!(text(Value::Number(1.5), "0.0\" e+\"").is_ok());
    assert!(text(Value::Number(1.5), "0.0\\e+").is_ok());
}

#[test]
fn an_error_argument_wins_over_an_invalid_format() {
    assert_eq!(text(Value::Error(CellError::NA), "0.0e+0"), Err(CellError::NA));
}

#[test]
fn cell_display_formats_are_unchanged() {
    // Only TEXT() rejects the code; a cell formatted `0.00e+00` still shows its number.
    let f = format_value(&Value::Number(314_159.0 / 100_000.0), &NumberFormat::parse("0.00e+00"), DateSystem::D1900);
    assert!(f.text.contains("3.14"), "display {:?}", f.text);
}
