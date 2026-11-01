//! Differential formats on tables and table columns (`headerRowDxfId`, `dataDxfId`, `totalsRowDxfId`
//! and the table's border dxfs) survive open + save. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// dxf 0: bold; 1: percent number format; 2: yellow fill; 3: thin top border.
const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><dxfs count="4"><dxf><font><b/></font></dxf><dxf><numFmt numFmtId="10" formatCode="0.00%"/></dxf><dxf><fill><patternFill patternType="solid"><bgColor rgb="FFFFFF00"/></patternFill></fill></dxf><dxf><border><top style="thin"><color rgb="FF000000"/></top></border></dxf></dxfs></styleSheet>"#;

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
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Share</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2"><v>0.25</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>b</t></is></c><c r="B3"><v>0.75</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>Total</t></is></c><c r="B4"><f>SUBTOTAL(109,[Share])</f><v>1</v></c></row></sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts></worksheet>"#
    );
    let table = format!(
        r#"<?xml version="1.0"?><table xmlns="{NS_MAIN}" id="1" name="T" displayName="T" ref="A1:B4" totalsRowCount="1"{table_attrs}><autoFilter ref="A1:B3"/><tableColumns count="2"><tableColumn id="1" name="Item" totalsRowLabel="Total"{}/><tableColumn id="2" name="Share" totalsRowFunction="sum"{}/></tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#,
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

/// What a source dxf (by index in `STYLES`) contains, in a form that survives renumbering.
#[derive(Debug, PartialEq)]
enum Look {
    Bold,
    Percent,
    YellowFill,
    TopBorder,
}

fn look(dxf: &El) -> Look {
    if dxf.child("font").and_then(|f| f.child("b")).is_some() {
        Look::Bold
    } else if dxf.child("numFmt").and_then(|n| n.attr("formatCode")) == Some("0.00%") {
        Look::Percent
    } else if dxf.child("fill").is_some() {
        Look::YellowFill
    } else if dxf.child("border").and_then(|b| b.child("top")).and_then(|t| t.attr("style")) == Some("thin") {
        Look::TopBorder
    } else {
        panic!("unexpected dxf {dxf:?}")
    }
}

fn part(z: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> El {
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    xml::parse(s.as_bytes()).unwrap()
}

/// Saved table root, saved columns, and the saved dxf an attribute on `el` points at.
fn round_trip(pkg: &[u8]) -> (El, El) {
    let out = write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/tables/table")).unwrap().to_string();
    (part(&mut z, &name), part(&mut z, "xl/styles.xml"))
}

fn dxf_of(el: &El, attr: &str, styles: &El) -> Option<Look> {
    let i: usize = el.attr(attr)?.parse().unwrap();
    let dxfs: Vec<&El> = styles.child("dxfs").expect("dxfs").kids("dxf").collect();
    Some(look(dxfs.get(i).unwrap_or_else(|| panic!("{attr}={i} out of range"))))
}

#[test]
fn table_column_dxfs_survive_a_round_trip() {
    let pkg = package("", [r#" headerRowDxfId="0""#, r#" dataDxfId="1" totalsRowDxfId="0""#]);
    let (t, styles) = round_trip(&pkg);
    let cols: Vec<&El> = t.child("tableColumns").unwrap().kids("tableColumn").collect();
    assert_eq!(dxf_of(cols[0], "headerRowDxfId", &styles), Some(Look::Bold));
    assert_eq!(dxf_of(cols[0], "dataDxfId", &styles), None);
    assert_eq!(dxf_of(cols[1], "dataDxfId", &styles), Some(Look::Percent));
    assert_eq!(dxf_of(cols[1], "totalsRowDxfId", &styles), Some(Look::Bold));
    assert_eq!(dxf_of(cols[1], "headerRowDxfId", &styles), None);
}

#[test]
fn table_level_dxfs_survive_a_round_trip() {
    let pkg = package(
        r#" headerRowDxfId="0" dataDxfId="2" totalsRowDxfId="0" headerRowBorderDxfId="3" tableBorderDxfId="3" totalsRowBorderDxfId="3""#,
        ["", ""],
    );
    let (t, styles) = round_trip(&pkg);
    assert_eq!(dxf_of(&t, "headerRowDxfId", &styles), Some(Look::Bold));
    assert_eq!(dxf_of(&t, "dataDxfId", &styles), Some(Look::YellowFill));
    assert_eq!(dxf_of(&t, "totalsRowDxfId", &styles), Some(Look::Bold));
    assert_eq!(dxf_of(&t, "headerRowBorderDxfId", &styles), Some(Look::TopBorder));
    assert_eq!(dxf_of(&t, "tableBorderDxfId", &styles), Some(Look::TopBorder));
    assert_eq!(dxf_of(&t, "totalsRowBorderDxfId", &styles), Some(Look::TopBorder));
}

#[test]
fn a_table_without_dxfs_writes_none() {
    let (t, _) = round_trip(&package("", ["", ""]));
    for a in ["headerRowDxfId", "dataDxfId", "totalsRowDxfId", "tableBorderDxfId"] {
        assert_eq!(t.attr(a), None, "{a}");
        for c in t.child("tableColumns").unwrap().kids("tableColumn") {
            assert_eq!(c.attr(a), None, "{a}");
        }
    }
}

#[test]
fn an_out_of_range_dxf_id_is_dropped_not_a_panic() {
    let (t, _) = round_trip(&package(r#" dataDxfId="99""#, [r#" dataDxfId="42""#, ""]));
    assert_eq!(t.attr("dataDxfId"), None);
}
