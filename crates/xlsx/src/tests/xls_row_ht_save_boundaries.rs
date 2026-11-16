//! Save-height thresholds follow Normal, independently of the stored sheet default.

use super::xls_tests::{one_sheet_with, rec, u16s};
use crate::{read_xls, write_xlsx};
use gridcraft_model::{LineInfo, Style, StyleTable};
use std::io::Read;

#[test]
fn only_stored_heights_below_each_normal_font_floor_become_custom() {
    for (name, size, floor) in [("Arial", 8.0, 15.0), ("Arial", 10.0, 17.0), ("Yu Gothic", 11.0, 25.0), ("Unknown font", 11.0, 20.0)] {
        // Read a real XLS sheet to establish the format-specific flag, then vary Normal.
        let (mut wb, _) = read_xls(&one_sheet_with(&rec(0x0225, &u16s(&[0, 255])))).unwrap();
        let mut normal = Style::default();
        normal.font.name = name.into();
        normal.font.size = size;
        wb.styles = StyleTable::with_default(normal);
        let sheet = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        sheet.default_row_height = 40.0;
        for (row, height) in [(1, Some(floor - 1.0)), (2, Some(floor)), (3, Some(floor + 1.0)), (4, None)] {
            sheet.rows.insert(row, LineInfo { size: height, outline: 1, ..LineInfo::default() });
        }
        let bytes = write_xlsx(&wb).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = Vec::new();
        zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_end(&mut xml).unwrap();
        let xml = crate::xml::parse(&xml).unwrap();
        let rows = xml.child("sheetData").unwrap();
        let rows: Vec<_> = rows.children.iter().filter(|el| el.name == "row").collect();
        assert_eq!(rows.len(), 4);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row.flag("customHeight", false), index == 0, "{name} {size}: {row:?}");
        }
        assert!(rows[3].attr("ht").is_none());
        assert!(!wb.sheets[0].rows.get(&1).unwrap().custom, "saving must not mutate the model");
    }
}
