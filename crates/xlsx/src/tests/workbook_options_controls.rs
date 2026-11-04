//! Defaults and hostile inputs for workbookPr preservation.
use super::make_zip;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use gridcraft_core::DateSystem;
use gridcraft_model::ShowObjects;
use std::io::Read;

const BOOLS: &[&str] = &[
    "autoCompressPictures",
    "discardImageEditData",
    "showInkAnnotation",
    "hidePivotFieldList",
    "showPivotChartFilter",
    "showBorderUnselectedTables",
    "checkCompatibility",
    "dateCompatibility",
];

fn fixture(properties: &str) -> Vec<u8> {
    let workbook = format!(
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{properties}<sheets><sheet name="Data" sheetId="1" r:id="sheet"/></sheets></workbook>"#
    );
    make_zip(&[
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", br#"<Relationships><Relationship Id="sheet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#),
        ("xl/worksheets/sheet1.xml", b"<worksheet><sheetData/></worksheet>"),
    ])
}

fn saved_properties(bytes: &[u8]) -> El {
    let (wb, _) = read_xlsx(bytes).unwrap();
    let first = write_xlsx(&wb).unwrap();
    let (wb, _) = read_xlsx(&first).unwrap();
    let second = write_xlsx(&wb).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(second)).unwrap();
    let mut xml = Vec::new();
    zip.by_name("xl/workbook.xml").unwrap().read_to_end(&mut xml).unwrap();
    parse(&xml).unwrap().child("workbookPr").unwrap().clone()
}

#[test]
fn absent_and_invalid_settings_use_schema_defaults() {
    for properties in ["", "<workbookPr/>"] {
        assert!(saved_properties(&fixture(properties)).attrs.is_empty());
    }
    for invalid in ["", "garbage", "-1", "4294967296", "999999999999999999999999999999"] {
        let flags: String = BOOLS.iter().map(|name| format!(r#" {name}="invalid""#)).collect();
        let source = format!(r#"<workbookPr{flags} updateLinks="invalid" defaultImageDpi="{invalid}" defaultThemeVersion="{invalid}"/>"#);
        assert!(saved_properties(&fixture(&source)).attrs.is_empty());
    }
}

#[test]
fn explicit_boolean_values_and_unsigned_endpoints_survive_twice() {
    for (source, expected) in [("false", false), ("true", true), ("0", false), ("1", true)] {
        let flags: String = BOOLS.iter().map(|name| format!(r#" {name}="{source}""#)).collect();
        let xml = saved_properties(&fixture(&format!("<workbookPr{flags}/>")));
        for name in BOOLS {
            assert_eq!(xml.attr_bool(name), Some(expected), "{name}");
        }
    }
    for number in [0, 96, 32767, u32::MAX] {
        let xml = saved_properties(&fixture(&format!(r#"<workbookPr defaultImageDpi="{number}" defaultThemeVersion="{number}"/>"#)));
        assert_eq!(xml.attr_u32("defaultImageDpi"), Some(number));
        assert_eq!(xml.attr_u32("defaultThemeVersion"), Some(number));
    }
    for policy in ["never", "always", "userSet"] {
        let xml = saved_properties(&fixture(&format!(r#"<workbookPr updateLinks="{policy}"/>"#)));
        assert_eq!(xml.attr("updateLinks"), Some(policy));
    }
}

#[test]
fn edited_existing_properties_remain_authoritative() {
    let bytes = fixture(r#"<workbookPr date1904="1" filterPrivacy="1" codeName="Old" showObjects="none" autoCompressPictures="0"/>"#);
    let (mut wb, _) = read_xlsx(&bytes).unwrap();
    wb.date_system = DateSystem::D1900;
    wb.remove_personal_info = false;
    wb.code_name = Some("New".into());
    wb.show_objects = ShowObjects::Placeholders;
    let xml = saved_properties(&write_xlsx(&wb).unwrap());
    assert_eq!(xml.attr("date1904"), None);
    assert_eq!(xml.attr("filterPrivacy"), None);
    assert_eq!(xml.attr("codeName"), Some("New"));
    assert_eq!(xml.attr("showObjects"), Some("placeholders"));
    assert_eq!(xml.attr_bool("autoCompressPictures"), Some(false));
}
