//! SUBTOTAL 3 / 103 (COUNTA) counts cells that hold an error instead of returning the error,
//! with the result Excel 365 gives for the same sheet.

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

#[test]
fn subtotal_3_counts_an_error_cell_between_blanks() {
    // Excel: D3, D4 blank, D5 #REF!, =SUBTOTAL(3,$D$3:D5)-1 is 0.
    let mut t = T::new();
    t.set("A3", "=INDEX(J1:J3,9)");
    t.set("B1", "=SUBTOTAL(3,A1:A3)");
    t.set("B2", "=SUBTOTAL(3,$A$1:A3)-1");
    t.set("B3", "=SUBTOTAL(103,A1:A3)");
    assert_eq!(t.get("A3"), Value::Error(CellError::Ref));
    assert_eq!(t.get("B1"), Value::Number(1.0));
    assert_eq!(t.get("B2"), Value::Number(0.0));
    assert_eq!(t.get("B3"), Value::Number(1.0));
}

#[test]
fn subtotal_3_counts_text_errors_and_numbers() {
    let mut t = T::new();
    t.set("A1", "x");
    t.set("A2", "=1/0");
    t.set("A3", "5");
    t.set("B1", "=SUBTOTAL(3,A1:A3)");
    t.set("B2", "=COUNTA(A1:A3)");
    assert_eq!(t.get("B1"), Value::Number(3.0));
    assert_eq!(t.get("B2"), Value::Number(3.0));
}

#[test]
fn subtotal_9_still_returns_the_error() {
    let mut t = T::new();
    t.set("A1", "x");
    t.set("A2", "=1/0");
    t.set("A3", "5");
    t.set("B1", "=SUBTOTAL(9,A1:A3)");
    assert_eq!(t.get("B1"), Value::Error(CellError::Div0));
}
