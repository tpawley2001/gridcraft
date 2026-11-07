//! Solver's Population report (Evolutionary method): for every variable cell and constraint cell,
//! its best value and the mean, standard deviation, maximum and minimum over the final population,
//! as Excel's Solver writes it.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const REPORT: &str = "Population Report 1";
const HEADERS: [&str; 7] = ["Cell", "Name", "Best Value", "Mean Value", "Standard Deviation", "Maximum Value", "Minimum Value"];

/// |x − 3| + |y + 2| over A1:A2 in [−10, 10], with the constraint cell C1 = A1 + A2 <= 5.
fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in [("A1", "0"), ("A2", "0"), ("B1", "=ABS(A1-3)+ABS(A2+2)"), ("C1", "=A1+A2")] {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    s
}

fn params(method: &str) -> Json {
    json!({"objective": "B1", "goal": "min", "variables": "A1:A2",
        "constraints": ["A1:A2 >= -10", "A1:A2 <= 10", "C1 <= 5"], "method": method, "reports": ["population"]})
}

fn at(s: &Session, sheet: &str, c: CellRef) -> Value {
    let d = s.doc().unwrap();
    d.wb.sheet(d.wb.sheet_index(sheet).unwrap()).unwrap().value(c)
}

fn model(s: &Session, a1: &str) -> f64 {
    let d = s.doc().unwrap();
    let i = d.wb.sheet_index("Sheet1").unwrap();
    d.wb.sheet(i).unwrap().value(CellRef::parse(a1).unwrap()).as_f64().unwrap_or(f64::NAN)
}

/// The row holding `text` in column B (the report's Cell column).
fn row_of(s: &Session, text: &str) -> u32 {
    (0..80).find(|r| at(s, REPORT, CellRef::new(*r, 1)) == Value::text(text)).unwrap_or_else(|| panic!("no row for {text}"))
}

/// Best, mean, standard deviation, maximum and minimum (columns D..H) of a report row.
fn stats(s: &Session, row: u32) -> [f64; 5] {
    [3, 4, 5, 6, 7].map(|c| at(s, REPORT, CellRef::new(row, c)).as_f64().unwrap_or(f64::NAN))
}

fn check_row(s: &Session, cell: &str, want_best: f64) -> [f64; 5] {
    let r = row_of(s, cell);
    let [best, mean, sd, max, min] = stats(s, r);
    assert!((best - want_best).abs() < 1e-9, "{cell}: best {best}, want {want_best}");
    assert!(min <= best && best <= max, "{cell}: best {best} outside {min}..{max}");
    assert!(min <= mean + 1e-12 && mean <= max + 1e-12, "{cell}: mean {mean} outside {min}..{max}");
    assert!(sd >= 0.0 && sd.is_finite(), "{cell}: sd {sd}");
    assert!(sd <= (max - min) + 1e-12, "{cell}: sd {sd} wider than the range {min}..{max}");
    [best, mean, sd, max, min]
}

#[test]
fn evolutionary_writes_a_population_report_sheet_before_the_model() {
    let mut s = session();
    let r = s.execute("data.solver", params("evolutionary")).unwrap();
    assert_eq!(r["reports"], json!([REPORT]), "{r}");
    let d = s.doc().unwrap();
    let names: Vec<&str> = d.wb.sheets.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, [REPORT, "Sheet1"]);
    assert_eq!(d.wb.active().unwrap().name, "Sheet1", "the model's sheet stays active");
    assert_eq!(at(&s, REPORT, CellRef::new(0, 0)), Value::text(format!("{} Population Report", gridcraft_core::brand::APP_NAME)));
    assert_eq!(at(&s, REPORT, CellRef::new(1, 0)), Value::text("Worksheet: Sheet1"));
}

#[test]
fn variable_and_constraint_rows_hold_best_mean_sd_max_and_min() {
    let mut s = session();
    s.execute("data.solver", params("evolutionary")).unwrap();
    let (a1, a2, c1) = (model(&s, "A1"), model(&s, "A2"), model(&s, "C1"));
    // Variable Cells section: header row above $A$1, then $A$1 and $A$2.
    let r = row_of(&s, "$A$1");
    for (k, h) in HEADERS.iter().enumerate() {
        assert_eq!(at(&s, REPORT, CellRef::new(r - 1, 1 + k as u32)), Value::text(*h), "header {h}");
    }
    assert_eq!(at(&s, REPORT, CellRef::new(r - 2, 0)), Value::text("Variable Cells"));
    assert_eq!(row_of(&s, "$A$2"), r + 1, "variables in model order");
    check_row(&s, "$A$1", a1);
    check_row(&s, "$A$2", a2);
    // Constraints section: one row per constraint cell, $C$1 at its solved value.
    let rc = row_of(&s, "$C$1");
    assert!(rc > r + 1, "constraints come after the variables");
    assert_eq!(at(&s, REPORT, CellRef::new(rc - 2, 0)), Value::text("Constraints"));
    for (k, h) in HEADERS.iter().enumerate() {
        assert_eq!(at(&s, REPORT, CellRef::new(rc - 1, 1 + k as u32)), Value::text(*h), "header {h}");
    }
    check_row(&s, "$C$1", c1);
}

#[test]
fn the_same_seed_writes_the_same_report() {
    let mut s = session();
    s.execute("data.solver", params("evolutionary")).unwrap();
    let first = stats(&s, row_of(&s, "$A$1"));
    for (cell, input) in [("A1", "0"), ("A2", "0")] {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    s.execute("data.solver", params("evolutionary")).unwrap();
    let d = s.doc().unwrap();
    let i = d.wb.sheet_index("Population Report 2").expect("a second report gets the next number");
    let second: [f64; 5] = {
        let sh = d.wb.sheet(i).unwrap();
        let r = (0..80).find(|r| sh.value(CellRef::new(*r, 1)) == Value::text("$A$1")).unwrap();
        [3, 4, 5, 6, 7].map(|c| sh.value(CellRef::new(r, c)).as_f64().unwrap_or(f64::NAN))
    };
    assert_eq!(first, second);
}

#[test]
fn simplex_and_grg_refuse_the_population_report_and_change_nothing() {
    for method in ["simplex", "grg"] {
        let mut s = session();
        let e = s.execute("data.solver", params(method)).unwrap_err().to_string();
        assert!(e.contains("Population"), "{method}: {e}");
        let d = s.doc().unwrap();
        assert_eq!(d.wb.sheets.len(), 1, "{method}: no report sheet");
        assert_eq!(model(&s, "A1"), 0.0, "{method}: the variables are untouched");
    }
}

#[test]
fn population_is_listed_with_the_other_reports() {
    let mut s = session();
    let mut p = params("evolutionary");
    p["reports"] = json!(["answer", "population"]);
    let r = s.execute("data.solver", p).unwrap();
    assert_eq!(r["reports"], json!(["Answer Report 1", REPORT]), "{r}");
}
