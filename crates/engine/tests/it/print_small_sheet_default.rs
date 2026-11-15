//! A saved default below Normal's row height survives on empty automatic rows.
//! Excel's 4afba18b7a retains a 9pt default and fits its borders on one page.
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_engine::cmd::print::{PrintOptions, layout_sheet};
use gridcraft_model::{Cell, LineInfo, Style, StyleTable, Workbook};
use std::io::{Read as _, Write as _};

fn fixture(custom: Option<bool>, default_px: f32) -> Workbook {
    let mut wb = Workbook::new();
    let mut normal = Style::default();
    normal.font.name = "Arial".into();
    normal.font.size = 10.0;
    wb.styles = StyleTable::with_default(normal);
    let sh = wb.sheet_mut(0).unwrap();
    sh.default_row_height = default_px;
    sh.default_row_custom_height = custom;
    sh.print.set_print_areas(vec![RangeRef::new(CellRef::new(0, 0), CellRef::new(149, 0))]);
    for row in [0, 149] {
        sh.set_cell(CellRef::new(row, 0), Cell { value: Value::Number(7.0), ..Cell::default() });
    }
    wb
}

fn parsed(wb: &Workbook) -> Workbook {
    let bytes = gridcraft_xlsx::write_xlsx(wb).unwrap();
    gridcraft_xlsx::read_xlsx(&bytes).unwrap().0
}

// The writer intentionally treats an unstated in-memory custom-height flag as a
// user-created custom default. Build this one input part explicitly so the test
// really reads an OMITTED source attribute, without rewriting XML by substrings.
fn parsed_omitted_flag() -> Workbook {
    let bytes = gridcraft_xlsx::write_xlsx(&fixture(Some(false), 12.0)).unwrap();
    let mut input = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetFormatPr defaultRowHeight="9"/><sheetData><row r="1"><c r="A1"><v>7</v></c></row><row r="150"><c r="A150"><v>7</v></c></row></sheetData></worksheet>"#;
    for i in 0..input.len() {
        let mut part = input.by_index(i).unwrap();
        let name = part.name().to_owned();
        let mut body = Vec::new();
        part.read_to_end(&mut body).unwrap();
        out.start_file(&name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(if name == "xl/worksheets/sheet1.xml" { sheet.as_bytes() } else { &body }).unwrap();
    }
    let bytes = out.finish().unwrap().into_inner();
    gridcraft_xlsx::read_xlsx(&bytes).unwrap().0
}

fn bands(wb: &Workbook) -> Vec<(u32, u32)> {
    layout_sheet(wb, 0, &PrintOptions::default(), None).unwrap().row_bands
}

#[test]
fn false_custom_flag_keeps_a_smaller_empty_row_baseline() {
    let automatic = parsed(&fixture(Some(false), 12.0));
    let explicit = parsed(&fixture(Some(true), 12.0));
    let sh = automatic.sheet(0).unwrap();
    assert_eq!(sh.default_row_custom_height, Some(false));
    assert_eq!(sh.default_row_height, 12.0);
    assert_eq!(bands(&automatic), bands(&explicit));
    assert_eq!(automatic.sheet(0).unwrap().default_row_custom_height, Some(false));
}

#[test]
fn omitted_custom_flag_keeps_the_smaller_default_too() {
    let automatic = parsed_omitted_flag();
    let explicit = parsed(&fixture(Some(true), 12.0));
    assert_eq!(automatic.sheet(0).unwrap().default_row_custom_height, Some(false));
    assert_eq!(bands(&automatic), bands(&explicit));
}

#[test]
fn larger_noncustom_defaults_still_normalize_to_the_normal_font() {
    let automatic = parsed(&fixture(Some(false), 40.0));
    let normal = parsed(&fixture(Some(false), 17.0));
    assert_eq!(bands(&automatic), bands(&normal));
}

#[test]
fn a_known_cell_font_can_grow_a_row_above_the_small_sheet_default() {
    let mut values = fixture(Some(false), 12.0);
    let mut large = Style::default();
    large.font.name = "Calibri".into();
    large.font.size = 20.0;
    let style = values.styles.intern(large);
    let sh = values.sheet_mut(0).unwrap();
    for row in 0..150 {
        sh.set_cell(CellRef::new(row, 0), Cell { style, value: Value::Number(7.0), ..Cell::default() });
    }
    let mut control = values.clone();
    control.sheet_mut(0).unwrap().default_row_custom_height = Some(true);
    assert_eq!(bands(&parsed(&values)), bands(&parsed(&control)));
    assert!(bands(&parsed(&values)).len() > bands(&parsed(&fixture(Some(true), 12.0))).len());
}

#[test]
fn explicit_row_heights_keep_precedence_over_the_sheet_default() {
    let mut automatic = fixture(Some(false), 12.0);
    for row in 0..150 {
        automatic.sheet_mut(0).unwrap().rows.insert(row, LineInfo { size: Some(32.0), custom: true, ..LineInfo::default() });
    }
    let mut explicit = automatic.clone();
    explicit.sheet_mut(0).unwrap().default_row_custom_height = Some(true);
    assert_eq!(bands(&parsed(&automatic)), bands(&parsed(&explicit)));
}
