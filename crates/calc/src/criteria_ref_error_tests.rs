//! The criteria functions take references for their range arguments: a `#REF!` written in the
//! formula (left behind when the referenced cells were deleted) is the result, as in Excel 365.
//! SUMIF and COUNTIF are measured on a real Excel 365 workbook (`=SUMIF(#REF!,">0")` saved as
//! `#REF!`); the other criteria functions follow the same reference-argument rule.

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_model::{Cell, Formula, Workbook};

use crate::Calc;

struct T {
    wb: Workbook,
    calc: Calc,
}

impl T {
    fn new() -> T {
        T { wb: Workbook::new(), calc: Calc::new() }
    }
    fn set(&mut self, at: &str, input: &str) {
        let cell = if input.starts_with('=') {
            Cell::formula(Formula::new(input))
        } else {
            Cell::value(gridcraft_core::parse::parse_input(input, self.wb.date_system).value)
        };
        let at = CellRef::parse(at).unwrap();
        self.wb.sheet_mut(0).unwrap().cells.set(at, cell);
        self.calc.cells_changed(&mut self.wb, &[(0, at)]);
    }
    fn get(&self, at: &str) -> Value {
        self.wb.sheet(0).unwrap().value(CellRef::parse(at).unwrap())
    }
}

fn sheet() -> T {
    let mut t = T::new();
    t.set("A1", "1");
    t.set("A2", "=1/0");
    t.set("A3", "3");
    t.set("B1", "10");
    t.set("B2", "20");
    t.set("B3", "30");
    t
}

#[test]
fn a_ref_error_range_argument_is_the_result() {
    let mut t = sheet();
    let cases = [
        ("D1", "=SUMIF(#REF!,\">0\")"),
        ("D2", "=COUNTIF(#REF!,\">0\")"),
        ("D3", "=SUMIF(A1:A3,\">0\",#REF!)"),
        ("D4", "=SUMIFS(#REF!,A1:A3,\">0\")"),
        ("D5", "=SUMIFS(B1:B3,#REF!,\">0\")"),
        ("D6", "=COUNTIFS(#REF!,\">0\")"),
        ("D7", "=AVERAGEIF(#REF!,\">0\")"),
        ("D8", "=AVERAGEIFS(B1:B3,#REF!,\">0\")"),
        ("D9", "=MAXIFS(B1:B3,#REF!,\">0\")"),
        ("D10", "=MINIFS(#REF!,A1:A3,\">0\")"),
    ];
    for (at, f) in cases {
        t.set(at, f);
    }
    for (at, f) in cases {
        assert_eq!(t.get(at), Value::Error(CellError::Ref), "{at} {f}");
    }
}

#[test]
fn error_cells_inside_a_real_range_keep_todays_results() {
    let mut t = sheet();
    t.set("E1", "=COUNTIF(A1:A3,\">0\")");
    t.set("E2", "=SUMIF(A1:A3,\">0\",B1:B3)");
    t.set("E3", "=COUNTIFS(A1:A3,\">0\",B1:B3,\">15\")");
    assert_eq!(t.get("E1"), Value::number(2.0));
    assert_eq!(t.get("E2"), Value::number(40.0));
    assert_eq!(t.get("E3"), Value::number(1.0));
}
