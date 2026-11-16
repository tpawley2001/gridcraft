//! A PivotTable lists each field at most once over its row, column and page areas. A grouped
//! cache field we can't model falls back to its base field; when the base field is on the
//! PivotTable too, the copy is dropped so the saved file stays one Excel can open.

use std::collections::HashMap;
use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const PT_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/></Relationships>"#;
const PT_CACHE_RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition" Target="../pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#;

/// Cat (text), Value (numbers, parent of a group), Value2 (manual discrete group of Value, not a database field).
const CACHE: &str = r#"<pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cacheSource type="worksheet"><worksheetSource ref="A1:B4"/></cacheSource><cacheFields count="3"><cacheField name="Cat" numFmtId="0"><sharedItems count="2"><s v="blue"/><s v="yellow"/></sharedItems></cacheField><cacheField name="Value" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="5" maxValue="20" count="3"><n v="10"/><n v="20"/><n v="5"/></sharedItems><fieldGroup par="2"/></cacheField><cacheField name="Value2" numFmtId="0" databaseField="0"><fieldGroup base="1"><discretePr count="3"><x v="1"/><x v="1"/><x v="0"/></discretePr><groupItems count="2"><n v="5"/><s v="Group1"/></groupItems></fieldGroup></cacheField></cacheFields></pivotCacheDefinition>"#;

fn table(pivot_fields: &str, axes: &str) -> String {
    format!(
        r#"<pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="PT" cacheId="1" dataCaption="Values"><location ref="D3:F9" firstHeaderRow="1" firstDataRow="2" firstDataCol="1"/><pivotFields count="3">{pivot_fields}</pivotFields>{axes}</pivotTableDefinition>"#
    )
}

fn book(table: &str) -> Vec<u8> {
    minimal(
        "<sheetData/>",
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", PT_RELS),
            ("xl/pivotTables/pivotTable1.xml", table),
            ("xl/pivotTables/_rels/pivotTable1.xml.rels", PT_CACHE_RELS),
            ("xl/pivotCache/pivotCacheDefinition1.xml", CACHE),
        ],
        "",
        "",
    )
}

fn unzip(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            out.insert(f.name().to_string(), s);
        }
    }
    out
}

/// The `x` / `fld` indices listed in one area element of a saved pivot table, and its `count`.
fn area(xml: &str, elem: &str, attr: &str) -> (Option<usize>, Vec<i64>) {
    let Some(start) = xml.find(&format!("<{elem}")) else { return (None, vec![]) };
    let rest = &xml[start..];
    let end = rest.find(&format!("</{elem}>")).unwrap_or_else(|| rest.find("/>").unwrap() + 2);
    let body = &rest[..end];
    let count = body.split("count=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse().ok());
    let key = format!(" {attr}=\"");
    let xs = body.split(&key).skip(1).filter_map(|s| s.split('"').next()).filter_map(|s| s.parse().ok()).collect();
    (count, xs)
}

/// Every saved pivot table lists each field at most once over rows, columns and pages,
/// and each area's `count` matches what it lists.
fn assert_saved_axes_unique(saved: &[u8]) {
    let parts = unzip(saved);
    let tables: Vec<&String> =
        parts.iter().filter(|(n, _)| n.starts_with("xl/pivotTables/pivotTable") && n.ends_with(".xml")).map(|(_, x)| x).collect();
    assert!(!tables.is_empty(), "the pivot table is saved");
    for x in tables {
        let mut seen = Vec::new();
        for (elem, attr) in [("rowFields", "x"), ("colFields", "x"), ("pageFields", "fld")] {
            let (count, xs) = area(x, elem, attr);
            if let Some(c) = count {
                assert_eq!(c, xs.len(), "{elem} count matches its children in {x}");
            }
            for i in xs.into_iter().filter(|i| *i >= 0) {
                assert!(!seen.contains(&i), "field {i} listed twice over rows/columns/pages in {x}");
                seen.push(i);
            }
        }
    }
}

#[test]
fn ungrouped_copy_on_rows_beside_its_base_is_dropped() {
    // rows = Value2 (group of Value), Value; columns = Cat; data = Sum of Value.
    let t = table(
        r#"<pivotField axis="axisCol" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField axis="axisRow" dataField="1" showAll="0"><items count="4"><item x="0"/><item x="1"/><item x="2"/><item t="default"/></items></pivotField><pivotField axis="axisRow" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField>"#,
        r#"<rowFields count="2"><field x="2"/><field x="1"/></rowFields><colFields count="1"><field x="0"/></colFields><dataFields count="1"><dataField name="Sum of Value" fld="1" baseField="0" baseItem="0"/></dataFields>"#,
    );
    let (wb, report) = read_xlsx(&book(&t)).unwrap();
    let pts = &wb.sheet(0).unwrap().pivots;
    assert_eq!(pts.len(), 1, "{:?}", report.warnings);
    let pt = &pts[0];
    let rows: Vec<&str> = pt.rows.iter().map(|f| f.source_col.as_str()).collect();
    let cols: Vec<&str> = pt.columns.iter().map(|f| f.source_col.as_str()).collect();
    assert_eq!(rows, ["Value"], "the base field stays once; its ungrouped copy is dropped");
    assert_eq!(cols, ["Cat"]);
    assert_eq!(pt.values.len(), 1, "the data field is untouched");
    assert!(report.warnings.iter().any(|w| w.contains("Value2") && w.contains("duplicates")), "{:?}", report.warnings);
    assert_saved_axes_unique(&write_xlsx(&wb).unwrap());
}

#[test]
fn ungrouped_copy_on_columns_while_its_base_is_on_rows_is_dropped() {
    // rows = Value; columns = Cat, Value2 (group of Value): the copy on columns goes.
    let t = table(
        r#"<pivotField axis="axisCol" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField axis="axisRow" showAll="0"><items count="4"><item x="0"/><item x="1"/><item x="2"/><item t="default"/></items></pivotField><pivotField axis="axisCol" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField>"#,
        r#"<rowFields count="1"><field x="1"/></rowFields><colFields count="2"><field x="0"/><field x="2"/></colFields>"#,
    );
    let (wb, report) = read_xlsx(&book(&t)).unwrap();
    let pt = &wb.sheet(0).unwrap().pivots[0];
    let rows: Vec<&str> = pt.rows.iter().map(|f| f.source_col.as_str()).collect();
    let cols: Vec<&str> = pt.columns.iter().map(|f| f.source_col.as_str()).collect();
    assert_eq!(rows, ["Value"]);
    assert_eq!(cols, ["Cat"], "{:?}", report.warnings);
    assert_saved_axes_unique(&write_xlsx(&wb).unwrap());
}

#[test]
fn ungrouped_copy_as_page_filter_beside_its_base_on_rows_is_dropped() {
    let t = table(
        r#"<pivotField showAll="0"/><pivotField axis="axisRow" showAll="0"><items count="4"><item x="0"/><item x="1"/><item x="2"/><item t="default"/></items></pivotField><pivotField axis="axisPage" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField>"#,
        r#"<rowFields count="1"><field x="1"/></rowFields><pageFields count="1"><pageField fld="2" hier="-1"/></pageFields>"#,
    );
    let (wb, report) = read_xlsx(&book(&t)).unwrap();
    let pt = &wb.sheet(0).unwrap().pivots[0];
    let rows: Vec<&str> = pt.rows.iter().map(|f| f.source_col.as_str()).collect();
    assert_eq!(rows, ["Value"]);
    assert!(
        !pt.filters.iter().any(|f| f.source_col.eq_ignore_ascii_case("Value")) || pt.rows.is_empty(),
        "Value is not both a row field and a page filter: {:?} / {:?}",
        pt.filters,
        report.warnings
    );
    assert_saved_axes_unique(&write_xlsx(&wb).unwrap());
}

#[test]
fn a_field_listed_twice_in_the_file_is_kept_once() {
    let t = table(
        r#"<pivotField axis="axisRow" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField dataField="1" showAll="0"/><pivotField showAll="0"/>"#,
        r#"<rowFields count="2"><field x="0"/><field x="0"/></rowFields><dataFields count="1"><dataField name="Sum of Value" fld="1"/></dataFields>"#,
    );
    let (wb, _) = read_xlsx(&book(&t)).unwrap();
    let pt = &wb.sheet(0).unwrap().pivots[0];
    let rows: Vec<&str> = pt.rows.iter().map(|f| f.source_col.as_str()).collect();
    assert_eq!(rows, ["Cat"]);
    assert_saved_axes_unique(&write_xlsx(&wb).unwrap());
}

#[test]
fn a_row_field_may_also_be_a_data_field() {
    // Excel allows the same field on rows and in Values: nothing is dropped.
    let t = table(
        r#"<pivotField axis="axisRow" dataField="1" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField showAll="0"/><pivotField showAll="0"/>"#,
        r#"<rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="Count of Cat" fld="0" subtotal="count"/></dataFields>"#,
    );
    let (wb, report) = read_xlsx(&book(&t)).unwrap();
    let pt = &wb.sheet(0).unwrap().pivots[0];
    assert_eq!(pt.rows.len(), 1);
    assert_eq!(pt.values.len(), 1);
    assert!(!report.warnings.iter().any(|w| w.contains("duplicates")), "{:?}", report.warnings);
    assert_saved_axes_unique(&write_xlsx(&wb).unwrap());
}
