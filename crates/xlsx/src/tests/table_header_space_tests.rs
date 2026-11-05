//! Table column names on save: a header cell holding only spaces is a real name. Excel keeps a
//! `" "` header (cell text and `tableColumn name`) when it saves the file; only an empty name
//! gets a generated `ColumnN`. Packages are built in code.

use std::io::Read;

use gridcraft_core::{CellRef, Value};

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// One sheet with a table on A1:B3. `first` is the text of header cell A1 and of the first
/// `tableColumn name`; B1 is "Series 1"; the body holds numbers.
fn package(first: &str) -> Vec<u8> {
    let a1 = if first.is_empty() { String::new() } else { format!(r#"<c r="A1" t="inlineStr"><is><t xml:space="preserve">{first}</t></is></c>"#) };
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1">{a1}<c r="B1" t="inlineStr"><is><t>Series 1</t></is></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>4</v></c></row></sheetData><tableParts count="1"><tablePart r:id="rIdT1"/></tableParts></worksheet>"#
    );
    let table = format!(
        r#"<?xml version="1.0"?><table xmlns="{NS_MAIN}" id="1" name="Table1" displayName="Table1" ref="A1:B3" totalsRowShown="0"><tableColumns count="2"><tableColumn id="1" name="{first}"/><tableColumn id="2" name="Series 1"/></tableColumns><tableStyleInfo showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>"#
    );
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/><Override PartName="/xl/tables/table1.xml" ContentType="{CT}.table+xml"/></Types>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", types),
        ("_rels/.rels", rels(&[("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", rels(&[("rId1", "worksheet", "worksheets/sheet1.xml")])),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/_rels/sheet1.xml.rels", rels(&[("rIdT1", "table", "../tables/table1.xml")])),
        ("xl/tables/table1.xml", table),
    ];
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (*n, d.as_bytes())).collect();
    make_zip(&refs)
}

/// Open the package, save it, and return the saved bytes.
fn save(pkg: &[u8]) -> Vec<u8> {
    write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap()
}

/// The `name` of every `tableColumn` in the saved table part.
fn saved_column_names(out: &[u8]) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out.to_vec())).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/tables/") && n.ends_with(".xml")).expect("table part").to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let el = crate::xml::parse(s.as_bytes()).unwrap();
    let cols = el.kids("tableColumns").next().expect("tableColumns");
    cols.kids("tableColumn").map(|c| c.attr("name").unwrap_or_default().to_string()).collect()
}

#[test]
fn table_header_space_is_kept_as_the_column_name() {
    assert_eq!(saved_column_names(&save(&package(" "))), vec![" ".to_string(), "Series 1".to_string()]);
}

#[test]
fn table_header_space_cell_keeps_its_text() {
    let out = save(&package(" "));
    let (wb, _) = read_xlsx(&out).unwrap();
    let s = wb.sheet(0).unwrap();
    assert_eq!(s.value(CellRef::new(0, 0)), Value::text(" "));
    assert_eq!(s.tables[0].columns[0].name, " ");
}

#[test]
fn table_header_space_guard_empty_header_still_becomes_column1() {
    assert_eq!(saved_column_names(&save(&package(""))), vec!["Column1".to_string(), "Series 1".to_string()]);
}
