//! Table ids on save: kept from the file and unique against the ids of single mapped XML cells
//! (`singleXmlCell`), which share the table id space. A collision makes Excel repair the file.
//! Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

fn rels(items: &[(String, &str, String)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// One sheet per entry; each sheet holds tables (`Some(id)` writes `id="…"`, `None` leaves it out)
/// stacked down column A, and optionally a single mapped cell with the given `singleXmlCell` id.
fn package(sheets: &[(&[Option<u32>], Option<u32>)]) -> Vec<u8> {
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.sheet.main+xml"/>"#
    );
    let mut wb_sheets = String::new();
    let mut wb_rels = Vec::new();
    let mut tn = 0;
    for (si, (tables, single)) in sheets.iter().enumerate() {
        let n = si + 1;
        wb_sheets.push_str(&format!(r#"<sheet name="S{n}" sheetId="{n}" r:id="rId{n}"/>"#));
        wb_rels.push((format!("rId{n}"), "worksheet", format!("worksheets/sheet{n}.xml")));
        types.push_str(&format!(r#"<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="{CT}.worksheet+xml"/>"#));
        let mut rows = String::new();
        let mut table_parts = String::new();
        let mut sheet_rels = Vec::new();
        for (k, id) in tables.iter().enumerate() {
            tn += 1;
            let r = 1 + 3 * k;
            rows.push_str(&format!(
                r#"<row r="{r}"><c r="A{r}" t="inlineStr"><is><t>H</t></is></c></row><row r="{}"><c r="A{}"><v>{k}</v></c></row>"#,
                r + 1,
                r + 1
            ));
            let rid = format!("rIdT{tn}");
            table_parts.push_str(&format!(r#"<tablePart r:id="{rid}"/>"#));
            sheet_rels.push((rid, "table", format!("../tables/table{tn}.xml")));
            types.push_str(&format!(r#"<Override PartName="/xl/tables/table{tn}.xml" ContentType="{CT}.table+xml"/>"#));
            let id_attr = id.map(|i| format!(r#" id="{i}""#)).unwrap_or_default();
            let xml = format!(
                r#"<?xml version="1.0"?><table xmlns="{NS_MAIN}"{id_attr} name="T{tn}" displayName="T{tn}" ref="A{r}:A{}" totalsRowShown="0"><autoFilter ref="A{r}:A{}"/><tableColumns count="1"><tableColumn id="1" name="H"/></tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#,
                r + 1,
                r + 1
            );
            parts.push((format!("xl/tables/table{tn}.xml"), xml.into_bytes()));
        }
        if let Some(id) = single {
            sheet_rels.push(("rIdS".into(), "tableSingleCells", format!("../tables/tableSingleCells{n}.xml")));
            types.push_str(&format!(r#"<Override PartName="/xl/tables/tableSingleCells{n}.xml" ContentType="{CT}.tableSingleCells+xml"/>"#));
            parts.push((format!("xl/tables/tableSingleCells{n}.xml"), single_cells(*id).into_bytes()));
            rows.push_str(r#"<row r="20"><c r="C20"><v>27</v></c></row>"#);
        }
        let tp = if tables.is_empty() { String::new() } else { format!(r#"<tableParts count="{}">{table_parts}</tableParts>"#, tables.len()) };
        let sheet = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData>{rows}</sheetData>{tp}</worksheet>"#);
        parts.push((format!("xl/worksheets/sheet{n}.xml"), sheet.into_bytes()));
        if !sheet_rels.is_empty() {
            parts.push((format!("xl/worksheets/_rels/sheet{n}.xml.rels"), rels(&sheet_rels).into_bytes()));
        }
    }
    if sheets.iter().any(|s| s.1.is_some()) {
        wb_rels.push(("rIdM".into(), "xmlMaps", "xmlMaps.xml".into()));
        types.push_str(&format!(r#"<Override PartName="/xl/xmlMaps.xml" ContentType="{CT}.xmlMaps+xml"/>"#));
        parts.push(("xl/xmlMaps.xml".into(), XML_MAPS.as_bytes().to_vec()));
    }
    types.push_str("</Types>");
    let workbook = format!(r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets>{wb_sheets}</sheets></workbook>"#);
    parts.push(("[Content_Types].xml".into(), types.into_bytes()));
    parts.push(("_rels/.rels".into(), rels(&[("rId1".into(), "officeDocument", "xl/workbook.xml".into())]).into_bytes()));
    parts.push(("xl/workbook.xml".into(), workbook.into_bytes()));
    parts.push(("xl/_rels/workbook.xml.rels".into(), rels(&wb_rels).into_bytes()));
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    make_zip(&refs)
}

const XML_MAPS: &str = r#"<?xml version="1.0"?><MapInfo xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" SelectionNamespaces=""><Schema ID="Schema1"><xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"><xsd:element name="V" type="xsd:integer"/></xsd:schema></Schema><Map ID="1" Name="V_Map" RootElement="V" SchemaID="Schema1" ShowImportExportValidationErrors="0" AutoFit="1" Append="0" PreserveSortAFLayout="1" PreserveFormat="1"/></MapInfo>"#;

fn single_cells(id: u32) -> String {
    format!(
        r#"<?xml version="1.0"?><singleXmlCells xmlns="{NS_MAIN}"><singleXmlCell id="{id}" r="C20" connectionId="0"><xmlCellPr id="1" uniqueName="V"><xmlPr mapId="1" xpath="/V" xmlDataType="integer"/></xmlCellPr></singleXmlCell></singleXmlCells>"#
    )
}

/// Every `id="…"` on a `<table` root and every `<singleXmlCell id="…"` in the saved package.
fn saved_ids(pkg: &[u8]) -> (Vec<u32>, Vec<u32>) {
    let out = write_xlsx(&read_xlsx(pkg).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let mut names: Vec<String> = z.file_names().map(str::to_string).collect();
    names.sort_by_key(|n| (n.len(), n.clone()));
    let (mut tables, mut cells) = (Vec::new(), Vec::new());
    for name in names.iter().filter(|n| n.starts_with("xl/tables/") && n.ends_with(".xml")) {
        let mut s = String::new();
        z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        let el = crate::xml::parse(s.as_bytes()).unwrap();
        if el.name == "table" {
            tables.push(el.attr("id").and_then(|v| v.parse().ok()).expect("table id"));
        } else if el.name == "singleXmlCells" {
            cells.extend(el.kids("singleXmlCell").filter_map(|c| c.attr("id").and_then(|v| v.parse::<u32>().ok())));
        }
    }
    (tables, cells)
}

/// Saved table ids are non-zero, distinct, and never reuse a single mapped cell's id (the two
/// share one id space; Excel repairs the file on a clash). The reader replaces the file's ids with
/// the workbook's object ids, so the exact numbers are not asserted.
fn assert_ids_distinct((tables, cells): (Vec<u32>, Vec<u32>), n_tables: usize) {
    assert_eq!(tables.len(), n_tables, "tables {tables:?}");
    let mut all: Vec<u32> = tables.iter().chain(&cells).copied().collect();
    assert!(!all.contains(&0), "ids {all:?}");
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), tables.len() + cells.len(), "tables {tables:?} cells {cells:?}");
}

#[test]
fn table_id_does_not_collide_with_a_single_mapped_cell() {
    let saved = saved_ids(&package(&[(&[Some(4)], Some(1))]));
    assert_eq!(saved.1, vec![1]);
    assert_ids_distinct(saved, 1);
}

#[test]
fn table_ids_are_distinct_across_sheets() {
    assert_ids_distinct(saved_ids(&package(&[(&[Some(7)], None), (&[Some(2)], None)])), 2);
}

#[test]
fn a_table_without_an_id_does_not_take_a_single_mapped_cells_id() {
    let saved = saved_ids(&package(&[(&[Some(1), None], Some(2))]));
    assert_eq!(saved.1, vec![2]);
    assert_ids_distinct(saved, 2);
}

#[test]
fn duplicate_table_ids_are_made_unique() {
    assert_ids_distinct(saved_ids(&package(&[(&[Some(5), Some(5)], None)])), 2);
}

#[test]
fn tables_without_ids_are_numbered_from_one() {
    assert_eq!(saved_ids(&package(&[(&[None, None], None)])).0, vec![1, 2]);
}
