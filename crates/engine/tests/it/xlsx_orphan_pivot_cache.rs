//! A pivot cache listed in `workbook.xml` `<pivotCaches>` that no PivotTable uses (ECMA-376 Part 1,
//! 18.2.22 / 18.10.1.67) is kept on save, as Excel's own save keeps it. Shape measured on real files:
//! a cache on a data connection (`cacheSource type="external" connectionId="1"`), `invalid="1"`,
//! `saveData="0"`, no records part, and `xl/connections.xml` holding that connection.

use std::io::{Read, Write};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn xlsx() -> Vec<u8> {
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets><pivotCaches><pivotCache cacheId="43" r:id="rId4"/></pivotCaches></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId3" Type="{R}/connections" Target="connections.xml"/><Relationship Id="rId4" Type="{R}/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#
    );
    let connections = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><connections xmlns="{MAIN}"><connection id="1" name="Sales" type="1" refreshedVersion="3" background="1" saveData="1"><dbPr connection="DSN=Sales;" command="SELECT Region, Amount FROM Sales"/></connection></connections>"#
    );
    let cache = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotCacheDefinition xmlns="{MAIN}" xmlns:r="{R}" invalid="1" saveData="0" refreshedBy="Someone" refreshedDate="39065.5" backgroundQuery="1" createdVersion="3" refreshedVersion="3" minRefreshableVersion="3" recordCount="0"><cacheSource type="external" connectionId="1"/><cacheFields count="2"><cacheField name="Region" numFmtId="0"><sharedItems/></cacheField><cacheField name="Amount" numFmtId="0"><sharedItems containsNonDate="0" containsString="0" containsBlank="1"/></cacheField></cacheFields></pivotCacheDefinition>"#
    );
    let rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/connections.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml"/><Override PartName="/xl/pivotCache/pivotCacheDefinition1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml"/></Types>"#;
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", types.to_string()),
        ("_rels/.rels", rels),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", wb_rels),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/connections.xml", connections),
        ("xl/pivotCache/pivotCacheDefinition1.xml", cache),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, text) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(text.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn parts(bytes: &[u8]) -> Vec<(String, String)> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            (f.name().to_string(), s)
        })
        .collect()
}

fn saved() -> Vec<(String, String)> {
    let (wb, _) = gridcraft_xlsx::read_xlsx(&xlsx()).unwrap();
    parts(&gridcraft_xlsx::write_xlsx(&wb).unwrap())
}

fn part<'a>(all: &'a [(String, String)], name: &str) -> Option<&'a str> {
    all.iter().find(|(n, _)| n == name).map(|(_, s)| s.as_str())
}

#[test]
fn an_unused_connection_pivot_cache_is_kept_with_its_workbook_entry() {
    let all = saved();
    let caches: Vec<&(String, String)> = all.iter().filter(|(n, _)| n.starts_with("xl/pivotCache/pivotCacheDefinition")).collect();
    assert_eq!(caches.len(), 1, "one cache part: {:?}", all.iter().map(|p| &p.0).collect::<Vec<_>>());
    let (cache_name, cache) = caches[0];
    assert!(cache.contains("connectionId=\"1\""), "cache source kept: {cache}");
    assert!(cache.contains("name=\"Region\"") && cache.contains("name=\"Amount\""), "cache fields kept: {cache}");

    let wb = part(&all, "xl/workbook.xml").unwrap();
    let start = wb.find("<pivotCaches>").expect("workbook lists the cache");
    let entries = &wb[start..wb[start..].find("</pivotCaches>").map(|e| start + e).unwrap()];
    assert_eq!(entries.matches("<pivotCache ").count(), 1, "{entries}");
    assert!(entries.contains("cacheId=\"43\""), "cacheId kept: {entries}");

    // The entry's relationship points at the kept part.
    let rid = entries.split("r:id=\"").nth(1).unwrap().split('"').next().unwrap();
    let rels = part(&all, "xl/_rels/workbook.xml.rels").unwrap();
    let rel = rels.split("<Relationship ").find(|r| r.contains(&format!("Id=\"{rid}\""))).expect("relationship for the entry");
    let target = rel.split("Target=\"").nth(1).unwrap().split('"').next().unwrap();
    assert_eq!(format!("xl/{}", target.trim_start_matches('/').trim_start_matches("xl/")), *cache_name);
    assert!(rel.contains("pivotCacheDefinition"), "{rel}");

    let types = part(&all, "[Content_Types].xml").unwrap();
    assert!(types.contains(&format!("PartName=\"/{cache_name}\"")), "content type for {cache_name}: {types}");
    assert!(part(&all, "xl/connections.xml").is_some_and(|c| c.contains("id=\"1\"")), "connection kept");
}

#[test]
fn an_unused_pivot_cache_survives_a_second_save() {
    let (wb, _) = gridcraft_xlsx::read_xlsx(&xlsx()).unwrap();
    let once = gridcraft_xlsx::write_xlsx(&wb).unwrap();
    let (wb2, _) = gridcraft_xlsx::read_xlsx(&once).unwrap();
    let all = parts(&gridcraft_xlsx::write_xlsx(&wb2).unwrap());
    assert_eq!(all.iter().filter(|(n, _)| n.starts_with("xl/pivotCache/pivotCacheDefinition")).count(), 1);
    assert!(part(&all, "xl/workbook.xml").unwrap().contains("cacheId=\"43\""));
}
