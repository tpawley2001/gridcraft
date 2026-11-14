//! Rows fitted to their content (on open, and after an edit) take each line of a cell's height from
//! the font's own default row height, as Excel does, not from a height scaled off Calibri 11's
//! 20 px row. Excel's default row heights per font (96 dpi pixels): Arial 8 = 15 (11.25 pt),
//! Arial 10 = 17 (12.75 pt), Arial 12 = 20 (15 pt), Times New Roman 12 = 21 (15.75 pt),
//! Calibri 11 = 20 (15 pt), Calibri 14 = 25 (18.75 pt): the table in `print::metrics`.
//! Measured: a legacy .xls whose 10,587 rows hold one line of wrapped Arial 10 text stores them
//! at 12.75 pt and Excel prints them 12.4 pt apart (52 rows a Letter page); a height scaled from
//! Calibri gives 13.64 pt (49 rows a page).
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, Style, Workbook};

/// A one-sheet workbook whose rows default to `default_px`, with `text` in A1 in `font`/`size`
/// (wrapped when `wrap`), fitted by `auto_row_height`. Returns row 1's height in pixels.
fn fitted_px(default_px: f32, font: &str, size: f32, wrap: bool, text: &str) -> f32 {
    let mut wb = Workbook::new();
    let mut st = Style::default();
    st.font.name = font.to_string();
    st.font.size = size;
    st.align.wrap = wrap;
    let id = wb.styles.intern(st);
    {
        let sh = wb.sheet_mut(0).unwrap();
        sh.default_row_height = default_px;
        sh.set_cell(CellRef::new(0, 0), Cell::styled(Value::Text(text.into()), id));
    }
    super::auto_row_height(&mut wb, 0, 0);
    wb.sheet(0).unwrap().row_height(0)
}

fn close(got: f32, want: f32) -> bool {
    (got - want).abs() < 0.01
}

#[test]
fn one_line_of_wrapped_text_takes_the_fonts_row_height() {
    // (font, size, sheet default px, expected px)
    let cases: &[(&str, f32, f32, f32)] = &[
        ("Arial", 10.0, 17.0, 17.0),
        ("Arial", 12.0, 17.0, 20.0),
        ("Times New Roman", 12.0, 17.0, 21.0),
        ("Calibri", 11.0, 20.0, 20.0),
        ("Calibri", 14.0, 20.0, 25.0),
    ];
    for &(font, size, default, want) in cases {
        let got = fitted_px(default, font, size, true, "Ab");
        assert!(close(got, want), "{font} {size} wrapped, one line, default {default}px: want {want}px, got {got}px");
    }
}

#[test]
fn a_bigger_font_without_wrap_takes_its_own_row_height() {
    let cases: &[(&str, f32, f32, f32)] = &[("Arial", 12.0, 17.0, 20.0), ("Calibri", 14.0, 20.0, 25.0), ("Times New Roman", 12.0, 17.0, 21.0)];
    for &(font, size, default, want) in cases {
        let got = fitted_px(default, font, size, false, "Ab");
        assert!(close(got, want), "{font} {size}, default {default}px: want {want}px, got {got}px");
    }
}

#[test]
fn the_sheet_default_stays_the_floor() {
    // Arial 8 (15 px) on an Arial 10 sheet (17 px): the row keeps the sheet default, as before.
    let got = fitted_px(17.0, "Arial", 8.0, true, "Ab");
    assert!(close(got, 17.0), "want 17px, got {got}px");
}

#[test]
fn a_font_outside_the_table_keeps_the_estimate() {
    // No measured row height for this font: the row keeps today's estimate,
    // max(round(size * 1.8), 20 * size / 11) = 20 * 10 / 11 px.
    let got = fitted_px(17.0, "Zzz Unlisted Sans", 10.0, true, "Ab");
    assert!(close(got, 20.0 * 10.0 / 11.0), "want {}px, got {got}px", 20.0 * 10.0 / 11.0);
}

#[test]
fn each_wrapped_line_takes_the_fonts_row_height() {
    // n lines of a listed font: n x its default row height (Arial 10: 2 x 17 px = 25.5 pt).
    let cases: &[(&str, f32, f32, f32)] = &[("Arial", 10.0, 17.0, 34.0), ("Calibri", 11.0, 20.0, 40.0)];
    for &(font, size, default, want) in cases {
        let got = fitted_px(default, font, size, true, "a\nb");
        assert!(close(got, want), "{font} {size}, two wrapped lines: want {want}px, got {got}px");
    }
}
