//! AMORLINC / AMORDEGRC with basis 2 (actual/360) are #NUM! in Microsoft Excel; other bases stay numbers.
use crate::Arg;
use crate::util::testutil::*;
use gridcraft_core::{CellError, DateSystem, Value, date::serial_from_ymd};

fn d(y: i64, m: i64, dd: i64) -> Arg {
    n(serial_from_ymd(DateSystem::D1900, y, m, dd).unwrap())
}

#[test]
fn amor_basis_2_is_num_error() {
    let starts = [(2008, 2, 15), (2008, 8, 19), (2023, 1, 31), (2024, 2, 29), (2025, 5, 15), (2026, 11, 11)];
    for name in ["AMORLINC", "AMORDEGRC"] {
        for (y, m, dd) in starts {
            let args = |basis: f64| vec![n(2400.0), d(y, m, dd), d(y, 12, 31), n(300.0), n(1.0), n(0.15), n(basis)];
            is_err(ev(name, args(2.0)), CellError::Num);
            for basis in [0.0, 1.0, 3.0, 4.0] {
                assert!(matches!(ev(name, args(basis)), Value::Number(_)), "{name} basis {basis} start {y}-{m}-{dd}");
            }
        }
    }
}

/// In bases 1 and 3 a purchase on 29 February has the same first period as one on 28 February (Excel).
#[test]
fn amor_feb_29_purchase_counts_as_feb_28() {
    for name in ["AMORLINC", "AMORDEGRC"] {
        for basis in [1.0, 3.0] {
            let args = |dd: i64| vec![n(2400.0), d(2024, 2, dd), d(2024, 12, 31), n(300.0), n(0.0), n(0.15), n(basis)];
            let (Value::Number(a), Value::Number(b)) = (ev(name, args(29)), ev(name, args(28))) else {
                panic!("{name} basis {basis} not a number");
            };
            assert_eq!(a, b, "{name} basis {basis}");
        }
    }
    // A zero-length first period stays zero.
    let same = vec![n(2400.0), d(2024, 2, 29), d(2024, 2, 29), n(300.0), n(0.0), n(0.15), n(1.0)];
    assert!(matches!(ev("AMORLINC", same), Value::Number(x) if x == 0.0));
}
