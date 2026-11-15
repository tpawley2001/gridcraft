//! One-cell known_y's: a known_x's with more than one row AND more than one column is #REF! in Excel
//! (measured on Excel 365: `=TREND(F2,G2:H3)` and `=TREND(F2,G2:H10)` are #REF!). A single row or a
//! single column of x keeps reading each cell as its own x variable.

use crate::util::testutil::*;
use gridcraft_core::CellError;

fn grid(rows: usize, cols: usize) -> crate::Arg {
    av(arr((0..rows).map(|r| (0..cols).map(|c| nv((r * cols + c + 1) as f64)).collect()).collect()))
}

#[test]
fn one_cell_y_with_two_dimensional_x_is_ref() {
    for f in ["TREND", "GROWTH", "LINEST", "LOGEST"] {
        for (rows, cols) in [(2, 2), (9, 2), (2, 9), (3, 3)] {
            is_err(ev(f, vec![n(12.0), grid(rows, cols)]), CellError::Ref);
        }
    }
    // With the optional arguments too.
    is_err(ev("TREND", vec![n(12.0), grid(2, 2), grid(2, 2), b(false)]), CellError::Ref);
    is_err(ev("LINEST", vec![n(12.0), grid(2, 2), b(true), b(true)]), CellError::Ref);
}

#[test]
fn one_cell_y_with_one_row_or_one_column_of_x_is_unchanged() {
    let row = || grid(1, 3);
    let column = || av(col(&[24.0, 24.0, 654.0]));
    for x in [row(), column()] {
        close(rows_of(&ev("TREND", vec![n(12.0), x]))[0][0].clone(), 12.0);
    }
    close(rows_of(&ev("GROWTH", vec![n(12.0), row()]))[0][0].clone(), 12.0);
    assert_eq!(rows_of(&ev("LINEST", vec![n(12.0), row()]))[0].len(), 4);
    assert_eq!(rows_of(&ev("LOGEST", vec![n(12.0), column()]))[0].len(), 4);
}
