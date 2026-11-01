//! Printing a sheet of very long text stays fast: print layout looks at no more than the 1,024
//! characters Excel shows in a cell (Excel specifications and limits), however long the stored
//! text is. A sheet of 12,000 cells holding a 1 MB string each never finished exporting.

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use serde_json::json;
use std::time::{Duration, Instant};

/// `rows` x `cols` cells, each holding the same `len`-character string made of 32-character runs.
fn session_with_long_text(rows: u32, cols: u32, len: usize) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let text: String = (0..len).map(|i| char::from(b'a' + ((i / 32) % 26) as u8)).collect();
    let v = Value::from(text.as_str());
    let d = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut d.wb);
    let i = wb.active_sheet();
    let sh = wb.sheet_mut(i).unwrap();
    for r in 0..rows {
        for c in 0..cols {
            sh.set_value(CellRef::new(r, c), v.clone());
        }
    }
    s
}

fn print_pages(s: &mut Session) -> (u64, Duration) {
    let t = Instant::now();
    let out = s.execute("file.print", json!({})).unwrap();
    (out["pages"].as_u64().unwrap_or(0), t.elapsed())
}

// The bound is generous for a debug build on a loaded machine; the bug took minutes.
const LIMIT: Duration = Duration::from_secs(20);

#[test]
fn long_text_sheet_prints_quickly() {
    let mut s = session_with_long_text(2_000, 3, 200_000);
    let (pages, took) = print_pages(&mut s);
    assert!(pages >= 1, "no pages printed");
    assert!(took < LIMIT, "printing 6,000 long cells took {took:?}");
}

#[test]
fn wrapped_long_text_prints_quickly() {
    let mut s = session_with_long_text(200, 3, 200_000);
    s.execute("home.wrapText", json!({"range": "A1:C200"})).unwrap();
    let (pages, took) = print_pages(&mut s);
    assert!(pages >= 1, "no pages printed");
    assert!(took < LIMIT, "printing 600 wrapped long cells took {took:?}");
}
