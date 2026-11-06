//! IF with an omitted result argument (`IF(c,,"x")`, `IF(c,"x",)`) returns an empty value, like
//! Excel: text functions join it as "", while a cell showing the IF result itself shows 0.

use gridcraft_core::{CellRef, Value};
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
fn concatenate_joins_an_omitted_if_branch_as_empty_text() {
    // Excel 365, real workbook: F=1, G=0, H=2, I=0, J=0 gives "TOC Clay ".
    let mut t = T::new();
    for (at, v) in [("F1", "1"), ("G1", "0"), ("H1", "2"), ("I1", "0"), ("J1", "0")] {
        t.set(at, v);
    }
    t.set("L1", r#"=CONCATENATE(IF(F1=0,,"TOC "),IF(G1=0,,"TIC "),IF(H1=0,,"Clay "),IF(I1=0,,"Iron "),IF(J1=0,,"pH"))"#);
    assert_eq!(t.get("L1"), Value::Text("TOC Clay ".into()));
    t.set("A2", r#"=CONCATENATE("a",IF(1=2,"x",),"b")"#);
    assert_eq!(t.get("A2"), Value::Text("ab".into()));
}

#[test]
fn an_omitted_if_branch_shown_on_its_own_is_zero() {
    let mut t = T::new();
    t.set("A1", r#"=IF(1=1,,"x")"#);
    t.set("A2", r#"=IF(1=2,"x",)"#);
    t.set("A3", r#"=IF(1=1,,"x")+5"#);
    assert_eq!(t.get("A1"), Value::Number(0.0));
    assert_eq!(t.get("A2"), Value::Number(0.0));
    assert_eq!(t.get("A3"), Value::Number(5.0));
}

#[test]
fn a_given_if_branch_is_unchanged() {
    let mut t = T::new();
    t.set("A1", r#"=IF(1=1,"y","x")"#);
    assert_eq!(t.get("A1"), Value::Text("y".into()));
}
