//! PivotTables whose worksheet source no longer resolves (deleted sheet, deleted table or name, a
//! table-column reference) are kept on XLSX save from the parts they were read from, as Excel
//! keeps them: the report still shows its cached results, only Refresh fails.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use super::{make_zip, minimal};
use crate::{read_xlsx, write_xlsx};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";
const CT_CACHE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
const CT_RECORDS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml";

const GONE_SHEET: &str = r#"<worksheetSource ref="A1:B3" sheet="Gone"/>"#;
const GONE_TABLE: &str = r#"<worksheetSource name="NoSuchTable"/>"#;
const TABLE_COLUMN: &str = r#"<worksheetSource name="Tbl[Col1]"/>"#;
const LIVE: &str = r#"<worksheetSource ref="A1:B3" sheet="Data"/>"#;

const SHEET: &str = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Region</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>North</t></is></c><c r="B2"><v>42.5</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>South</t></is></c><c r="B3"><v>99.25</v></c></row></sheetData>"#;

fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut m = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        m.insert(f.name().to_string(), b);
    }
    m
}

fn text(parts: &HashMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(parts.get(name).unwrap_or_else(|| panic!("part {name} is missing")).clone()).unwrap()
}

/// The value of attribute `name` in the element text `el` (`<tag a="1" b="2"`).
fn attr(el: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let start = el.find(&key)? + key.len();
    let end = el[start..].find('"')? + start;
    Some(el[start..end].to_string())
}

/// The start tags of every `tag` element in `xml`.
fn tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag} ");
    xml.match_indices(&open).map(|(i, _)| &xml[i..i + xml[i..].find('>').unwrap()]).collect()
}

/// (type, resolved part name) of a part's internal relationships.
fn rels(parts: &HashMap<String, Vec<u8>>, part: &str) -> Vec<(String, String)> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let Some(bytes) = parts.get(&rels_name) else { return Vec::new() };
    let xml = String::from_utf8(bytes.clone()).unwrap();
    let mut out = Vec::new();
    for el in tags(&xml, "Relationship") {
        if attr(el, "TargetMode").as_deref() == Some("External") {
            continue;
        }
        let target = attr(el, "Target").unwrap();
        let mut segs: Vec<&str> = if target.starts_with('/') { Vec::new() } else { dir.split('/').filter(|s| !s.is_empty()).collect() };
        for s in target.trim_start_matches('/').split('/') {
            match s {
                ".." => {
                    segs.pop();
                }
                "." | "" => {}
                s => segs.push(s),
            }
        }
        out.push((attr(el, "Type").unwrap(), segs.join("/")));
    }
    out
}

fn kind(parts: &HashMap<String, Vec<u8>>, part: &str, suffix: &str) -> Vec<String> {
    rels(parts, part).into_iter().filter(|(t, _)| t.ends_with(suffix)).map(|(_, p)| p).collect()
}

fn assert_override(parts: &HashMap<String, Vec<u8>>, part: &str, content_type: &str) {
    let ct = text(parts, "[Content_Types].xml");
    let found = tags(&ct, "Override").into_iter().find(|o| attr(o, "PartName").as_deref() == Some(&format!("/{part}")));
    let found = found.unwrap_or_else(|| panic!("no content type Override for /{part}"));
    assert_eq!(attr(found, "ContentType").as_deref(), Some(content_type), "content type of /{part}");
}

/// A one-sheet workbook ("Data", A1:B3 = Region/Amount, two records) with one PivotTable per
/// `tables` entry (name, index into `caches`) and one pivot cache per `caches` entry (its
/// `worksheetSource` element). Cache `c` has cacheId `c + 1` and its own records part.
fn package(tables: &[(&str, usize)], caches: &[&str]) -> Vec<u8> {
    let mut extra: Vec<(String, String)> = Vec::new();
    let mut sheet_rels = String::new();
    for (i, (name, c)) in tables.iter().enumerate() {
        let n = i + 1;
        sheet_rels.push_str(&format!(r#"<Relationship Id="rId{n}" Type="{REL}/pivotTable" Target="../pivotTables/pivotTable{n}.xml"/>"#));
        let (top, bottom) = (3 + 10 * i, 5 + 10 * i);
        extra.push((
            format!("xl/pivotTables/pivotTable{n}.xml"),
            format!(
                r#"<?xml version="1.0"?><pivotTableDefinition xmlns="{MAIN}" name="{name}" cacheId="{id}" dataCaption="Amounts kept"><location ref="D{top}:E{bottom}" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/><pivotFields count="2"><pivotField axis="axisRow" showAll="0"><items count="3"><item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField dataField="1" showAll="0"/></pivotFields><rowFields count="1"><field x="0"/></rowFields><rowItems count="3"><i><x/></i><i><x v="1"/></i><i t="grand"><x/></i></rowItems><colItems count="1"><i/></colItems><dataFields count="1"><dataField name="Sum of Amount" fld="1" baseField="0" baseItem="0"/></dataFields></pivotTableDefinition>"#,
                id = c + 1
            ),
        ));
        extra.push((
            format!("xl/pivotTables/_rels/pivotTable{n}.xml.rels"),
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/pivotCacheDefinition" Target="../pivotCache/pivotCacheDefinition{}.xml"/></Relationships>"#,
                c + 1
            ),
        ));
    }
    extra.push((
        "xl/worksheets/_rels/sheet1.xml.rels".into(),
        format!(
            r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{sheet_rels}</Relationships>"#
        ),
    ));
    let (mut wb_rels, mut wb_caches) = (String::new(), String::new());
    for (c, source) in caches.iter().enumerate() {
        let n = c + 1;
        wb_rels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="{REL}/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition{n}.xml"/>"#,
            20 + c
        ));
        wb_caches.push_str(&format!(r#"<pivotCache cacheId="{n}" r:id="rId{}"/>"#, 20 + c));
        extra.push((
            format!("xl/pivotCache/pivotCacheDefinition{n}.xml"),
            format!(
                r#"<?xml version="1.0"?><pivotCacheDefinition xmlns="{MAIN}" xmlns:r="{REL}" r:id="rId1" refreshedBy="Cache Keeper" refreshedDate="45000.5" createdVersion="8" refreshedVersion="8" recordCount="2"><cacheSource type="worksheet">{source}</cacheSource><cacheFields count="2"><cacheField name="Region" numFmtId="0"><sharedItems count="2"><s v="North"/><s v="South"/></sharedItems></cacheField><cacheField name="Amount" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" minValue="42.5" maxValue="99.25"/></cacheField></cacheFields></pivotCacheDefinition>"#
            ),
        ));
        extra.push((
            format!("xl/pivotCache/_rels/pivotCacheDefinition{n}.xml.rels"),
            format!(
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/pivotCacheRecords" Target="pivotCacheRecords{n}.xml"/></Relationships>"#
            ),
        ));
        extra.push((
            format!("xl/pivotCache/pivotCacheRecords{n}.xml"),
            format!(
                r#"<?xml version="1.0"?><pivotCacheRecords xmlns="{MAIN}" count="2"><r><x v="0"/><n v="42.5"/></r><r><x v="1"/><n v="99.25"/></r></pivotCacheRecords>"#
            ),
        ));
    }
    let refs: Vec<(&str, &str)> = extra.iter().map(|(n, d)| (n.as_str(), d.as_str())).collect();
    let bytes = minimal(SHEET, &refs, &wb_rels, "");
    // `pivotCaches` belongs after `sheets` in the workbook part.
    let mut parts = unzip(&bytes);
    let wbx = String::from_utf8(parts.remove("xl/workbook.xml").unwrap()).unwrap();
    let wbx = wbx.replace("</workbook>", &format!("<pivotCaches>{wb_caches}</pivotCaches></workbook>"));
    let mut list: Vec<(String, Vec<u8>)> = parts.into_iter().collect();
    list.push(("xl/workbook.xml".into(), wbx.into_bytes()));
    let refs: Vec<(&str, &[u8])> = list.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    make_zip(&refs)
}

/// One saved PivotTable: its name, and the text of its table, cache definition and records parts.
struct Kept {
    name: String,
    table: String,
    cache_part: String,
    cache: String,
    records: Option<String>,
}

/// Checks the saved package's PivotTable wiring (every relationship target exists; sheet → table →
/// cache → records; the workbook's `pivotCaches` entry; content types; unique cache ids) and
/// returns the PivotTables in sheet-relationship order.
fn saved_pivots(bytes: &[u8]) -> Vec<Kept> {
    let parts = unzip(bytes);
    for name in parts.keys().filter(|n| n.ends_with(".rels")) {
        let owner = name.replace("_rels/", "").trim_end_matches(".rels").to_string();
        for (_, target) in rels(&parts, &owner) {
            assert!(parts.contains_key(&target), "{name} points at {target}, which is not in the package");
        }
    }
    let wbx = text(&parts, "xl/workbook.xml");
    let wb_caches: Vec<(String, String)> =
        tags(&wbx, "pivotCache").into_iter().map(|e| (attr(e, "cacheId").unwrap(), attr(e, "r:id").unwrap())).collect();
    let ids: HashSet<&String> = wb_caches.iter().map(|(id, _)| id).collect();
    assert_eq!(ids.len(), wb_caches.len(), "cacheId values must be unique: {wbx}");
    let wb_rels_xml = text(&parts, "xl/_rels/workbook.xml.rels");
    let mut out = Vec::new();
    for table_part in kind(&parts, "xl/worksheets/sheet1.xml", "/pivotTable") {
        assert_override(&parts, &table_part, CT_TABLE);
        let table = text(&parts, &table_part);
        let root = tags(&table, "pivotTableDefinition")[0];
        let cache_parts = kind(&parts, &table_part, "/pivotCacheDefinition");
        assert_eq!(cache_parts.len(), 1, "{table_part} needs exactly one cache relationship");
        let cache_part = cache_parts[0].clone();
        assert_override(&parts, &cache_part, CT_CACHE);
        // The workbook lists the cache under the id the table uses, and that entry's relationship
        // reaches the same cache part.
        let cache_id = attr(root, "cacheId").expect("pivotTableDefinition cacheId");
        let (_, rid) = wb_caches.iter().find(|(id, _)| *id == cache_id).unwrap_or_else(|| panic!("workbook has no pivotCache {cache_id}: {wbx}"));
        let rel = tags(&wb_rels_xml, "Relationship")
            .into_iter()
            .find(|r| attr(r, "Id").as_deref() == Some(rid))
            .expect("workbook relationship of the pivotCache");
        assert_eq!(format!("xl/{}", attr(rel, "Target").unwrap().trim_start_matches("/xl/")), cache_part, "workbook pivotCache {cache_id} target");
        let records = kind(&parts, &cache_part, "/pivotCacheRecords").first().map(|p| {
            assert_override(&parts, p, CT_RECORDS);
            text(&parts, p)
        });
        out.push(Kept { name: attr(root, "name").unwrap_or_default(), table, cache: text(&parts, &cache_part), cache_part, records });
    }
    out
}

/// Opens a workbook with one PivotTable on `source`, saves it, and checks the PivotTable came back
/// from its original parts.
fn assert_kept(source: &str) -> Vec<u8> {
    let (wb, report) = read_xlsx(&package(&[("Lost", 0)], &[source])).unwrap();
    assert_eq!(wb.sheet(0).unwrap().pivots.len(), 1, "fixture must read as a PivotTable: {:?}", report.warnings);
    let saved = write_xlsx(&wb).unwrap();
    let pivots = saved_pivots(&saved);
    assert_eq!(pivots.len(), 1, "the PivotTable on {source} was dropped on save");
    let p = &pivots[0];
    assert_eq!(p.name, "Lost");
    assert!(p.table.contains(r#"dataCaption="Amounts kept""#), "table part is not the original: {}", p.table);
    assert!(p.table.contains(r#"<location ref="D3:E5""#), "{}", p.table);
    assert!(p.cache.contains(source), "the stale source must stay as it was: {}", p.cache);
    assert!(p.cache.contains(r#"refreshedBy="Cache Keeper""#), "cache part is not the original: {}", p.cache);
    let records = p.records.as_ref().expect("the cache records part is kept");
    assert!(records.contains(r#"<n v="42.5"/>"#) && records.contains(r#"<n v="99.25"/>"#), "{records}");
    let (back, _) = read_xlsx(&saved).unwrap();
    let again = &back.sheet(0).unwrap().pivots;
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].name, "Lost");
    saved
}

#[test]
fn pivot_unresolved_source_fixture_with_a_live_source_round_trips() {
    // Guards the fixture and the checker: a resolvable source is rebuilt as before.
    let (wb, report) = read_xlsx(&package(&[("Live", 0)], &[LIVE])).unwrap();
    assert_eq!(wb.sheet(0).unwrap().pivots.len(), 1, "{:?}", report.warnings);
    let pivots = saved_pivots(&write_xlsx(&wb).unwrap());
    assert_eq!(pivots.len(), 1);
    assert_eq!(pivots[0].name, "Live");
    assert!(pivots[0].cache.contains(r#"sheet="Data""#), "{}", pivots[0].cache);
}

#[test]
fn pivot_unresolved_source_on_a_deleted_sheet_is_kept_on_save() {
    assert_kept(GONE_SHEET);
}

#[test]
fn pivot_unresolved_source_on_a_deleted_table_is_kept_on_save() {
    assert_kept(GONE_TABLE);
}

#[test]
fn pivot_unresolved_source_as_a_table_column_reference_is_kept_on_save() {
    assert_kept(TABLE_COLUMN);
}

#[test]
fn pivot_unresolved_source_survives_a_second_save() {
    let first = assert_kept(GONE_SHEET);
    let (wb, _) = read_xlsx(&first).unwrap();
    let pivots = saved_pivots(&write_xlsx(&wb).unwrap());
    assert_eq!(pivots.len(), 1);
    assert!(pivots[0].cache.contains(GONE_SHEET), "{}", pivots[0].cache);
    assert!(pivots[0].records.as_ref().is_some_and(|r| r.contains(r#"<n v="99.25"/>"#)));
}

#[test]
fn pivot_unresolved_source_shared_by_two_tables_is_written_once() {
    let (wb, report) = read_xlsx(&package(&[("One", 0), ("Two", 0)], &[GONE_SHEET])).unwrap();
    assert_eq!(wb.sheet(0).unwrap().pivots.len(), 2, "{:?}", report.warnings);
    let saved = write_xlsx(&wb).unwrap();
    let pivots = saved_pivots(&saved);
    let names: Vec<&str> = pivots.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["One", "Two"]);
    assert_eq!(pivots[0].cache_part, pivots[1].cache_part, "both tables use one cache");
    let parts = unzip(&saved);
    let count = |stem: &str| parts.keys().filter(|n| n.starts_with(stem) && n.ends_with(".xml")).count();
    assert_eq!(count("xl/pivotCache/pivotCacheDefinition"), 1);
    assert_eq!(count("xl/pivotCache/pivotCacheRecords"), 1);
    assert_eq!(count("xl/pivotTables/pivotTable"), 2);
}

#[test]
fn pivot_unresolved_source_is_kept_beside_a_rebuilt_pivot() {
    // Both orders: the kept cache's original id (1 or 2) may be the one the writer gives the
    // rebuilt cache, so ids must be made unique (checked in `saved_pivots`).
    for (tables, caches) in [([("Live", 0), ("Lost", 1)], [LIVE, GONE_SHEET]), ([("Lost", 0), ("Live", 1)], [GONE_SHEET, LIVE])] {
        let (wb, report) = read_xlsx(&package(&tables, &caches)).unwrap();
        assert_eq!(wb.sheet(0).unwrap().pivots.len(), 2, "{:?}", report.warnings);
        let pivots = saved_pivots(&write_xlsx(&wb).unwrap());
        assert_eq!(pivots.len(), 2, "tables {tables:?}");
        let lost = pivots.iter().find(|p| p.name == "Lost").expect("the PivotTable on the deleted sheet");
        let live = pivots.iter().find(|p| p.name == "Live").expect("the PivotTable on live data");
        assert_ne!(lost.cache_part, live.cache_part);
        assert!(lost.cache.contains(GONE_SHEET) && lost.cache.contains(r#"refreshedBy="Cache Keeper""#), "{}", lost.cache);
        assert!(lost.table.contains(r#"dataCaption="Amounts kept""#), "{}", lost.table);
        assert!(live.cache.contains(r#"sheet="Data""#), "{}", live.cache);
    }
}
