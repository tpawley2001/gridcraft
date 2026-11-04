//! VLOOKUP's col_index_num errors, DGET with error-valued headers and D-function `~~` criteria,
//! with the results Excel 365 gives for the same sheets.

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

/// Error values and texts made by formulas, so no input parsing is involved.
const VALUE: &str = "=\"a\"+1";
const DIV0: &str = "=1/0";
const NA: &str = "=NA()";
const NUM: &str = "=SQRT(-1)";
const REF: &str = "=INDEX(J1:J3,9)";

#[test]
fn vlookup_col_index_errors_and_text_give_ref() {
    let mut t = T::new();
    for (at, v) in [("J1", "0"), ("K1", "rZero"), ("J2", "5"), ("K2", "r0"), ("J3", "45"), ("K3", "r3")] {
        t.set(at, v);
    }
    // (lookup, col_index, range_lookup, Excel); "" = empty cell.
    let rows: [(&str, &str, &str, Value); 10] = [
        ("0", VALUE, "TRUE", Value::Error(CellError::Ref)),
        ("0", DIV0, "TRUE", Value::Error(CellError::Ref)),
        ("0", NA, "TRUE", Value::Error(CellError::Ref)),
        ("0", REF, "TRUE", Value::Error(CellError::Ref)),
        ("0", NUM, "TRUE", Value::Error(CellError::Ref)),
        ("0", "=\"abc\"", "TRUE", Value::Error(CellError::Ref)),
        (NA, "=\"#DIV/0\"", "TRUE", Value::Error(CellError::NA)),
        ("0", "=\"#DIV/0\"", NA, Value::Error(CellError::NA)),
        ("0", "", "TRUE", Value::Error(CellError::Value)),
        ("45", "2", "FALSE", Value::from("r3")),
    ];
    for (i, (f, g, h, _)) in rows.iter().enumerate() {
        let r = i + 1;
        t.set(&format!("F{r}"), f);
        if !g.is_empty() {
            t.set(&format!("G{r}"), g);
        }
        t.set(&format!("H{r}"), h);
        t.set(&format!("B{r}"), &format!("=VLOOKUP(F{r},$J$1:$K$3,G{r},H{r})"));
    }
    for (i, (f, g, h, want)) in rows.iter().enumerate() {
        let r = i + 1;
        assert_eq!(t.get(&format!("B{r}")), *want, "VLOOKUP({f}, J1:K3, {g:?}, {h})");
    }
}

#[test]
fn dget_matches_error_valued_criteria_headers() {
    let mut t = T::new();
    // Database G1:N2: headers One, 2, "TRUE" (text), #VALUE!, #N/A, #NUM!, x, Three; one record a..h.
    for (at, v) in [
        ("G1", "One"),
        ("H1", "2"),
        // The text TRUE, as in the measured workbook (a boolean header is not measured).
        ("I1", "=\"TRUE\""),
        ("J1", VALUE),
        ("K1", NA),
        ("L1", NUM),
        ("M1", "x"),
        ("N1", "Three"),
        ("G2", "a"),
        ("H2", "b"),
        ("I2", "c"),
        ("J2", "d"),
        ("K2", "e"),
        ("L2", "f"),
        ("M2", "g"),
        ("N2", "h"),
    ] {
        t.set(at, v);
    }
    // Criteria blocks of two columns: header One + the second header, values a + the record's.
    let blocks = [("P", "Q", "2", "b"), ("S", "T", "=\"TRUE\"", "c"), ("V", "W", VALUE, "d"), ("Y", "Z", NA, "e"), ("AB", "AC", NUM, "f")];
    for (i, (c1, c2, header, value)) in blocks.iter().enumerate() {
        t.set(&format!("{c1}1"), "One");
        t.set(&format!("{c2}1"), header);
        t.set(&format!("{c1}2"), "a");
        t.set(&format!("{c2}2"), value);
        t.set(&format!("A{}", i + 1), &format!("=DGET($G$1:$N$2,\"One\",{c1}1:{c2}2)"));
    }
    for (i, (_, _, header, _)) in blocks.iter().enumerate() {
        assert_eq!(t.get(&format!("A{}", i + 1)), Value::from("a"), "criteria header {header}");
    }
}

#[test]
fn database_criteria_double_tilde_is_not_an_escaped_tilde() {
    let mut t = T::new();
    for (at, v) in [("A1", "name"), ("B1", "headgear"), ("A2", "Huey"), ("B2", "cap~"), ("A3", "Dewey"), ("B3", "cap?")] {
        t.set(at, v);
    }
    t.set("D1", "headgear");
    t.set("D2", "=\"=cap~~\"");
    t.set("E1", "headgear");
    t.set("E2", "=\"=cap~?\"");
    t.set("G1", "=DGET(A1:B3,\"name\",D1:D2)");
    t.set("G2", "=DGET(A1:B3,\"name\",E1:E2)");
    assert_eq!(t.get("G1"), Value::Error(CellError::Value), "Excel: =cap~~ matches no record");
    assert_eq!(t.get("G2"), Value::from("Dewey"));
}
