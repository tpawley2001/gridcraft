//! A PivotTable's own formatting (ECMA-376 Part 1, 18.10.1.40/41 `format`/`formats`, 18.3.1.68
//! `pivotArea`) survives save: each saved `format` points at an equal dxf in `styles.xml`, and
//! its field / item references name the same cache field and shared item as before, even though
//! the cache is rebuilt on save. Excel's own re-save keeps every `format` (measured on 13 real
//! workbooks); before this change ours kept none.

use std::collections::HashMap;
use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RED: &str = "FFC80815";
const BLUE: &str = "FF0000FF";

fn s(r: &str, t: &str) -> String {
    format!(r#"<c r="{r}" t="inlineStr"><is><t>{t}</t></is></c>"#)
}

fn n(r: &str, v: f64) -> String {
    format!(r#"<c r="{r}"><v>{v}</v></c>"#)
}

fn book() -> Vec<u8> {
    let data = format!(
        r#"<sheetData><row r="1">{}{}</row><row r="2">{}{}</row><row r="3">{}{}</row><row r="4">{}{}</row></sheetData>"#,
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
    // Shared items deliberately listed yellow-first, so a rebuilt cache may number them differently.
    let cache = format!(
        r#"<pivotCacheDefinition xmlns="{MAIN}" xmlns:r="{REL}" recordCount="3"><cacheSource type="worksheet"><worksheetSource ref="A1:B4" sheet="Data"/></cacheSource><cacheFields count="2"><cacheField name="Value" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="5" maxValue="20"/></cacheField><cacheField name="Cat" numFmtId="0"><sharedItems count="2"><s v="yellow"/><s v="blue"/></sharedItems></cacheField></cacheFields></pivotCacheDefinition>"#
    );
    // Cache field 1 = Cat on rows; field 0 = Value summed. Format 0: grand total row (red font).
    // Format 1: the label of view item 1 of field 1 (pivotField items x=1, x=0: view item 1 = shared item 0 = "yellow") (blue fill).
    let table = format!(
        r#"<pivotTableDefinition xmlns="{MAIN}" name="PT" cacheId="1" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="1" dataCaption="Values" updatedVersion="6" minRefreshableVersion="3" createdVersion="6" outline="1" outlineData="1"><location ref="D3:E6" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/><pivotFields count="2"><pivotField dataField="1" showAll="0"/><pivotField axis="axisRow" showAll="0"><items count="3"><item x="1"/><item x="0"/><item t="default"/></items></pivotField></pivotFields><rowFields count="1"><field x="1"/></rowFields><rowItems count="3"><i><x/></i><i><x v="1"/></i><i t="grand"><x/></i></rowItems><colItems count="1"><i/></colItems><dataFields count="1"><dataField name="Sum of Value" fld="0" baseField="0" baseItem="0"/></dataFields><formats count="2"><format dxfId="0"><pivotArea dataOnly="0" grandRow="1" outline="0" fieldPosition="0"/></format><format dxfId="1"><pivotArea dataOnly="0" labelOnly="1" outline="0" fieldPosition="0"><references count="1"><reference field="1" count="1"><x v="1"/></reference></references></pivotArea></format></formats><pivotTableStyleInfo name="PivotStyleLight16" showRowHeaders="1" showColHeaders="1" showRowStripes="0" showColStripes="0" showLastColumn="1"/></pivotTableDefinition>"#
    );
    let styles = format!(
        r#"<?xml version="1.0"?><styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><dxfs count="2"><dxf><font><color rgb="{RED}"/></font></dxf><dxf><fill><patternFill patternType="solid"><bgColor rgb="{BLUE}"/></patternFill></fill></dxf></dxfs></styleSheet>"#
    );
    let wb_rels = format!(
        r#"<Relationship Id="rId2" Type="{REL}/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/><Relationship Id="rId3" Type="{REL}/styles" Target="styles.xml"/>"#
    );
    minimal(
        &data,
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

/// The `<dxf>` bodies of the saved styles part, in order.
fn dxfs(styles: &str) -> Vec<String> {
    let Some(start) = styles.find("<dxfs") else { return vec![] };
    let end = styles[start..].find("</dxfs>").map(|e| start + e).unwrap_or(start);
    styles[start..end].split("<dxf>").skip(1).map(|d| d.split("</dxf>").next().unwrap_or("").to_string()).collect()
}

/// Cache field names in order, and each field's shared string items in order.
fn cache_fields(cache: &str) -> Vec<(String, Vec<String>)> {
    cache
        .split("<cacheField ")
        .skip(1)
        .map(|f| {
            let body = f.split("</cacheField>").next().unwrap_or(f);
            let name = attr(&format!(" {body}"), "name").unwrap_or_default();
            let items = body.split("<s v=\"").skip(1).filter_map(|s| s.split('"').next()).map(str::to_string).collect();
            (name, items)
        })
        .collect()
}

/// The saved `<format>` elements of the one PivotTable: (dxfId, the format's inner XML).
fn formats(table: &str) -> Vec<(usize, String)> {
    let Some(start) = table.find("<formats") else { return vec![] };
    let end = table[start..].find("</formats>").map(|e| start + e).expect("formats closes");
    table[start..end]
        .split("<format ")
        .skip(1)
        .map(|f| {
            let head = f.split('>').next().unwrap();
            let id = attr(&format!(" {head}"), "dxfId").expect("format has a dxfId").parse().unwrap();
            (id, f.split("</format>").next().unwrap().to_string())
        })
        .collect()
}

fn check(saved: &[u8]) {
    let parts = unzip(saved);
    let tables: Vec<&String> =
        parts.iter().filter(|(n, _)| n.starts_with("xl/pivotTables/pivotTable") && n.ends_with(".xml")).map(|(_, x)| x).collect();
    assert_eq!(tables.len(), 1, "one PivotTable saved");
    let table = tables[0];
    let cache = parts.iter().find(|(n, _)| n.starts_with("xl/pivotCache/pivotCacheDefinition") && n.ends_with(".xml")).unwrap().1;
    let styles = &parts["xl/styles.xml"];
    let dx = dxfs(styles);
    let fs = formats(table);
    assert_eq!(fs.len(), 2, "both formats kept: {table}");
    assert!(table.contains("<formats count=\"2\""), "formats count: {table}");

    // Schema order (CT_pivotTableDefinition): dataFields < formats < pivotTableStyleInfo.
    let pos = |t: &str| table.find(t).unwrap_or_else(|| panic!("{t} in {table}"));
    assert!(pos("<dataFields") < pos("<formats") && pos("<formats") < pos("<pivotTableStyleInfo"), "{table}");

    let grand = fs.iter().find(|(_, x)| x.contains("grandRow=\"1\"")).expect("grand-row format");
    assert!(dx.get(grand.0).is_some_and(|d| d.contains(RED)), "grand row dxf {} = red font: {dx:?}", grand.0);

    let label = fs.iter().find(|(_, x)| x.contains("labelOnly=\"1\"")).expect("label format");
    assert!(dx.get(label.0).is_some_and(|d| d.contains(BLUE)), "label dxf {} = blue fill: {dx:?}", label.0);
    let reference = label.1.split("<reference ").nth(1).expect("reference kept");
    let field: usize = attr(&format!(" {reference}"), "field").unwrap().parse().unwrap();
    let item: usize = reference.split("<x v=\"").nth(1).and_then(|s| s.split('"').next()).unwrap().parse().unwrap();
    let fields = cache_fields(cache);
    assert_eq!(fields[field].0, "Cat", "reference names the Cat field: {cache}");
    assert_eq!(fields[field].1[item], "yellow", "reference names the yellow item: {cache}");
}

#[test]
fn pivot_formats_survive_save() {
    let (wb, _) = read_xlsx(&book()).unwrap();
    check(&write_xlsx(&wb).unwrap());
}

#[test]
fn pivot_formats_survive_a_second_save() {
    let (wb, _) = read_xlsx(&book()).unwrap();
    let (wb2, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    check(&write_xlsx(&wb2).unwrap());
}

/// A one-field PivotTable whose row field's shared items are [A, B, C] but whose pivotField
/// lists them in view order C, A, B, with one format whose reference is `<x v="0"/>`.
fn view_order_book(cache_index: bool) -> Vec<u8> {
    let data = format!(
        r#"<sheetData><row r="1">{}{}</row><row r="2">{}{}</row><row r="3">{}{}</row><row r="4">{}{}</row></sheetData>"#,
        s("A1", "Cat"),
        s("B1", "Value"),
        s("A2", "A"),
        n("B2", 1.0),
        s("A3", "B"),
        n("B3", 2.0),
        s("A4", "C"),
        n("B4", 3.0)
    );
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/pivotTable" Target="../pivotTables/pivotTable1.xml"/></Relationships>"#
    );
    let pt_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/pivotCacheDefinition" Target="../pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#
    );
    let cache = format!(
        r#"<pivotCacheDefinition xmlns="{MAIN}" xmlns:r="{REL}" recordCount="3"><cacheSource type="worksheet"><worksheetSource ref="A1:B4" sheet="Data"/></cacheSource><cacheFields count="2"><cacheField name="Cat" numFmtId="0"><sharedItems count="3"><s v="A"/><s v="B"/><s v="C"/></sharedItems></cacheField><cacheField name="Value" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" minValue="1" maxValue="3"/></cacheField></cacheFields></pivotCacheDefinition>"#
    );
    let ci = if cache_index { r#" cacheIndex="1""# } else { "" };
    let table = format!(
        r#"<pivotTableDefinition xmlns="{MAIN}" name="PT" cacheId="1" dataCaption="Values" outline="1" outlineData="1"><location ref="D3:E7" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/><pivotFields count="2"><pivotField axis="axisRow" showAll="0"><items count="4"><item x="2"/><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField dataField="1" showAll="0"/></pivotFields><rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="Sum of Value" fld="1"/></dataFields><formats count="1"><format dxfId="0"><pivotArea dataOnly="0" labelOnly="1"{ci}><references count="1"><reference field="0" count="1"><x v="0"/></reference></references></pivotArea></format></formats></pivotTableDefinition>"#
    );
    let styles = format!(
        r#"<?xml version="1.0"?><styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><dxfs count="1"><dxf><font><color rgb="{RED}"/></font></dxf></dxfs></styleSheet>"#
    );
    let wb_rels = format!(
        r#"<Relationship Id="rId2" Type="{REL}/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/><Relationship Id="rId3" Type="{REL}/styles" Target="styles.xml"/>"#
    );
    minimal(
        &data,
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

/// The item of the one reference of the one format of the one PivotTable.
fn format_item(wb: &gridcraft_model::Workbook) -> gridcraft_model::PivotAreaItem {
    let pt = &wb.sheet(0).unwrap().pivots[0];
    assert_eq!(pt.formats.len(), 1, "format kept: {:?}", pt.formats);
    pt.formats[0].area.references[0].items[0].clone()
}

#[test]
fn pivot_format_item_without_cache_index_is_a_view_position() {
    use gridcraft_model::PivotAreaItem::Label;
    let (wb, _) = read_xlsx(&view_order_book(false)).unwrap();
    assert_eq!(format_item(&wb), Label("C".into()), "v=0 is the pivotField's first item (x=2 = C)");
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(format_item(&back), Label("C".into()));
}

#[test]
fn pivot_format_item_with_cache_index_is_a_shared_item() {
    use gridcraft_model::PivotAreaItem::Label;
    let (wb, _) = read_xlsx(&view_order_book(true)).unwrap();
    assert_eq!(format_item(&wb), Label("A".into()), "cacheIndex=1: v=0 is shared item 0 (A)");
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(format_item(&back), Label("A".into()));
}
