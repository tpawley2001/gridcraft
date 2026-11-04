//! GROUPBY with `field_headers` omitted: a detected header row (text over numbers) is dropped
//! from the data and not shown in the result, as Microsoft 365 Excel does. Explicit modes keep
//! their meaning (3 = headers present and shown).

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, Formula, Workbook};

use crate::Calc;

fn c(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

struct T {
    wb: Workbook,
    calc: Calc,
}

impl T {
    /// B2:C2 headers ("Project type", "Revenue"), B3:C7 five projects.
    fn new() -> T {
        let mut wb = Workbook::new();
        let sheet = wb.sheet_mut(0).unwrap();
        sheet.set_value(c("B2"), Value::from("Project type"));
        sheet.set_value(c("C2"), Value::from("Revenue"));
        let rows = [("Design", 60000.0), ("Research", 86000.0), ("Development", 73000.0), ("Design", 32000.0), ("Marketing", 10000.0)];
        for (i, (kind, amount)) in rows.iter().enumerate() {
            sheet.set_value(c(&format!("B{}", i + 3)), Value::from(*kind));
            sheet.set_value(c(&format!("C{}", i + 3)), Value::Number(*amount));
        }
        let mut calc = Calc::new();
        calc.rebuild(&wb);
        T { wb, calc }
    }
    fn set(&mut self, at: &str, formula: &str) {
        self.wb.sheet_mut(0).unwrap().cells.set(c(at), Cell::formula(Formula::new(formula)));
        self.calc.cells_changed(&mut self.wb, &[(0, c(at))]);
    }
    fn get(&self, at: &str) -> Value {
        self.wb.sheet(0).unwrap().value(c(at))
    }
}

#[test]
fn automatic_headers_are_detected_and_not_shown() {
    let mut t = T::new();
    t.set("E3", "=GROUPBY(B2:B7,C2:C7,SUM)");
    let expected = [("Design", 92000.0), ("Development", 73000.0), ("Marketing", 10000.0), ("Research", 86000.0), ("Total", 261000.0)];
    for (i, (key, sum)) in expected.iter().enumerate() {
        let row = i + 3;
        assert_eq!(t.get(&format!("E{row}")), Value::from(*key), "E{row}");
        assert_eq!(t.get(&format!("F{row}")), Value::Number(*sum), "F{row}");
    }
    assert_eq!(t.get("E8"), Value::Empty);
    assert_eq!(t.get("F8"), Value::Empty);
}

#[test]
fn explicit_mode_three_still_shows_the_headers() {
    let mut t = T::new();
    t.set("E3", "=GROUPBY(B2:B7,C2:C7,SUM,3)");
    assert_eq!(t.get("E3"), Value::from("Project type"));
    assert_eq!(t.get("F3"), Value::from("Revenue"));
    assert_eq!(t.get("E4"), Value::from("Design"));
    assert_eq!(t.get("F4"), Value::Number(92000.0));
    assert_eq!(t.get("E8"), Value::from("Total"));
    assert_eq!(t.get("F8"), Value::Number(261000.0));
}

#[test]
fn explicit_mode_one_matches_automatic() {
    let mut t = T::new();
    t.set("E3", "=GROUPBY(B2:B7,C2:C7,SUM,1)");
    t.set("H3", "=GROUPBY(B2:B7,C2:C7,SUM)");
    for row in 3..=8 {
        assert_eq!(t.get(&format!("H{row}")), t.get(&format!("E{row}")), "H{row}");
        assert_eq!(t.get(&format!("I{row}")), t.get(&format!("F{row}")), "I{row}");
    }
}
