//! Which non-custom saved sheet defaults (`sheetFormatPr defaultRowHeight` without `customHeight`)
//! Excel prints empty rows at. Measured on Excel's PDFs of real workbooks of these shapes: a
//! default a little or a quarter below the Normal font's row prints at Normal's row, one shorter
//! than the Normal font itself prints at the saved height.
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_engine::cmd::print::{PrintOptions, layout_sheet};
use gridcraft_model::{Cell, Style, StyleTable, Workbook};

/// A sheet in Normal `font` `size`, sheet default `default_pt` with the given custom flag, an
/// explicit print area A1:A400 with values only in its first and last rows.
fn sheet(font: &str, size: f32, default_pt: f32, custom: bool) -> Workbook {
    let mut wb = Workbook::new();
    let mut normal = Style::default();
    normal.font.name = font.into();
    normal.font.size = size;
    wb.styles = StyleTable::with_default(normal);
    let sh = wb.sheet_mut(0).unwrap();
    sh.default_row_height = default_pt * 96.0 / 72.0;
    sh.default_row_custom_height = Some(custom);
    sh.print.set_print_areas(vec![RangeRef::new(CellRef::new(0, 0), CellRef::new(399, 0))]);
    for row in [0, 399] {
        sh.set_cell(CellRef::new(row, 0), Cell { value: Value::Number(7.0), ..Cell::default() });
    }
    let bytes = gridcraft_xlsx::write_xlsx(&wb).unwrap();
    gridcraft_xlsx::read_xlsx(&bytes).unwrap().0
}

fn bands(wb: &Workbook) -> Vec<(u32, u32)> {
    layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap().row_bands
}

/// Asserts the automatic sheet prints like `normal_pt` (the Normal font's row) and not like a
/// custom `saved_pt` default.
fn prints_at_normal(font: &str, size: f32, saved_pt: f32, normal_pt: f32) {
    let automatic = sheet(font, size, saved_pt, false);
    assert_eq!(automatic.sheet(0).unwrap().default_row_custom_height, Some(false));
    assert_eq!(bands(&automatic), bands(&sheet(font, size, normal_pt, true)), "{font} {size}");
    if (saved_pt - normal_pt).abs() > 0.01 {
        assert_ne!(bands(&automatic), bands(&sheet(font, size, saved_pt, true)), "{font} {size}");
    }
}

#[test]
fn calibri_11_saved_14_5_prints_empty_rows_at_15_pt() {
    // Excel's own files: defaultRowHeight="14.5" x14ac:dyDescent="0.35" under Calibri 11.
    prints_at_normal("Calibri", 11.0, 14.5, 15.0);
}

#[test]
fn yu_gothic_10_saved_12_8_prints_empty_rows_at_16_5_pt() {
    prints_at_normal("游ゴシック", 10.0, 12.8, 16.5);
}

#[test]
fn ms_pgothic_11_saved_13_5_prints_empty_rows_at_13_5_pt() {
    prints_at_normal("ＭＳ Ｐゴシック", 11.0, 13.5, 13.5);
}

#[test]
fn arial_10_saved_9_6_prints_empty_rows_at_the_saved_height() {
    let automatic = sheet("Arial", 10.0, 9.6, false);
    assert_eq!(bands(&automatic), bands(&sheet("Arial", 10.0, 9.6, true)));
    assert_ne!(bands(&automatic), bands(&sheet("Arial", 10.0, 12.75, true)));
}
