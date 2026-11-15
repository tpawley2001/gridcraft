//! ODS column widths, row heights and hidden rows/columns, as Excel applies them when it opens an
//! `.ods`: lengths are converted at 96 dpi and rounded to whole pixels. Packages assembled in Rust.

use gridcraft_model::Workbook;

use super::make_zip;
use crate::read_ods;

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;
const NS: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0""#;

fn col_style(name: &str, width: &str) -> String {
    format!(
        r#"<style:style style:name="{name}" style:family="table-column"><style:table-column-properties style:column-width="{width}"/></style:style>"#
    )
}

fn row_style(name: &str, height: &str, optimal: Option<bool>) -> String {
    let opt = optimal.map(|o| format!(r#" style:use-optimal-row-height="{o}""#)).unwrap_or_default();
    format!(
        r#"<style:style style:name="{name}" style:family="table-row"><style:table-row-properties style:row-height="{height}"{opt}/></style:style>"#
    )
}

fn read(styles: &str, table: &str) -> Workbook {
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content {NS}><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:spreadsheet><table:table table:name="S">{table}</table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    let parts: Vec<(&str, &[u8])> = vec![("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_bytes())];
    read_ods(&make_zip(&parts)).unwrap().0
}

const CELL_ROW: &str = r#"<table:table-row><table:table-cell office:value-type="float" office:value="1"/></table:table-row>"#;

fn col_px(wb: &Workbook, col: u32) -> Option<f32> {
    wb.sheet(0).unwrap().cols.get(&col).and_then(|c| c.size)
}

fn row_px(wb: &Workbook, row: u32) -> Option<f32> {
    wb.sheet(0).unwrap().rows.get(&row).and_then(|r| r.size)
}

/// (ODF width, whole px Excel keeps): 2.868cm -> 108.397 px, 2.258cm -> 85.342, 69.59mm -> 263.017,
/// 0.75in -> 72, 1.2345in -> 118.512, 44.71mm -> 168.983, 20pt -> 26.667.
#[test]
fn column_widths_round_to_whole_pixels() {
    let cases =
        [("2.868cm", 108.0), ("2.258cm", 85.0), ("69.59mm", 263.0), ("0.75in", 72.0), ("1.2345in", 119.0), ("44.71mm", 169.0), ("20pt", 27.0)];
    let styles: String = cases.iter().enumerate().map(|(i, (w, _))| col_style(&format!("co{i}"), w)).collect();
    let cols: String = (0..cases.len()).map(|i| format!(r#"<table:table-column table:style-name="co{i}"/>"#)).collect();
    let wb = read(&styles, &format!("{cols}{CELL_ROW}"));
    for (i, (w, px)) in cases.iter().enumerate() {
        assert_eq!(col_px(&wb, i as u32), Some(*px), "column {i} width {w}");
    }
}

#[test]
fn repeated_columns_share_the_width_and_later_columns_follow_on() {
    let styles = format!("{}{}", col_style("co1", "2.868cm"), col_style("co2", "2.258cm"));
    let table = format!(
        r#"<table:table-column table:style-name="co1"/><table:table-column table:style-name="co2" table:number-columns-repeated="3"/><table:table-column table:style-name="co1"/>{CELL_ROW}"#
    );
    let wb = read(&styles, &table);
    assert_eq!(col_px(&wb, 0), Some(108.0));
    for c in 1..=3 {
        assert_eq!(col_px(&wb, c), Some(85.0), "repeated column {c}");
    }
    assert_eq!(col_px(&wb, 4), Some(108.0));
}

#[test]
fn columns_inside_groups_and_header_columns_count() {
    let styles = col_style("co1", "69.59mm");
    let table = format!(
        r#"<table:table-header-columns><table:table-column table:style-name="co1"/></table:table-header-columns><table:table-column-group><table:table-column table:number-columns-repeated="2"/><table:table-column table:style-name="co1"/></table:table-column-group>{CELL_ROW}"#
    );
    let wb = read(&styles, &table);
    assert_eq!(col_px(&wb, 0), Some(263.0));
    assert_eq!(col_px(&wb, 1), None);
    assert_eq!(col_px(&wb, 2), None);
    assert_eq!(col_px(&wb, 3), Some(263.0));
}

/// Row heights: 0.452cm -> 17.083 px (Excel keeps 17 px = 12.75pt), 0.498in -> 47.808 -> 48, 15pt -> 20,
/// 0.6cm -> 22.677 -> 23. Optimal rows keep the height as an automatic one; non-optimal rows are custom.
#[test]
fn row_heights_round_to_whole_pixels_and_optimal_means_not_custom() {
    let styles = format!(
        "{}{}{}{}",
        row_style("ro1", "0.452cm", Some(true)),
        row_style("ro2", "0.498in", Some(false)),
        row_style("ro3", "15pt", None),
        row_style("ro4", "0.6cm", Some(false))
    );
    let cell = r#"<table:table-cell office:value-type="float" office:value="1"/>"#;
    let table = format!(
        r#"<table:table-column/><table:table-row table:style-name="ro1">{cell}</table:table-row><table:table-row table:style-name="ro2">{cell}</table:table-row><table:table-row table:style-name="ro3">{cell}</table:table-row><table:table-row table:style-name="ro4" table:number-rows-repeated="2">{cell}</table:table-row>"#
    );
    let wb = read(&styles, &table);
    let sheet = wb.sheet(0).unwrap();
    let want = [(17.0, false), (48.0, true), (20.0, false), (23.0, true), (23.0, true)];
    for (r, (px, custom)) in want.iter().enumerate() {
        let info = sheet.rows.get(&(r as u32)).unwrap_or_else(|| panic!("row {r} has no entry"));
        assert_eq!(info.size, Some(*px), "row {r} height");
        assert_eq!(info.custom, *custom, "row {r} custom flag");
    }
}

#[test]
fn collapsed_and_filtered_rows_and_columns_are_hidden() {
    let styles = col_style("co1", "2.258cm");
    let cell = r#"<table:table-cell office:value-type="float" office:value="1"/>"#;
    let table = format!(
        r#"<table:table-column/><table:table-column table:style-name="co1" table:visibility="collapse"/><table:table-column table:visibility="filter"/><table:table-row>{cell}{cell}{cell}</table:table-row><table:table-row table:visibility="collapse">{cell}</table:table-row><table:table-row table:visibility="filter">{cell}</table:table-row><table:table-row>{cell}</table:table-row>"#
    );
    let wb = read(&styles, &table);
    let sheet = wb.sheet(0).unwrap();
    assert!(!sheet.cols.get(&0).is_some_and(|c| c.hidden), "A visible");
    let b = sheet.cols.get(&1).expect("B entry");
    assert!(b.hidden, "B collapsed");
    assert_eq!(b.size, Some(85.0), "B keeps its width");
    assert!(sheet.cols.get(&2).is_some_and(|c| c.hidden), "C filtered");
    assert!(!sheet.rows.get(&0).is_some_and(|r| r.hidden), "row 1 visible");
    assert!(sheet.rows.get(&1).is_some_and(|r| r.hidden), "row 2 collapsed");
    assert!(sheet.rows.get(&2).is_some_and(|r| r.hidden), "row 3 filtered");
    assert!(!sheet.rows.get(&3).is_some_and(|r| r.hidden), "row 4 visible");
}

/// Writers repeat an empty styled row/column run to the sheet edge; it must not become a map
/// entry per row or column.
#[test]
fn trailing_empty_runs_to_the_sheet_edge_stay_bounded() {
    let styles = format!("{}{}", col_style("co1", "2.868cm"), row_style("ro1", "0.452cm", Some(true)));
    let table = r#"<table:table-column table:style-name="co1" table:number-columns-repeated="16384"/><table:table-row table:style-name="ro1"><table:table-cell office:value-type="float" office:value="1"/></table:table-row><table:table-row table:style-name="ro1" table:number-rows-repeated="1048575"><table:table-cell table:number-columns-repeated="16384"/></table:table-row>"#;
    let wb = read(&styles, table);
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(row_px(&wb, 0), Some(17.0), "the row with content keeps its height");
    assert!(sheet.rows.len() <= 1025, "trailing empty row run must stay bounded, got {} entries", sheet.rows.len());
    assert!(sheet.cols.len() <= 1025, "trailing column run must stay bounded, got {} entries", sheet.cols.len());
    let a = col_px(&wb, 0).unwrap_or(sheet.default_col_width);
    assert_eq!(a, 108.0, "column A prints at the file's width (entry or sheet default)");
}

#[test]
fn unknown_styles_and_bad_lengths_leave_defaults() {
    let styles = format!("{}{}", col_style("bad", "wide"), col_style("zero", "0cm"));
    let table = format!(
        r#"<table:table-column table:style-name="nosuch"/><table:table-column table:style-name="bad"/><table:table-column table:style-name="zero"/>{CELL_ROW}"#
    );
    let wb = read(&styles, &table);
    for c in 0..3 {
        assert_eq!(col_px(&wb, c), None, "column {c}");
    }
}
