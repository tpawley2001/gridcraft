//! The named cell styles of tables and table columns (`headerRowCellStyle`, `dataCellStyle`,
//! `totalsRowCellStyle`; ECMA-376 Part 1 §18.5.1.2 and §18.5.1.3) survive open + save. Packages
//! are built in code.

use std::io::Read;

use super::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// Cell styles: Normal, the built-in Comma and a custom "Grades & Marks". dxf 0: bold.
const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><i/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="43" fontId="0" fillId="0" borderId="0" applyFont="0" applyFill="0" applyBorder="0" applyAlignment="0" applyProtection="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="3"><cellStyle name="Comma" xfId="1" builtinId="3"/><cellStyle name="Grades &amp; Marks" xfId="2"/><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="1"><dxf><font><b/></font></dxf></dxfs></styleSheet>"#;

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// A 2-column table A1:B4 (header, 2 data rows, totals row) with the given extra attributes on
/// `<table>` and on each `<tableColumn>`.
fn package(table_attrs: &str, col_attrs: [&str; 2]) -> Vec<u8> {
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="{CT}.styles+xml"/><Override PartName="/xl/tables/table1.xml" ContentType="{CT}.table+xml"/></Types>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>City</t></is></c><c r="B1" t="inlineStr"><is><t>Population</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2"><v>1200</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>b</t></is></c><c r="B3"><v>3400</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>Total</t></is></c><c r="B4"><f>SUBTOTAL(109,[Population])</f><v>4600</v></c></row></sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts></worksheet>"#
    );
    let table = format!(
        r#"<?xml version="1.0"?><table xmlns="{NS_MAIN}" id="1" name="T" displayName="T" ref="A1:B4" totalsRowCount="1"{table_attrs}><autoFilter ref="A1:B3"/><tableColumns count="2"><tableColumn id="1" name="City" totalsRowLabel="Total"{}/><tableColumn id="2" name="Population" totalsRowFunction="sum"{}/></tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#,
        col_attrs[0], col_attrs[1]
    );
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "styles", "styles.xml")]);
    let sheet_rels = rels(&[("rId1", "table", "../tables/table1.xml")]);
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/styles.xml", STYLES.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/tables/table1.xml", table.as_bytes()),
    ])
}

fn part(z: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> El {
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    xml::parse(s.as_bytes()).unwrap()
}

/// The saved table root and the cell style names of the saved styles part.
fn saved(out: Vec<u8>) -> (El, Vec<String>) {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/tables/table")).unwrap().to_string();
    let table = part(&mut z, &name);
    let styles = part(&mut z, "xl/styles.xml");
    let names = styles.child("cellStyles").map(|c| c.kids("cellStyle").filter_map(|s| s.attr("name")).map(str::to_string).collect());
    (table, names.unwrap_or_default())
}

fn round_trip(pkg: &[u8]) -> (El, Vec<String>) {
    saved(write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap())
}

/// (headerRowCellStyle, dataCellStyle, totalsRowCellStyle) of a `<table>` or `<tableColumn>`.
fn names(el: &El) -> (Option<&str>, Option<&str>, Option<&str>) {
    (el.attr("headerRowCellStyle"), el.attr("dataCellStyle"), el.attr("totalsRowCellStyle"))
}

fn columns(table: &El) -> Vec<&El> {
    table.child("tableColumns").expect("tableColumns").kids("tableColumn").collect()
}

#[test]
fn table_cell_style_names_on_a_column_are_kept() {
    let pkg = package("", ["", r#" dataDxfId="0" dataCellStyle="Comma""#]);
    let (t, styles) = round_trip(&pkg);
    assert!(styles.iter().any(|n| n == "Comma"), "the Comma style is still in the workbook: {styles:?}");
    let cols = columns(&t);
    assert_eq!(names(cols[1]), (None, Some("Comma"), None));
    assert!(cols[1].attr("dataDxfId").is_some(), "the column's dxf is kept next to the style name");
    assert_eq!(names(cols[0]), (None, None, None));
    assert_eq!(names(&t), (None, None, None));
}

#[test]
fn table_cell_style_names_all_three_on_a_column_are_kept() {
    let col = r#" headerRowCellStyle="Grades &amp; Marks" dataCellStyle="Grades &amp; Marks" totalsRowCellStyle="Comma""#;
    let (t, styles) = round_trip(&package("", [col, ""]));
    assert!(styles.iter().any(|n| n == "Grades & Marks"), "the custom style is still in the workbook: {styles:?}");
    let cols = columns(&t);
    assert_eq!(names(cols[0]), (Some("Grades & Marks"), Some("Grades & Marks"), Some("Comma")));
    assert_eq!(names(cols[1]), (None, None, None));
}

#[test]
fn table_cell_style_names_on_the_table_are_kept() {
    let table = r#" headerRowCellStyle="Normal" dataCellStyle="Comma" totalsRowCellStyle="Grades &amp; Marks""#;
    let (t, _) = round_trip(&package(table, ["", ""]));
    assert_eq!(names(&t), (Some("Normal"), Some("Comma"), Some("Grades & Marks")));
    for c in columns(&t) {
        assert_eq!(names(c), (None, None, None));
    }
}

#[test]
fn table_cell_style_names_absent_or_empty_stay_absent() {
    let (t, _) = round_trip(&package(r#" dataCellStyle="""#, ["", r#" totalsRowCellStyle="""#]));
    assert_eq!(names(&t), (None, None, None));
    for c in columns(&t) {
        assert_eq!(names(c), (None, None, None));
    }
}

/// A name whose cell style is no longer in the workbook is not written.
#[test]
fn table_cell_style_names_of_a_removed_style_are_left_out() {
    let table = r#" dataCellStyle="Grades &amp; Marks""#;
    let col = r#" dataCellStyle="Grades &amp; Marks" totalsRowCellStyle="Comma""#;
    let mut wb = read_xlsx(&package(table, ["", col])).unwrap().0;
    wb.cell_styles.retain(|(name, _)| name != "Grades & Marks");
    let (t, styles) = saved(write_xlsx(&wb).unwrap());
    assert!(!styles.iter().any(|n| n == "Grades & Marks"), "{styles:?}");
    assert_eq!(names(&t), (None, None, None));
    assert_eq!(names(columns(&t)[1]), (None, None, Some("Comma")));
}
