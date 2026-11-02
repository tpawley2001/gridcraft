//! Integration-style tests: round trips, hand-written fixtures, hostile input and CSV.

mod array_export;
mod axis_scale_roundtrip;
mod binary_doc_props;
mod binary_workbook_view_tests;
mod cellmetadata;
mod chart_book_names;
mod chart_bool_default;
mod chart_combo_kinds;
mod chart_data_table;
mod chart_ref_offsets;
mod csv_tests;
mod drawing_hyperlinks;
mod dxf_font_charset;
mod encrypted_tests;
mod fixtures;
mod google_sheets_dummy;
mod group_passthrough;
mod hidden_drawings;
mod ink_roundtrip;
mod kept_macro_dialog_sheets;
mod legacy_active_sheet_tests;
mod legacy_outline_settings;
mod literal_category_roundtrip;
mod locale_numfmt;
mod malformed;
mod multilevel_category_roundtrip;
mod note_box;
mod ods_tests;
mod page_order;
mod page_setup_scale_zero;
mod passthrough_tests;
mod phonetic_tests;
mod picture_effects;
mod pivot_tests;
mod richdata_tests;
mod roundtrip;
mod sensitivity_label;
mod shape_text_rtl;
mod shared_media;
mod sheet_custom_properties;
mod sheet_data_pin;
mod sheet_names;
mod smartart_passthrough;
mod smooth_roundtrip;
mod strict_passthrough;
mod table_dxfs;
mod table_ids;
mod trendline_roundtrip;
mod volatile_dependencies;
mod webextension_passthrough;
mod xls_autofilter_tests;
mod xls_biff5_tests;
mod xls_cf12_tests;
mod xls_chart_tests;
mod xls_pivot_tests;
mod xls_print_tests;
mod xls_protection_tests;
mod xls_style_tests;
mod xls_table_tests;
mod xls_tests;
mod xlsb_array_const_tests;
mod xlsb_cell_style_tests;
mod xlsb_drawing_tests;
mod xlsb_layout_tests;
mod xlsb_links_tests;
mod xlsb_list_tests;
mod xlsb_names_tests;
mod xlsb_pivot_tests;
mod xlsb_print_tests;
mod xlsb_protection_tests;
mod xlsb_rule_links_tests;
mod xlsb_rules_tests;
mod xlsb_sparkline_tests;
mod xlsb_tables_tests;
mod xlsb_tests;
mod xlsb_theme_tests;
mod xlsb_vba_tests;
mod zip_order;

use std::io::Write;

/// Builds a zip package from (name, content) pairs.
pub fn make_zip(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in parts {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

pub const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>"#;
pub const ROOT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

/// A minimal one-sheet package with the given sheet XML body (inside `<worksheet>`), plus
/// optional extra parts and workbook relationships.
pub fn minimal(sheet_body: &str, extra_parts: &[(&str, &str)], wb_extra_rels: &str, wb_extra: &str) -> Vec<u8> {
    let wb = format!(
        r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{wb_extra}<sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>{wb_extra_rels}</Relationships>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{sheet_body}</worksheet>"#
    );
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ];
    for (n, c) in extra_parts {
        parts.push((n, c.as_bytes()));
    }
    make_zip(&parts)
}
