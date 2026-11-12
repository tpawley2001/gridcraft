//! Integration-style tests: round trips, hand-written fixtures, hostile input and CSV.

mod app_props_manager_tests;
mod array_export;
mod autofilter_date_tests;
mod axis_scale_roundtrip;
mod binary_cell_style_identity_tests;
mod binary_custom_props_tests;
mod binary_doc_props;
mod binary_workbook_view_tests;
mod camera_picture_vml_tests;
mod cellmetadata;
mod chart_3d;
mod chart_3d_series_shape;
mod chart_axis_options;
mod chart_book_names;
mod chart_bool_default;
mod chart_combo_kinds;
mod chart_combo_pie_doughnut;
mod chart_data_table;
mod chart_date_axis;
mod chart_of_pie;
mod chart_pie_explosion;
mod chart_print_settings;
mod chart_ref_offsets;
mod chart_sheet_print_tests;
mod chart_sheet_style_parts_tests;
mod chart_space_settings;
mod chart_surface;
mod chart_theme_override_tests;
mod chart_tick_skip;
mod code_name_tests;
mod compat_sp_anchor_tests;
mod control_pr_kept_tests;
mod controls_tests;
mod csv_tests;
mod custom_table_styles_tests;
mod custom_views_tests;
mod data_table_formula_tests;
mod data_table_text_results_tests;
mod dif_tests;
mod drawing_alt_text;
mod drawing_hyperlinks;
mod drawing_locked_title_kept_tests;
mod dxf_font_charset;
mod encrypted_tests;
mod equation_tests;
mod fit_to_stored_tests;
mod fixtures;
mod form_check_option_controls_tests;
mod form_list_controls_tests;
mod google_sheets_dummy;
mod group_passthrough;
mod hidden_drawings;
mod hyperlink_display_kept_tests;
mod indexed_colors_roundtrip;
mod ink_roundtrip;
mod kept_macro_dialog_sheets;
mod legacy_active_sheet_tests;
mod legacy_outline_apply_styles;
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
mod note_run_formatting_tests;
mod ods_floor_ceiling_mode_tests;
mod ods_no_mimetype_tests;
mod ods_page_setup_tests;
mod ods_repeat_edge_tests;
mod ods_tests;
mod page_order;
mod page_setup_scale_zero;
mod pane_frozen_split_kept_tests;
mod paper_size_ids;
mod passthrough_tests;
mod per_view_zoom_tests;
mod phonetic_tests;
mod picture_border_kept_tests;
mod picture_effects;
mod picture_locks_tests;
mod pivot_tests;
mod quote_prefix_tests;
mod rich_error_values_tests;
mod richdata_excel_package_tests;
mod richdata_tests;
mod richdata_types_part_tests;
mod roundtrip;
mod sensitivity_label;
mod shape_block_arrow_tests;
mod shape_body_props_kept_tests;
mod shape_line_dash_effects_tests;
mod shape_outline_width_tests;
mod shape_preset_geometry;
mod shape_style_refs_kept_tests;
mod shape_text_rtl;
mod shape_theme_color_kept_tests;
mod shared_media;
mod sheet_custom_properties;
mod sheet_data_pin;
mod sheet_names;
mod sheet_phonetic_pr_tests;
mod sheet_pr_flags_tests;
mod sheet_sort_state;
mod sheet_view_flags_tests;
mod signature_line_vml_tests;
mod smartart_passthrough;
mod smooth_roundtrip;
mod sparkline_group_tests;
mod spreadsheetml_tests;
mod strict_passthrough;
mod sylk_tests;
mod sylk_write_tests;
mod table_cell_style_names_tests;
mod table_dxfs;
mod table_header_space_tests;
mod table_hidden_totals_tests;
mod table_ids;
mod theme_fmt_scheme_tests;
mod thick_row_flags_tests;
mod third_party_workbook_parts;
mod trendline_roundtrip;
mod vml_html_br_tests;
mod volatile_dependencies;
mod webextension_passthrough;
mod workbook_window_rect_tests;
mod xlm_names_tests;
mod xls_autofilter_tests;
mod xls_biff5_chart_extern_tests;
mod xls_biff5_chart_tests;
mod xls_biff5_header_footer_tests;
mod xls_biff5_line_tests;
mod xls_biff5_objects_tests;
mod xls_biff5_page_breaks_tests;
mod xls_biff5_picture_tests;
mod xls_biff5_polygon_tests;
mod xls_biff5_style_tests;
mod xls_biff5_tests;
mod xls_cell_address_names_tests;
mod xls_cf12_tests;
mod xls_chart_pie_angle_tests;
mod xls_chart_tests;
mod xls_codepages_tests;
mod xls_connections_tests;
mod xls_custom_views_tests;
mod xls_data_table_tests;
mod xls_date_filter_tests;
mod xls_default_password_tests;
mod xls_doc_props_codepage_tests;
mod xls_dv_tests;
mod xls_edit_ranges_tests;
mod xls_extern_name_tests;
mod xls_external_link_tests;
mod xls_external_name_ref_tests;
mod xls_form_control_tests;
mod xls_group_shapes_tests;
mod xls_invalid_names_tests;
mod xls_label_tail_tests;
mod xls_list_control_tests;
mod xls_long_text_tests;
mod xls_macro_sheet_tests;
mod xls_office_art_presets_tests;
mod xls_picture_tests;
mod xls_pivot_tests;
mod xls_print_object_tests;
mod xls_print_tests;
mod xls_protection_tests;
mod xls_radar_area_tests;
mod xls_relative_name_tests;
mod xls_row_height_oor;
mod xls_selection_refu_tests;
mod xls_selection_tests;
mod xls_shape_text_tests;
mod xls_shapes_continue_tests;
mod xls_shapes_tests;
mod xls_sheet_name_tests;
mod xls_sheet_zoom_view_tests;
mod xls_style_tests;
mod xls_table_tests;
mod xls_tests;
mod xls_textbox_format_tests;
mod xls_textbox_tests;
mod xls_theme_tests;
mod xls_user_password_tests;
mod xls_user_xor_password_tests;
mod xls_vba_tests;
mod xls_worksheet_command_tests;
mod xls_xor_password_tests;
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
mod xlsb_external_link_cache_tests;
mod xlsb_external_link_nonfinite_tests;
mod xlsb_external_link_tests;
mod xlsb_filter_tests;
mod xlsb_gradient_fill_tests;
mod xlsb_layout_tests;
mod xlsb_links_tests;
mod xlsb_list_tests;
mod xlsb_macro_sheet_tests;
mod xlsb_missing_sheet_part_tests;
mod xlsb_mixed_sheets_tests;
mod xlsb_names_tests;
mod xlsb_phonetic_tail_tests;
mod xlsb_pivot_tests;
mod xlsb_print_tests;
mod xlsb_printer_settings_tests;
mod xlsb_protection_tests;
mod xlsb_rule_links_tests;
mod xlsb_rules_tests;
mod xlsb_selection_tests;
mod xlsb_sheet_view_mode_tests;
mod xlsb_smartart_tests;
mod xlsb_sparkline_tests;
mod xlsb_tables_tests;
mod xlsb_tests;
mod xlsb_theme_tests;
mod xlsb_vba_tests;
mod zip_bad_checksum_tests;
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
mod cell_style_identity_tests;
mod codex_day_border_extras;
mod codex_day_protection;
mod outline_apply_styles_tests;

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
mod split_panes_kept_tests;
