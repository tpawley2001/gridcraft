//! VLOOKUP/HLOOKUP search before they reject a column/row index below 1: a value that isn't
//! found gives #N/A, as Excel 365 does for the same sheets.

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
fn vlookup_not_found_with_index_zero_is_na() {
    let mut t = T::new();
    t.set("B1", "1");
    t.set("C1", "0");
    // D1 stays empty.
    t.set("A5", "=VLOOKUP(D1,B1:C1,0)");
    assert_eq!(t.get("A5"), Value::Error(CellError::NA), "Excel: #N/A");
}

#[test]
fn vlookup_empty_one_cell_table_empty_index_is_na() {
    let mut t = T::new();
    // B1 stays empty.
    t.set("A7", "=VLOOKUP(B1,B1,B1,3)");
    assert_eq!(t.get("A7"), Value::Error(CellError::NA), "Excel: #N/A");
}

#[test]
fn vlookup_found_with_index_below_one_stays_value_error() {
    let mut t = T::new();
    for (at, v) in [("J1", "0"), ("K1", "rZero"), ("J2", "5"), ("K2", "r0"), ("J3", "45"), ("K3", "r3")] {
        t.set(at, v);
    }
    // L1 stays empty: an index of 0, and 0 is found in J1.
    t.set("A1", "=VLOOKUP(0,J1:K3,L1,TRUE)");
    t.set("A2", "=VLOOKUP(45,J1:K3,2,FALSE)");
    assert_eq!(t.get("A1"), Value::Error(CellError::Value), "Excel: #VALUE!");
    assert_eq!(t.get("A2"), Value::Text("r3".into()), "Excel: r3");
}
