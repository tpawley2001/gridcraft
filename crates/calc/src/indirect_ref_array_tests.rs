//! INDIRECT with an array of reference texts: the criteria functions (SUMIF, COUNTIF, AVERAGEIF,
//! SUMIFS, COUNTIFS, AVERAGEIFS) are called once per reference and return an array, as in Excel's
//! multi-sheet `SUMPRODUCT(SUMIF(INDIRECT(...)))` pattern.

use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_model::{Cell, Formula, Sheet, Workbook};

use crate::Calc;

fn c(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

struct T {
    wb: Workbook,
    calc: Calc,
}

impl T {
    /// Sheet1 plus M1, M2, M3, each with keys in A1:A4 and numbers in B1:B4.
    fn new() -> T {
        let mut wb = Workbook::new();
        let data: [(&str, [&str; 4], [f64; 4]); 3] = [
            ("M1", ["x", "y", "x", "z"], [1.0, 2.0, 3.0, 4.0]),
            ("M2", ["x", "x", "y", "y"], [10.0, 20.0, 30.0, 40.0]),
            ("M3", ["z", "z", "z", "x"], [100.0, 200.0, 300.0, 400.0]),
        ];
        for (name, keys, nums) in data {
            let mut sheet = Sheet::new(name);
            for i in 0..4 {
                sheet.set_value(c(&format!("A{}", i + 1)), Value::from(keys[i]));
                sheet.set_value(c(&format!("B{}", i + 1)), Value::Number(nums[i]));
            }
            wb.sheets.push(Arc::new(sheet));
        }
        let mut calc = Calc::new();
        calc.rebuild(&wb);
        T { wb, calc }
    }
    fn set(&mut self, at: &str, input: &str) {
        let cell = if input.starts_with('=') {
            Cell::formula(Formula::new(input))
        } else {
            Cell::value(gridcraft_core::parse::parse_input(input, self.wb.date_system).value)
        };
        self.wb.sheet_mut(0).unwrap().cells.set(c(at), cell);
        self.calc.cells_changed(&mut self.wb, &[(0, c(at))]);
    }
    fn get(&self, at: &str) -> Value {
        self.wb.sheet(0).unwrap().value(c(at))
    }
    fn num(&self, at: &str) -> f64 {
        self.get(at).as_f64().unwrap_or_else(|| panic!("{at} = {:?}", self.get(at)))
    }
}

const SUMIF_THREE: &str =
    "=SUMPRODUCT(SUMIF(INDIRECT({\"M1!A1:A4\",\"M2!A1:A4\",\"M3!A1:A4\"}),\"x\",INDIRECT({\"M1!B1:B4\",\"M2!B1:B4\",\"M3!B1:B4\"})))";

#[test]
fn sumif_over_an_array_of_references_adds_each_sheet() {
    let mut t = T::new();
    t.set("D1", SUMIF_THREE);
    // M1: 1 + 3, M2: 10 + 20, M3: 400.
    assert_eq!(t.num("D1"), 434.0);
}

#[test]
fn sheet_names_built_from_row_numbers() {
    let mut t = T::new();
    t.set("N1", "2");
    t.set("D1", "=SUMPRODUCT(SUMIF(INDIRECT(\"M\"&ROW(INDIRECT(\"1:\"&N1))&\"!A:A\"),\"x\",INDIRECT(\"M\"&ROW(INDIRECT(\"1:\"&N1))&\"!B:B\")))");
    assert_eq!(t.num("D1"), 34.0);
    t.set("N1", "3");
    assert_eq!(t.num("D1"), 434.0);
}

#[test]
fn countif_sumifs_and_averageif_lift_over_reference_arrays() {
    let mut t = T::new();
    t.set("D1", "=SUMPRODUCT(COUNTIF(INDIRECT({\"M1!A1:A4\",\"M2!A1:A4\",\"M3!A1:A4\"}),\"z\"))");
    assert_eq!(t.num("D1"), 4.0);
    // A vertical text array gives a vertical result: {2;70}.
    t.set("D2", "=INDEX(SUMIF(INDIRECT({\"M1!A1:A4\";\"M2!A1:A4\"}),\"y\",INDIRECT({\"M1!B1:B4\";\"M2!B1:B4\"})),2)");
    assert_eq!(t.num("D2"), 70.0);
    t.set("D3", "=SUMPRODUCT(SUMIFS(INDIRECT({\"M1!B1:B4\",\"M2!B1:B4\"}),INDIRECT({\"M1!A1:A4\",\"M2!A1:A4\"}),\"y\"))");
    assert_eq!(t.num("D3"), 72.0);
    t.set("D4", "=INDEX(AVERAGEIF(INDIRECT({\"M1!A1:A4\",\"M2!A1:A4\"}),\"x\",INDIRECT({\"M1!B1:B4\",\"M2!B1:B4\"})),1,2)");
    assert_eq!(t.num("D4"), 15.0);
}

#[test]
fn scalar_indirect_is_unchanged() {
    let mut t = T::new();
    t.set("D1", "=SUMIF(INDIRECT(\"M2!A1:A4\"),\"x\",INDIRECT(\"M2!B1:B4\"))");
    assert_eq!(t.num("D1"), 30.0);
}

#[test]
fn a_bad_reference_text_is_ref_error_in_its_position_only() {
    let mut t = T::new();
    t.set("D1", "=SUMPRODUCT(SUMIF(INDIRECT({\"M1!A1:A4\",\"NoSuch!A1:A4\"}),\"x\"))");
    assert_eq!(t.get("D1"), Value::Error(CellError::Ref));
    t.set("D2", "=INDEX(COUNTIF(INDIRECT({\"M1!A1:A4\",\"NoSuch!A1:A4\"}),\"x\"),1,1)");
    assert_eq!(t.num("D2"), 2.0);
}

#[test]
fn recalculates_after_a_referenced_sheet_changes() {
    let mut t = T::new();
    t.set("D1", SUMIF_THREE);
    assert_eq!(t.num("D1"), 434.0);
    t.wb.sheet_mut(2).unwrap().set_value(c("B1"), Value::Number(11.0));
    t.calc.cells_changed(&mut t.wb, &[(2, c("B1"))]);
    assert_eq!(t.num("D1"), 435.0);
}
