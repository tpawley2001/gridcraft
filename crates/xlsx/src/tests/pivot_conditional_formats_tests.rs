//! A PivotTable's conditional formats (ECMA-376 Part 1, 18.10.1.18/19 `conditionalFormat` /
//! `conditionalFormats`, 18.3.1.68 `pivotArea`) survive save. Excel requires each one's `priority`
//! to equal the priority of a `cfRule` inside a `<conditionalFormatting pivot="1">` on the
//! PivotTable's sheet, so the saved priority follows the sheet writer's 1..n renumbering. A
//! conditional format with no matching pivot rule is dropped.

use std::collections::HashMap;
use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn s(r: &str, t: &str) -> String {
    format!(r#"<c r="{r}" t="inlineStr"><is><t>{t}</t></is></c>"#)
}

fn n(r: &str, v: f64) -> String {
    format!(r#"<c r="{r}"><v>{v}</v></c>"#)
}

/// Sheet "Data" with a normal rule (priority 1) and a pivot rule (priority 5) on the PivotTable's
/// values, and a PivotTable whose `conditionalFormats` hold `pivot_cfs`.
fn book(pivot_cfs: &str) -> Vec<u8> {
    let body = format!(
        r#"<sheetData><row r="1">{}{}</row><row r="2">{}{}</row><row r="3">{}{}</row><row r="4">{}{}</row></sheetData><conditionalFormatting sqref="B2:B4"><cfRule type="cellIs" dxfId="0" priority="1" operator="greaterThan"><formula>15</formula></cfRule></conditionalFormatting><conditionalFormatting pivot="1" sqref="E4:E5"><cfRule type="cellIs" dxfId="1" priority="5" operator="greaterThan"><formula>12</formula></cfRule></conditionalFormatting>"#,
        s("A1", "Cat"),
        s("B1", "Value"),
        s("A2", "blue"),
        n("B2", 10.0),
        s("A3", "yellow"),
        n("B3", 20.0),
        s("A4", "blue"),
        n("B4", 5.0)
    );
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/pivotTable" Target="../pivotTables/pivotTable1.xml"/></Relationships>"#
    );
    let pt_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/pivotCacheDefinition" Target="../pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#
    );
    let cache = format!(
        r#"<pivotCacheDefinition xmlns="{MAIN}" xmlns:r="{REL}" recordCount="3"><cacheSource type="worksheet"><worksheetSource ref="A1:B4" sheet="Data"/></cacheSource><cacheFields count="2"><cacheField name="Cat" numFmtId="0"><sharedItems count="2"><s v="blue"/><s v="yellow"/></sharedItems></cacheField><cacheField name="Value" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="5" maxValue="20"/></cacheField></cacheFields></pivotCacheDefinition>"#
    );
    let table = format!(
        r#"<pivotTableDefinition xmlns="{MAIN}" name="PT" cacheId="1" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="1" dataCaption="Values" updatedVersion="6" minRefreshableVersion="3" createdVersion="6" outline="1" outlineData="1"><location ref="D3:E6" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/><pivotFields count="2"><pivotField axis="axisRow" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField dataField="1" showAll="0"/></pivotFields><rowFields count="1"><field x="0"/></rowFields><rowItems count="3"><i><x/></i><i><x v="1"/></i><i t="grand"><x/></i></rowItems><colItems count="1"><i/></colItems><dataFields count="1"><dataField name="Sum of Value" fld="1" baseField="0" baseItem="0"/></dataFields>{pivot_cfs}<pivotTableStyleInfo name="PivotStyleLight16" showRowHeaders="1" showColHeaders="1" showRowStripes="0" showColStripes="0" showLastColumn="1"/></pivotTableDefinition>"#
    );
    let styles = format!(
        r#"<?xml version="1.0"?><styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><dxfs count="2"><dxf><font><color rgb="FFC80815"/></font></dxf><dxf><fill><patternFill patternType="solid"><bgColor rgb="FF0000FF"/></patternFill></fill></dxf></dxfs></styleSheet>"#
    );
    let wb_rels = format!(
        r#"<Relationship Id="rId2" Type="{REL}/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/><Relationship Id="rId3" Type="{REL}/styles" Target="styles.xml"/>"#
    );
    minimal(
        &body,
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &sheet_rels),
            ("xl/pivotTables/pivotTable1.xml", &table),
            ("xl/pivotTables/_rels/pivotTable1.xml.rels", &pt_rels),
            ("xl/pivotCache/pivotCacheDefinition1.xml", &cache),
            ("xl/styles.xml", &styles),
        ],
        &wb_rels,
        r#"<pivotCaches><pivotCache cacheId="1" r:id="rId2"/></pivotCaches>"#,
    )
}

/// Priority 5 on the values (data field -2 = 4294967294) plus an orphan at priority 9.
const DATA_CF: &str = r#"<conditionalFormats count="2"><conditionalFormat priority="5"><pivotAreas count="1"><pivotArea type="data" outline="0" collapsedLevelsAreSubtotals="1" fieldPosition="0"><references count="1"><reference field="4294967294" count="1" selected="0"><x v="0"/></reference></references></pivotArea></pivotAreas></conditionalFormat><conditionalFormat priority="9"><pivotAreas count="1"><pivotArea type="data" outline="0" fieldPosition="0"><references count="1"><reference field="4294967294" count="1" selected="0"><x v="0"/></reference></references></pivotArea></pivotAreas></conditionalFormat></conditionalFormats>"#;

/// Priority 5, field scope on rows.
const FIELD_CF: &str = r#"<conditionalFormats count="1"><conditionalFormat scope="field" type="row" priority="5"><pivotAreas count="1"><pivotArea outline="0" fieldPosition="0"><references count="2"><reference field="4294967294" count="1" selected="0"><x v="0"/></reference><reference field="0" count="1" selected="0"><x v="1"/></reference></references></pivotArea></pivotAreas></conditionalFormat></conditionalFormats>"#;

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

fn attr(tag: &str, name: &str) -> Option<String> {
    tag.split(&format!(" {name}=\"")).nth(1).and_then(|s| s.split('"').next()).map(str::to_string)
}

fn part(parts: &HashMap<String, String>, prefix: &str) -> String {
    let mut names: Vec<&String> = parts.keys().filter(|k| k.starts_with(prefix) && k.ends_with(".xml")).collect();
    names.sort();
    assert_eq!(names.len(), 1, "exactly one {prefix}*.xml part, got {names:?}");
    parts[names[0]].clone()
}

/// Priority of the cfRule inside the sheet's `pivot="1"` conditionalFormatting.
fn pivot_rule_priority(sheet: &str) -> String {
    let at = sheet.find("pivot=\"1\"").expect("sheet keeps its pivot conditionalFormatting");
    let rest = &sheet[at..];
    let end = rest.find("</conditionalFormatting>").unwrap();
    let rule = rest[..end].split("<cfRule").nth(1).expect("pivot block has a cfRule");
    attr(&format!(" {rule}"), "priority").expect("cfRule priority")
}

/// The saved `<conditionalFormat` elements: (opening tag attrs, inner XML).
fn pivot_cfs(table: &str) -> Vec<(String, String)> {
    table
        .split("<conditionalFormat ")
        .skip(1)
        .map(|c| {
            let head = c.split('>').next().unwrap().to_string();
            let body = c.split("</conditionalFormat>").next().unwrap().to_string();
            (format!(" {head}"), body)
        })
        .collect()
}

fn saved(book_bytes: &[u8], twice: bool) -> HashMap<String, String> {
    let (wb, _) = read_xlsx(book_bytes).unwrap();
    let mut bytes = write_xlsx(&wb).unwrap();
    if twice {
        let (wb2, _) = read_xlsx(&bytes).unwrap();
        bytes = write_xlsx(&wb2).unwrap();
    }
    unzip(&bytes)
}

fn check_data(parts: &HashMap<String, String>) {
    let sheet = part(parts, "xl/worksheets/sheet");
    let table = part(parts, "xl/pivotTables/pivotTable");
    let rule_prio = pivot_rule_priority(&sheet);
    assert_eq!(rule_prio, "2", "sheet rules are renumbered 1..n (normal 1, pivot 2)");
    let cfs = pivot_cfs(&table);
    assert_eq!(cfs.len(), 1, "the matched conditional format is kept, the orphan (priority 9) dropped: {table}");
    let (head, body) = &cfs[0];
    assert_eq!(attr(head, "priority").as_deref(), Some(rule_prio.as_str()), "priority follows the sheet rule");
    assert!(body.contains("type=\"data\""), "pivotArea type kept: {body}");
    assert!(body.contains("field=\"4294967294\""), "data-field reference kept: {body}");
    assert!(body.contains("<x v=\"0\"/>") || body.contains("<x/>"), "data field index kept: {body}");
    assert!(table.contains("<conditionalFormats count=\"1\">"), "count matches the kept element: {table}");
    let cf_at = table.find("<conditionalFormats").unwrap();
    let style_at = table.find("<pivotTableStyleInfo").expect("style info kept");
    assert!(cf_at < style_at, "conditionalFormats comes before pivotTableStyleInfo");
    if let Some(formats_at) = table.find("<formats") {
        assert!(formats_at < cf_at, "conditionalFormats comes after formats");
    }
}

#[test]
fn pivot_conditional_format_survives_save_with_renumbered_priority() {
    check_data(&saved(&book(DATA_CF), false));
}

#[test]
fn pivot_conditional_format_survives_a_second_save() {
    check_data(&saved(&book(DATA_CF), true));
}

#[test]
fn pivot_conditional_format_keeps_scope_type_and_item_reference() {
    for twice in [false, true] {
        let parts = saved(&book(FIELD_CF), twice);
        let table = part(&parts, "xl/pivotTables/pivotTable");
        let cache = part(&parts, "xl/pivotCache/pivotCacheDefinition");
        let cfs = pivot_cfs(&table);
        assert_eq!(cfs.len(), 1, "{table}");
        let (head, body) = &cfs[0];
        assert_eq!(attr(head, "scope").as_deref(), Some("field"));
        assert_eq!(attr(head, "type").as_deref(), Some("row"));
        assert_eq!(attr(head, "priority").as_deref(), Some("2"));
        // The field-0 reference still names shared item "yellow" in the (possibly rebuilt) cache.
        let r = body.split("<reference ").find(|r| r.starts_with("field=\"0\"")).expect("field 0 reference kept");
        let x: usize = attr(&format!(" {}", r.split("<x ").nth(1).unwrap()), "v").unwrap().parse().unwrap();
        let cat = cache.split("<cacheField ").nth(1).unwrap();
        let items: Vec<&str> = cat.split("<s v=\"").skip(1).filter_map(|s| s.split('"').next()).collect();
        // pivotField items map view index -> shared index; read it from the saved table.
        let pf = table.split("<pivotField ").nth(1).unwrap();
        let shared: Vec<usize> = pf.split("<item x=\"").skip(1).filter_map(|s| s.split('"').next()).filter_map(|v| v.parse().ok()).collect();
        let shared_ix = if shared.is_empty() { x } else { shared[x] };
        assert_eq!(items[shared_ix], "yellow", "item reference follows the cache: {body}");
    }
}

#[test]
fn no_pivot_rule_means_no_conditional_formats_element() {
    // Only the orphan (priority 9): nothing to keep, and no empty element.
    let orphan = r#"<conditionalFormats count="1"><conditionalFormat priority="9"><pivotAreas count="1"><pivotArea type="data" outline="0" fieldPosition="0"><references count="1"><reference field="4294967294" count="1" selected="0"><x v="0"/></reference></references></pivotArea></pivotAreas></conditionalFormat></conditionalFormats>"#;
    let parts = saved(&book(orphan), false);
    let table = part(&parts, "xl/pivotTables/pivotTable");
    assert!(!table.contains("<conditionalFormats"), "{table}");
}
