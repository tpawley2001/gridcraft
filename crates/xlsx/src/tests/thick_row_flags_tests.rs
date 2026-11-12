//! `<row thickTop thickBot>` (ECMA-376 Part 1 §18.3.1.73), the flags Excel sets on
//! automatic-height rows with thick borders, survive open + save. Packages are built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

/// Open + save a sheet with flagged and plain rows; the saved `<row>` elements.
fn saved_rows() -> Vec<El> {
    let body = concat!(
        r#"<sheetFormatPr defaultRowHeight="12.75"/><sheetData>"#,
        r#"<row r="2" ht="13.5" thickBot="1"><c r="A2"><v>1</v></c></row>"#,
        r#"<row r="4" ht="14.25" thickTop="1" thickBot="1"/>"#,
        r#"<row r="6" thickTop="true"><c r="A6"><v>2</v></c></row>"#,
        r#"<row r="8" ht="20.25" customHeight="1" thickTop="0"><c r="A8"><v>3</v></c></row>"#,
        r#"<row r="9"><c r="A9"><v>4</v></c></row>"#,
        r#"</sheetData>"#
    );
    let out = write_xlsx(&read_xlsx(&minimal(body, &[], "", "")).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let sheet = xml::parse(s.as_bytes()).unwrap();
    sheet.child("sheetData").expect("sheetData").kids("row").cloned().collect()
}

/// (thickTop, thickBot, height in points) of the saved row numbered `r`.
fn row(rows: &[El], r: u32) -> (bool, bool, Option<f64>) {
    let row = rows.iter().find(|x| x.attr_u32("r") == Some(r)).unwrap_or_else(|| panic!("row {r} was not written"));
    (row.flag("thickTop", false), row.flag("thickBot", false), row.attr("ht").and_then(|h| h.parse().ok()))
}

#[test]
fn thick_row_flags_kept_with_the_row_height() {
    let rows = saved_rows();
    assert_eq!(row(&rows, 2), (false, true, Some(13.5)));
    assert_eq!(row(&rows, 4), (true, true, Some(14.25)), "a row without cells keeps both flags");
}

#[test]
fn thick_row_flags_kept_on_a_default_height_row() {
    let rows = saved_rows();
    assert_eq!(row(&rows, 6), (true, false, None));
}

#[test]
fn thick_row_flags_absent_stay_absent() {
    let rows = saved_rows();
    assert_eq!(row(&rows, 8), (false, false, Some(20.25)));
    assert_eq!(row(&rows, 9), (false, false, None));
    for r in &rows {
        for a in ["thickTop", "thickBot"] {
            assert_ne!(r.attr(a), Some("0"), "{a}=\"0\" is never written");
        }
    }
}
