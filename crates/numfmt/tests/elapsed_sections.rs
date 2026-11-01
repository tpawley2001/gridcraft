use gridcraft_core::{DateSystem, Value};
use gridcraft_numfmt::text_function;

#[test]
fn mixed_elapsed_units_keep_section_scope_width_and_rounding() {
    for (code, value, want) in [
        ("ss\" then \"mm", 2.085_462_962_963, "04 then 03"),
        ("[s]:m", 2.085_462_962_963, "180184:3"),
        ("[m]:mm:ss", 1.5, "2160:2160:00"),
        ("hh [hhhh]", 1.5, "36 0036"),
        ("ss [s].000", 1.999_999_999_9, "172800 172800.000"),
        ("h [h]:mm:ss.000", 1.999_999_999_9, "48 48:00:00.000"),
        ("[>=2][h] h;h", 1.5, "12"),
        ("ss:mm:ss", 2.085_462_962_963, "04:03:04"),
        ("s:mmm", 2.085_462_962_963, "4:Jan"),
    ] {
        assert_eq!(text_function(&Value::Number(value), code, DateSystem::D1900), Ok(want.to_string()), "{code}");
    }
}
