//! Integration-style tests: round trips, hand-written fixtures, hostile input and CSV.

mod app_props_manager_tests;
mod array_export;
mod autofilter_date_tests;
mod axis_scale_roundtrip;
mod binary_custom_props_tests;
mod binary_doc_props;
mod binary_workbook_view_tests;
mod cellmetadata;
mod chart_book_names;
mod chart_bool_default;
mod chart_combo_kinds;
mod chart_combo_pie_doughnut;
mod chart_data_table;
mod chart_ref_offsets;
mod chart_sheet_print_tests;
mod chart_theme_override_tests;
mod code_name_tests;
mod controls_tests;
mod csv_tests;
mod custom_views_tests;
mod data_table_formula_tests;
mod data_table_text_results_tests;
mod dif_tests;
mod drawing_hyperlinks;
mod dxf_font_charset;
mod encrypted_tests;
mod equation_tests;
mod fixtures;
mod form_check_option_controls_tests;
mod google_sheets_dummy;
mod group_passthrough;
mod hidden_drawings;
mod ink_roundtrip;
mod kept_macro_dialog_sheets;
mod legacy_active_sheet_tests;
mod legacy_outline_settings;
mod legacy_vml_controls_tests;
mod linked_picture_tests;
mod literal_category_roundtrip;
mod locale_numfmt;
mod malformed;
mod multilevel_category_roundtrip;
mod note_box;
mod note_picture_fill_ids;
mod note_picture_fill_tests;
mod ods_floor_ceiling_mode_tests;
mod ods_no_mimetype_tests;
mod ods_repeat_edge_tests;
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
mod shape_preset_geometry;
mod shape_text_rtl;
mod shared_media;
mod sheet_custom_properties;
mod sheet_data_pin;
mod sheet_names;
mod smartart_passthrough;
mod smooth_roundtrip;
mod spreadsheetml_tests;
mod strict_passthrough;
mod sylk_tests;
mod sylk_write_tests;
mod table_dxfs;
mod table_header_space_tests;
mod table_ids;
mod theme_fmt_scheme_tests;
mod trendline_roundtrip;
mod volatile_dependencies;
mod webextension_passthrough;
mod xlm_names_tests;
mod xls_autofilter_tests;
mod xls_biff5_header_footer_tests;
mod xls_biff5_objects_tests;
mod xls_biff5_page_breaks_tests;
mod xls_biff5_style_tests;
mod xls_biff5_tests;
mod xls_cell_address_names_tests;
mod xls_cf12_tests;
mod xls_chart_pie_angle_tests;
mod xls_chart_tests;
mod xls_codepages_tests;
mod xls_connections_tests;
mod xls_custom_views_tests;
mod xls_date_filter_tests;
mod xls_edit_ranges_tests;
mod xls_group_shapes_tests;
mod xls_invalid_names_tests;
mod xls_label_tail_tests;
mod xls_macro_sheet_tests;
mod xls_picture_tests;
mod xls_pivot_tests;
mod xls_print_tests;
mod xls_protection_tests;
mod xls_selection_refu_tests;
mod xls_selection_tests;
mod xls_shape_text_tests;
mod xls_shapes_tests;
mod xls_sheet_name_tests;
mod xls_style_tests;
mod xls_table_tests;
mod xls_tests;
mod xls_textbox_tests;
mod xls_theme_tests;
mod xls_vba_tests;
mod xlsb_array_const_tests;
mod xlsb_block_warning_tests;
mod xlsb_cell_style_tests;
mod xlsb_chart_sheet_tests;
mod xlsb_comment_parts_tests;
mod xlsb_connections_tests;
mod xlsb_custom_views_tests;
mod xlsb_date_filter_tests;
mod xlsb_drawing_tests;
mod xlsb_dynamic_array_tests;
mod xlsb_edit_ranges_tests;
mod xlsb_filter_tests;
mod xlsb_layout_tests;
mod xlsb_links_tests;
mod xlsb_list_tests;
mod xlsb_macro_sheet_tests;
mod xlsb_mixed_sheets_tests;
mod xlsb_names_tests;
mod xlsb_phonetic_tail_tests;
mod xlsb_pivot_tests;
mod xlsb_print_tests;
mod xlsb_protection_tests;
mod xlsb_rule_links_tests;
mod xlsb_rules_tests;
mod xlsb_selection_tests;
mod xlsb_smartart_tests;
mod xlsb_sparkline_tests;
mod xlsb_tables_tests;
mod xlsb_tests;
mod xlsb_theme_tests;
mod xlsb_vba_tests;
mod zip_order;
mod zip_truncated_recovery_tests;

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

mod codex_day_major_font;
mod codex_day_revision_history;
mod codex_day_table_sort;
mod codex_day_web_publishing;
mod codex_day_workbook_options;
mod codex_day_worksheet_settings;
mod codex_day_xls_textboxes;
mod codex_day_xlsb_cell_parameter;
mod font_scheme_tests;
mod revision_history;
mod table_sort_tests;
mod web_publishing_tests;
mod workbook_options_controls;
mod worksheet_settings;

mod border_inside_lines;
mod codex_day_border_extras;
mod codex_day_protection;

mod workbook_protection_locks {
    use super::minimal;
    use crate::package::Package;
    use crate::xml::{El, parse};
    use crate::{read_xlsx, write_xlsx};

    fn saved(body: &str) -> El {
        let (wb, _) = read_xlsx(&minimal("<sheetData/>", &[], "", body)).unwrap();
        let bytes = write_xlsx(&wb).unwrap();
        parse(&Package::open(&bytes).unwrap().read("xl/workbook.xml").unwrap().unwrap()).unwrap()
    }

    #[test]
    fn invalid_lock_booleans_are_omitted() {
        let x = saved(r#"<workbookProtection lockWindows="maybe" lockRevision="1"/>"#);
        let p = x.child("workbookProtection").unwrap();
        assert!(p.attr("lockWindows").is_none());
        assert!(p.attr("lockStructure").is_none());
        assert_eq!(p.attr_bool("lockRevision"), Some(true));
    }

    #[test]
    fn lowercase_legacy_hex_is_written_canonical() {
        let x = saved(r#"<workbookProtection lockWindows="1" workbookPassword="cbeb" revisionsPassword="abcd"/>"#);
        let p = x.child("workbookProtection").unwrap();
        assert_eq!(p.attr("workbookPassword"), Some("CBEB"));
        assert_eq!(p.attr("revisionsPassword"), Some("ABCD"));
    }

    #[test]
    fn malformed_password_never_unprotects() {
        let (wb, _) = read_xlsx(&minimal(
            "<sheetData/>",
            &[],
            "",
            r#"<workbookProtection lockStructure="1" lockRevision="1" workbookPassword="zz" revisionsPassword="q"/>"#,
        ))
        .unwrap();
        assert!(wb.protected_structure);
        assert!(!wb.structure_password.as_ref().unwrap().verify("").unwrap_or(false));
        assert!(!wb.locks.revisions_password.as_ref().unwrap().verify("").unwrap_or(false));
        assert_eq!(wb.locks.lock_revision, Some(true));
    }

    #[test]
    fn structure_lock_is_written_once_with_window_lock() {
        let x = saved(r#"<workbookProtection lockStructure="1" lockWindows="1" workbookPassword="CBEB"/><bookViews><workbookView/></bookViews>"#);
        assert_eq!(x.kids("workbookProtection").count(), 1);
        let p = x.child("workbookProtection").unwrap();
        assert_eq!(p.attr_bool("lockStructure"), Some(true));
        assert_eq!(p.attr_bool("lockWindows"), Some(true));
        assert_eq!(p.attr("workbookPassword"), Some("CBEB"));
        let names: Vec<&str> = x.children.iter().map(|c| c.name.as_str()).collect();
        let at = |n: &str| names.iter().position(|c| *c == n).unwrap();
        assert!(at("workbookProtection") < at("bookViews"));
    }
}
mod codex_day_alignment_extras;
mod codex_day_font_effects;
mod codex_day_note_appearance;
mod codex_day_print_options;
mod codex_day_table_metadata;
mod codex_day_text_body;
