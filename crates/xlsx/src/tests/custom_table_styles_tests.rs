//! Custom table styles (`<tableStyles>` in `xl/styles.xml`, ECMA-376 Part 1 §18.8.40-§18.8.42) survive
//! open + save: names, the `pivot` / `table` flags, every element's type, size and differential
//! format, and the workbook's default style names. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// dxf 0: bold; 1: percent number format (not used by a table style); 2: yellow fill; 3: thin top border.
const DXFS: &str = r#"<dxfs count="4"><dxf><font><b/></font></dxf><dxf><numFmt numFmtId="10" formatCode="0.00%"/></dxf><dxf><fill><patternFill patternType="solid"><bgColor rgb="FFFFFF00"/></patternFill></fill></dxf><dxf><border><top style="thin"><color rgb="FF000000"/></top></border></dxf></dxfs>"#;

/// Two custom styles: `Ledger & Co` (tables only; whole table = top border, header = bold, 2-row
/// stripes = yellow fill, and an element with no format) and `PivotLook` (PivotTables only).
const TABLE_STYLES: &str = r#"<tableStyles count="2" defaultTableStyle="Ledger &amp; Co" defaultPivotStyle="PivotLook"><tableStyle name="Ledger &amp; Co" pivot="0" count="4"><tableStyleElement type="wholeTable" dxfId="3"/><tableStyleElement type="headerRow" dxfId="0"/><tableStyleElement type="firstRowStripe" size="2" dxfId="2"/><tableStyleElement type="lastColumn"/></tableStyle><tableStyle name="PivotLook" table="0" count="2"><tableStyleElement type="pageFieldLabels" dxfId="0"/><tableStyleElement type="firstSubtotalRow" dxfId="2"/></tableStyle></tableStyles>"#;

fn styles(dxfs: &str, table_styles: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><styleSheet xmlns="{NS_MAIN}"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>{dxfs}{table_styles}</styleSheet>"#
    )
}

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// A 2-column table A1:B3 styled `table_style`, with extra attributes on `<table>`, in a workbook
/// with the given styles part.
fn package(styles_xml: &str, table_style: &str, table_attrs: &str) -> Vec<u8> {
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="{CT}.styles+xml"/><Override PartName="/xl/tables/table1.xml" ContentType="{CT}.table+xml"/></Types>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Share</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2"><v>0.25</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>b</t></is></c><c r="B3"><v>0.75</v></c></row></sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts></worksheet>"#
    );
    let table = format!(
        r#"<?xml version="1.0"?><table xmlns="{NS_MAIN}" id="1" name="T" displayName="T" ref="A1:B3"{table_attrs}><autoFilter ref="A1:B3"/><tableColumns count="2"><tableColumn id="1" name="Item"/><tableColumn id="2" name="Share"/></tableColumns><tableStyleInfo name="{table_style}" showRowStripes="1"/></table>"#
    );
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "styles", "styles.xml")]);
    let sheet_rels = rels(&[("rId1", "table", "../tables/table1.xml")]);
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/styles.xml", styles_xml.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/tables/table1.xml", table.as_bytes()),
    ])
}

/// What a source dxf contains, in a form that survives renumbering.
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

/// Open + save once: the saved package, its table root and its styles root.
fn round_trip(pkg: &[u8]) -> (Vec<u8>, El, El) {
    let out = write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out.clone())).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/tables/table")).unwrap().to_string();
    let (t, s) = (part(&mut z, &name), part(&mut z, "xl/styles.xml"));
    (out, t, s)
}

/// The saved dxf an attribute on `el` points at.
fn dxf_of(el: &El, attr: &str, styles: &El) -> Option<Look> {
    let i: usize = el.attr(attr)?.parse().unwrap();
    let dxfs: Vec<&El> = styles.child("dxfs").expect("dxfs").kids("dxf").collect();
    Some(look(dxfs.get(i).unwrap_or_else(|| panic!("{attr}={i} out of range"))))
}

/// One saved `tableStyle` as (name, pivot, table, count, elements as (type, size, look)).
type Saved = (String, Option<String>, Option<String>, Option<String>, Vec<(String, Option<String>, Option<Look>)>);

fn saved_styles(styles: &El) -> Vec<Saved> {
    let own = |v: Option<&str>| v.map(str::to_string);
    styles
        .child("tableStyles")
        .expect("tableStyles")
        .kids("tableStyle")
        .map(|s| {
            let els = s
                .kids("tableStyleElement")
                .map(|e| (e.attr("type").unwrap_or("").to_string(), own(e.attr("size")), dxf_of(e, "dxfId", styles)))
                .collect();
            (s.attr("name").unwrap_or("").to_string(), own(s.attr("pivot")), own(s.attr("table")), own(s.attr("count")), els)
        })
        .collect()
}

fn expected() -> Vec<Saved> {
    let s = |v: &str| Some(v.to_string());
    vec![
        (
            "Ledger & Co".to_string(),
            s("0"),
            None,
            s("4"),
            vec![
                ("wholeTable".to_string(), None, Some(Look::TopBorder)),
                ("headerRow".to_string(), None, Some(Look::Bold)),
                ("firstRowStripe".to_string(), s("2"), Some(Look::YellowFill)),
                ("lastColumn".to_string(), None, None),
            ],
        ),
        (
            "PivotLook".to_string(),
            None,
            s("0"),
            s("2"),
            vec![("pageFieldLabels".to_string(), None, Some(Look::Bold)), ("firstSubtotalRow".to_string(), None, Some(Look::YellowFill))],
        ),
    ]
}

#[test]
fn custom_table_styles_survive_a_round_trip() {
    let pkg = package(&styles(DXFS, TABLE_STYLES), "Ledger &amp; Co", "");
    let (_, table, st) = round_trip(&pkg);
    let ts = st.child("tableStyles").expect("tableStyles");
    assert_eq!(ts.attr("count"), Some("2"));
    assert_eq!(ts.attr("defaultTableStyle"), Some("Ledger & Co"));
    assert_eq!(ts.attr("defaultPivotStyle"), Some("PivotLook"));
    assert_eq!(saved_styles(&st), expected());
    // The table still names its custom style.
    assert_eq!(table.child("tableStyleInfo").and_then(|i| i.attr("name")), Some("Ledger & Co"));
}

#[test]
fn custom_table_styles_are_stable_over_a_second_save() {
    let pkg = package(&styles(DXFS, TABLE_STYLES), "Ledger &amp; Co", "");
    let (once, _, _) = round_trip(&pkg);
    let (_, _, st) = round_trip(&once);
    assert_eq!(st.child("tableStyles").and_then(|t| t.attr("count")), Some("2"));
    assert_eq!(saved_styles(&st), expected());
}

#[test]
fn table_style_dxfs_and_table_dxfs_are_renumbered_together() {
    // The table also uses dxf 1 (percent) for its data and dxf 0 (bold) for its header row.
    let pkg = package(&styles(DXFS, TABLE_STYLES), "Ledger &amp; Co", r#" headerRowDxfId="0" dataDxfId="1""#);
    let (_, table, st) = round_trip(&pkg);
    assert_eq!(dxf_of(&table, "headerRowDxfId", &st), Some(Look::Bold));
    assert_eq!(dxf_of(&table, "dataDxfId", &st), Some(Look::Percent));
    assert_eq!(saved_styles(&st), expected());
    let n = st.child("dxfs").expect("dxfs").kids("dxf").count();
    assert_eq!(st.child("dxfs").and_then(|d| d.attr("count")), Some(n.to_string().as_str()));
}

#[test]
fn a_dxf_id_past_the_end_keeps_the_element_without_a_format() {
    let ts = r#"<tableStyles count="1"><tableStyle name="Odd" count="2"><tableStyleElement type="headerRow" dxfId="99"/><tableStyleElement type="totalRow" dxfId="2"/></tableStyle></tableStyles>"#;
    let (_, _, st) = round_trip(&package(&styles(DXFS, ts), "Odd", ""));
    let saved = saved_styles(&st);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, "Odd");
    assert_eq!((saved[0].1.as_deref(), saved[0].2.as_deref()), (None, None), "pivot and table default to true and are not written");
    assert_eq!(saved[0].4, vec![("headerRow".to_string(), None, None), ("totalRow".to_string(), None, Some(Look::YellowFill))]);
}

#[test]
fn a_workbook_without_custom_table_styles_writes_the_empty_default() {
    let (_, _, st) = round_trip(&package(&styles(DXFS, ""), "TableStyleMedium2", ""));
    let ts = st.child("tableStyles").expect("tableStyles");
    assert_eq!(ts.attr("count"), Some("0"));
    assert_eq!(ts.attr("defaultTableStyle"), Some("TableStyleMedium2"));
    assert_eq!(ts.attr("defaultPivotStyle"), Some("PivotStyleLight16"));
    assert_eq!(ts.kids("tableStyle").count(), 0);
}

#[test]
fn stated_default_style_names_are_kept_without_custom_styles() {
    let ts = r#"<tableStyles count="0" defaultTableStyle="TableStyleLight9" defaultPivotStyle="PivotStyleMedium4"/>"#;
    let (_, _, st) = round_trip(&package(&styles(DXFS, ts), "TableStyleLight9", ""));
    let ts = st.child("tableStyles").expect("tableStyles");
    assert_eq!(ts.attr("count"), Some("0"));
    assert_eq!(ts.attr("defaultTableStyle"), Some("TableStyleLight9"));
    assert_eq!(ts.attr("defaultPivotStyle"), Some("PivotStyleMedium4"));
}
