//! INDIRECT reference text whose sheet name starts with a digit and is not quoted (`"1月!A:A"`).
//! Excel resolves it to that sheet: real workbooks build such texts for the multi-sheet
//! `SUMPRODUCT(SUMIF(INDIRECT(ROW(INDIRECT("1:"&N))&"月!A:A"),key,INDIRECT(...)))` pattern, with
//! one sheet per month named 1月, 2月, ... (Excel's recalculated answers for that pattern were the
//! per-sheet sums, not `#REF!`).

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
    /// Sheet1 (formulas) plus 1月 and 2月, each with keys in A1:A4 and numbers in B1:B4.
    fn new() -> T {
        let mut wb = Workbook::new();
        let data: [(&str, [&str; 4], [f64; 4]); 2] =
            [("1月", ["x", "y", "x", "z"], [1.0, 2.0, 3.0, 4.0]), ("2月", ["x", "x", "y", "y"], [10.0, 20.0, 30.0, 40.0])];
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
}

#[test]
fn unquoted_digit_sheet_name_resolves_to_the_sheet() {
    // (formula, expected): sums of B where A = "x" are 1月 1+3 = 4 and 2月 10+20 = 30.
    let cases: [(&str, f64); 6] = [
        ("=INDIRECT(\"1月!B2\")", 2.0),
        ("=INDIRECT(\"2月!B4\")", 40.0),
        ("=SUM(INDIRECT(\"1月!B1:B4\"))", 10.0),
        ("=SUMIF(INDIRECT(\"1月!A:A\"),\"x\",INDIRECT(\"1月!B:B\"))", 4.0),
        // Lower-case column letters, as real workbooks write them.
        ("=SUMIF(INDIRECT(\"2月!a:a\"),\"x\",INDIRECT(\"2月!b:b\"))", 30.0),
        ("=SUMPRODUCT(SUMIF(INDIRECT({\"1月!A:A\";\"2月!A:A\"}),\"x\",INDIRECT({\"1月!B:B\";\"2月!B:B\"})))", 34.0),
    ];
    let mut t = T::new();
    for (i, (f, want)) in cases.iter().enumerate() {
        let at = format!("D{}", i + 1);
        t.set(&at, f);
        assert_eq!(t.get(&at), Value::Number(*want), "{f}");
    }
}

#[test]
fn month_sheet_sumproduct_pattern_sums_the_first_n_sheets() {
    // N in C1; ROW(INDIRECT("1:"&N))&"月!A:A" names sheets 1月..N月.
    let f = "=SUMPRODUCT(SUMIF(INDIRECT(ROW(INDIRECT(\"1:\"&$C$1))&\"月!A:A\"),\"x\",INDIRECT(ROW(INDIRECT(\"1:\"&$C$1))&\"月!B:B\")))";
    let mut t = T::new();
    t.set("C1", "1");
    t.set("D1", f);
    assert_eq!(t.get("D1"), Value::Number(4.0));
    t.set("C1", "2");
    assert_eq!(t.get("D1"), Value::Number(34.0));
}

#[test]
fn quoted_names_and_missing_sheets_are_unchanged() {
    let mut t = T::new();
    t.set("D1", "=INDIRECT(\"'1月'!B3\")");
    assert_eq!(t.get("D1"), Value::Number(3.0));
    // No sheet named 9月: still #REF!.
    t.set("D2", "=INDIRECT(\"9月!B1\")");
    assert_eq!(t.get("D2"), Value::Error(CellError::Ref));
}
