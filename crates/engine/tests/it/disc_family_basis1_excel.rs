//! DISC, INTRATE, RECEIVED, PRICEDISC and YIELDDISC across day-count bases. Expected values are Microsoft Excel's own
//! recalculated answers for the same formulas.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::Session;
use serde_json::json;

/// Excel's answer for a cell (each table uses only the kinds it needs).
#[allow(dead_code)]
enum X {
    N(f64),
    T(&'static str),
    B(bool),
    E(&'static str),
}

/// Input cells the formulas read.
const DATA: &[(&str, &str)] = &[];

/// (cell, formula, Excel's answer)
const CASES: &[(&str, &str, X)] = &[
    ("B6", r#"DISC(DATE(2022,3,1),DATE(2022,9,1),97.975,100,1)"#, X::N(0.04016983695652194)),
    ("B7", r#"INTRATE(DATE(2022,3,1),DATE(2022,9,1),1000000,1014420,1)"#, X::N(0.02860489130434783)),
    ("B8", r#"RECEIVED(DATE(2022,3,1),DATE(2022,9,1),1000000,0.0575,1)"#, X::N(1029851.5885108064)),
    ("B9", r#"PRICEDISC(DATE(2022,3,1),DATE(2022,9,1),0.0525,100,1)"#, X::N(97.35342465753425)),
    ("B10", r#"YIELDDISC(DATE(2022,3,1),DATE(2022,9,1),99.795,100,1)"#, X::N(0.004074929692826783)),
    ("B17", r#"DISC(DATE(2023,11,1),DATE(2024,4,1),97.975,100,1)"#, X::N(0.04875986842105288)),
    ("B18", r#"INTRATE(DATE(2023,11,1),DATE(2024,4,1),1000000,1014420,1)"#, X::N(0.03472184210526316)),
    ("B19", r#"RECEIVED(DATE(2023,11,1),DATE(2024,4,1),1000000,0.0575,1)"#, X::N(1024463.9758159324)),
    ("B20", r#"PRICEDISC(DATE(2023,11,1),DATE(2024,4,1),0.0525,100,1)"#, X::N(97.81967213114754)),
    ("B21", r#"YIELDDISC(DATE(2023,11,1),DATE(2024,4,1),99.795,100,1)"#, X::N(0.004946324175085199)),
    ("B28", r#"DISC(DATE(2024,1,10),DATE(2024,2,10),97.975,100,1)"#, X::N(0.23908064516129152)),
    ("B29", r#"INTRATE(DATE(2024,1,10),DATE(2024,2,10),1000000,1014420,1)"#, X::N(0.17024903225806454)),
    ("B30", r#"RECEIVED(DATE(2024,1,10),DATE(2024,2,10),1000000,0.0575,1)"#, X::N(1004894.053690446)),
    ("B31", r#"PRICEDISC(DATE(2024,1,10),DATE(2024,2,10),0.0525,100,1)"#, X::N(99.55532786885246)),
    ("B32", r#"YIELDDISC(DATE(2024,1,10),DATE(2024,2,10),99.795,100,1)"#, X::N(0.024252944342353235)),
    ("B39", r#"DISC(DATE(2024,3,1),DATE(2024,12,1),97.975,100,1)"#, X::N(0.026950909090909223)),
    ("B40", r#"INTRATE(DATE(2024,3,1),DATE(2024,12,1),1000000,1014420,1)"#, X::N(0.01919170909090909)),
    ("B41", r#"RECEIVED(DATE(2024,3,1),DATE(2024,12,1),1000000,0.0575,1)"#, X::N(1045154.3815812957)),
    ("B42", r#"PRICEDISC(DATE(2024,3,1),DATE(2024,12,1),0.0525,100,1)"#, X::N(96.05532786885246)),
    ("B43", r#"YIELDDISC(DATE(2024,3,1),DATE(2024,12,1),99.795,100,1)"#, X::N(0.0027339682713198193)),
    ("B50", r#"DISC(DATE(2023,3,1),DATE(2024,3,1),97.975,100,1)"#, X::N(0.0202500000000001)),
    ("B51", r#"INTRATE(DATE(2023,3,1),DATE(2024,3,1),1000000,1014420,1)"#, X::N(0.01442)),
    ("B52", r#"RECEIVED(DATE(2023,3,1),DATE(2024,3,1),1000000,0.0575,1)"#, X::N(1061007.9575596817)),
    ("B53", r#"PRICEDISC(DATE(2023,3,1),DATE(2024,3,1),0.0525,100,1)"#, X::N(94.75)),
    ("B54", r#"YIELDDISC(DATE(2023,3,1),DATE(2024,3,1),99.795,100,1)"#, X::N(0.0020542111328222686)),
    ("B61", r#"DISC(DATE(2023,2,28),DATE(2024,2,29),97.975,100,1)"#, X::N(0.020222336065573873)),
    ("B62", r#"INTRATE(DATE(2023,2,28),DATE(2024,2,29),1000000,1014420,1)"#, X::N(0.01440030054644809)),
    ("B63", r#"RECEIVED(DATE(2023,2,28),DATE(2024,2,29),1000000,0.0575,1)"#, X::N(1061096.5147842243)),
    ("B64", r#"PRICEDISC(DATE(2023,2,28),DATE(2024,2,29),0.0525,100,1)"#, X::N(94.74281805745554)),
    ("B65", r#"YIELDDISC(DATE(2023,2,28),DATE(2024,2,29),99.795,100,1)"#, X::N(0.0020514048334604896)),
    ("B72", r#"DISC(DATE(2024,2,29),DATE(2025,2,28),97.975,100,1)"#, X::N(0.020305479452054898)),
    ("B73", r#"INTRATE(DATE(2024,2,29),DATE(2025,2,28),1000000,1014420,1)"#, X::N(0.01445950684931507)),
    ("B74", r#"RECEIVED(DATE(2024,2,29),DATE(2025,2,28),1000000,0.0575,1)"#, X::N(1060831.1293069092)),
    ("B75", r#"PRICEDISC(DATE(2024,2,29),DATE(2025,2,28),0.0525,100,1)"#, X::N(94.76434426229508)),
    ("B76", r#"YIELDDISC(DATE(2024,2,29),DATE(2025,2,28),99.795,100,1)"#, X::N(0.0020598391085286313)),
    ("B83", r#"DISC(DATE(2023,3,1),DATE(2024,3,2),97.975,100,1)"#, X::N(0.02016723433242517)),
    ("B84", r#"INTRATE(DATE(2023,3,1),DATE(2024,3,2),1000000,1014420,1)"#, X::N(0.014361062670299728)),
    ("B85", r#"RECEIVED(DATE(2023,3,1),DATE(2024,3,2),1000000,0.0575,1)"#, X::N(1061273.6735893844)),
    ("B86", r#"PRICEDISC(DATE(2023,3,1),DATE(2024,3,2),0.0525,100,1)"#, X::N(94.72845417236663)),
    ("B87", r#"YIELDDISC(DATE(2023,3,1),DATE(2024,3,2),99.795,100,1)"#, X::N(0.0020458151745137306)),
    ("B94", r#"DISC(DATE(2021,6,1),DATE(2023,1,1),97.975,100,1)"#, X::N(0.01276554404145084)),
    ("B95", r#"INTRATE(DATE(2021,6,1),DATE(2023,1,1),1000000,1014420,1)"#, X::N(0.009090328151986182)),
    ("B96", r#"RECEIVED(DATE(2021,6,1),DATE(2023,1,1),1000000,0.0575,1)"#, X::N(1100367.0402387646)),
    ("B97", r#"PRICEDISC(DATE(2021,6,1),DATE(2023,1,1),0.0525,100,1)"#, X::N(91.67191780821918)),
    ("B98", r#"YIELDDISC(DATE(2021,6,1),DATE(2023,1,1),99.795,100,1)"#, X::N(0.0012949690215546253)),
    ("B105", r#"DISC(DATE(2023,12,31),DATE(2024,1,1),97.975,100,1)"#, X::N(7.391250000000037)),
    ("B106", r#"INTRATE(DATE(2023,12,31),DATE(2024,1,1),1000000,1014420,1)"#, X::N(5.2633)),
    ("B107", r#"RECEIVED(DATE(2023,12,31),DATE(2024,1,1),1000000,0.0575,1)"#, X::N(1000157.5590675244)),
    ("B108", r#"PRICEDISC(DATE(2023,12,31),DATE(2024,1,1),0.0525,100,1)"#, X::N(99.98561643835616)),
    ("B109", r#"YIELDDISC(DATE(2023,12,31),DATE(2024,1,1),99.795,100,1)"#, X::N(0.749787063480128)),
];

#[test]
fn disc_family_actual_actual_match_excel() {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in DATA {
        s.execute("cell.set", json!({"cell": cell, "input": input})).unwrap();
    }
    for (cell, f, _) in CASES {
        s.execute("cell.set", json!({"cell": cell, "input": format!("={f}")})).unwrap();
    }
    let sheet = s.doc().unwrap().wb.sheet(0).unwrap();
    let mut wrong = Vec::new();
    for (cell, f, want) in CASES {
        let got = sheet.value(CellRef::parse(cell).unwrap());
        let ok = match (want, &got) {
            (X::N(w), Value::Number(n)) => (n - w).abs() <= 1e-9 * w.abs().max(1.0),
            (X::T(w), Value::Text(t)) => &**t == *w,
            (X::B(w), Value::Bool(b)) => b == w,
            (X::E(w), Value::Error(e)) => e.to_string() == *w,
            _ => false,
        };
        if !ok {
            let w = match want {
                X::N(n) => n.to_string(),
                X::T(t) => format!("{t:?}"),
                X::B(b) => b.to_string(),
                X::E(e) => (*e).to_string(),
            };
            wrong.push(format!("{cell} ={f}: Excel {w}, got {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ from Excel:\n{}", wrong.len(), CASES.len(), wrong.join("\n"));
}
