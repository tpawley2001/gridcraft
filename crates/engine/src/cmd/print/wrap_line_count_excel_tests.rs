//! The lines wrapped cell text takes, against Microsoft 365 Excel's own line counts (measured on a
//! synthetic Calibri 11 sheet: each row one wrapped string with a stored `ht` and no
//! `customHeight`, which Excel re-fits to lines x 15 pt). Column widths are the stored
//! `<col width>` in characters; `\n` is a line feed in the cell.
//!
//! 137 of the 140 counts match. Not matched (the wrap measures unhinted Calibri advances, Excel the
//! screen-hinted ones, which this table does not have):
//! - "ABCDEFGHIJKLMNOP" at 10.4286: Excel 2 lines, ours 3 (the capitals' hinted widths are narrower).
//! - "1234567890123" at 14.7143: Excel 1 line, ours 2 (hinted Calibri 11 digits are 7 px, unhinted 7.6).
//! - "Day shown in dd/mm/yyyy style." at 29.7143: Excel 2 lines, ours 1 (the whole string measures
//!   199.2 px of the 200.75 px it may use; Excel's hinted widths make it overrun).
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, LineInfo, Style, Workbook};

const WIDTHS: [f64; 5] = [7.0, 9.5714, 10.4286, 14.7143, 29.7143];

/// (text, Excel's line count at each of [`WIDTHS`]).
const CASES: [(&str, [usize; 5]); 28] = [
    ("ABCDEFGHIJKLMNOP", [4, 3, 2, 2, 1]),
    ("abcdefghijklmnopqrstuvwxyz", [5, 4, 3, 2, 1]),
    ("internationalization", [4, 3, 2, 2, 1]),
    ("WWWWWWWW", [3, 2, 2, 2, 1]),
    ("iiiiiiiiiiiiiiiiiiii", [2, 2, 2, 1, 1]),
    ("CONTINENTAL HOLDINGS OF THE SUPPLIER", [9, 5, 5, 3, 2]),
    ("Registration number of the vendor.", [7, 5, 5, 3, 2]),
    ("Field required if the Code is 07.", [6, 5, 4, 3, 1]),
    ("dd/mm/yyyy 01/01/2011 31/07/2011", [7, 6, 6, 3, 2]),
    ("Type of the person / Type of the company for non-member.", [11, 8, 8, 5, 2]),
    ("non-individual entity", [4, 3, 3, 2, 1]),
    ("ab cd ef gh ij kl", [3, 2, 2, 1, 1]),
    ("abcde fghij", [2, 2, 2, 1, 1]),
    ("abcdef ghijkl", [2, 2, 2, 1, 1]),
    ("ab  cd   ef", [2, 1, 1, 1, 1]),
    ("word ", [1, 1, 1, 1, 1]),
    ("SHORT CODE", [2, 2, 2, 1, 1]),
    ("TRADE NAME OF THE APPLICANT FIRM", [7, 5, 5, 4, 2]),
    ("Tax copy if attached or not.", [5, 4, 3, 3, 1]),
    ("If the person is exempt.", [5, 3, 3, 2, 1]),
    ("Category under which person or firm is exempt. Required if the Exempt flag is Yes", [15, 10, 10, 7, 3]),
    ("A-B-C-D-E-F-G-H-I-J", [3, 3, 2, 2, 1]),
    ("1234567890123", [3, 2, 2, 1, 1]),
    ("x\ny\nz", [3, 3, 3, 3, 3]),
    ("Format to be used is dd/mm/yyyy", [7, 5, 4, 3, 2]),
    ("Day shown in dd/mm/yyyy style.", [7, 4, 4, 3, 2]),
    ("Say whether the code is checked or not. \nAllowed values are \"A\" / \"B\"", [14, 10, 8, 5, 3]),
    ("PRIMARY STATUS OF THE RECIPIENT", [7, 5, 4, 3, 2]),
];

/// The (text, width) pairs known not to match, listed in the module documentation.
const KNOWN_MISSES: [(&str, usize); 3] = [("ABCDEFGHIJKLMNOP", 2), ("1234567890123", 3), ("Day shown in dd/mm/yyyy style.", 4)];

/// A stored `<col width>` in characters as pixels for Calibri 11 (7 px digits; ECMA-376 Part 1, 18.3.1.13).
fn col_px(w: f64) -> f32 {
    (((256.0 * w + 18.0) / 256.0) * 7.0).trunc() as f32
}

#[test]
fn wrapped_line_counts_match_excel_for_calibri_11() {
    let mut wb = Workbook::new();
    let mut wrapped = Style::default();
    wrapped.font.name = "Calibri".into();
    wrapped.font.size = 11.0;
    wrapped.align.wrap = true;
    let style = wb.styles.intern(wrapped);
    {
        let sh = wb.sheet_mut(0).unwrap();
        for (col, w) in WIDTHS.iter().enumerate() {
            sh.cols.insert(col as u32, LineInfo { size: Some(col_px(*w)), ..Default::default() });
        }
        for (row, (text, _)) in CASES.iter().enumerate() {
            for col in 0..WIDTHS.len() {
                sh.set_cell(CellRef::new(row as u32, col as u32), Cell { style, value: Value::Text((*text).into()), ..Cell::default() });
            }
        }
    }
    let sh = wb.sheet(0).unwrap();
    let st = wb.styles.get(style);
    let mut matched = 0;
    let mut misses = Vec::new();
    for (row, (text, excel)) in CASES.iter().enumerate() {
        for (col, &want) in excel.iter().enumerate() {
            let got = super::text::folded_line_count(&wb, sh, CellRef::new(row as u32, col as u32), st);
            if got == Some(want) {
                matched += 1;
            } else {
                misses.push((*text, col, want, got));
            }
        }
    }
    assert!(matched >= 132, "{matched} of 140 match; misses {misses:?}");
    for (text, col, _, _) in &misses {
        assert!(KNOWN_MISSES.contains(&(*text, *col)), "unlisted miss {text:?} at width {}: {misses:?}", WIDTHS[*col]);
    }
}
