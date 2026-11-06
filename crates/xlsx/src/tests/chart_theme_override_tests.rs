//! A chart's Theme Override part (ECMA-376 Part 1, relationship type
//! `.../relationships/themeOverride` from a chart part, root `<a:themeOverride>`) survives
//! open + save byte for byte, as Excel keeps it.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use crate::{chart::read_chart, read_xlsx, write_xlsx, xml::parse};

const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/themeOverride";
const CT: &str = "application/vnd.openxmlformats-officedocument.themeOverride+xml";

fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut m = BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        m.insert(f.name().to_string(), b);
    }
    m
}

fn rezip(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in parts {
        out.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn text(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(parts.get(name).unwrap_or_else(|| panic!("missing part {name}")).clone()).unwrap()
}

fn chart(value: u32) -> gridcraft_model::Chart {
    let xml = format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
        <c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/>\
        <c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{value}</c:v></c:pt></c:numLit></c:val>\
        </c:ser><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart><c:catAx/><c:valAx/></c:plotArea></c:chart></c:chartSpace>"
    );
    read_chart(&parse(xml.as_bytes()).unwrap()).unwrap()
}

/// A package with `n` charts on the first sheet, none with an override.
fn base(n: u32) -> BTreeMap<String, Vec<u8>> {
    let mut wb = gridcraft_model::Workbook::new();
    for i in 0..n {
        let mut c = chart(10 + i);
        c.id = i + 1;
        wb.sheet_mut(0).unwrap().charts.push(c);
    }
    unzip(&write_xlsx(&wb).unwrap())
}

fn override_xml(scheme: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<a:themeOverride xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
        <a:clrScheme name=\"{scheme}\"><a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1><a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>\
        <a:dk2><a:srgbClr val=\"1F497D\"/></a:dk2><a:lt2><a:srgbClr val=\"EEECE1\"/></a:lt2><a:accent1><a:srgbClr val=\"4F81BD\"/></a:accent1>\
        <a:accent2><a:srgbClr val=\"C0504D\"/></a:accent2><a:accent3><a:srgbClr val=\"9BBB59\"/></a:accent3><a:accent4><a:srgbClr val=\"8064A2\"/></a:accent4>\
        <a:accent5><a:srgbClr val=\"4BACC6\"/></a:accent5><a:accent6><a:srgbClr val=\"F79646\"/></a:accent6><a:hlink><a:srgbClr val=\"0000FF\"/></a:hlink>\
        <a:folHlink><a:srgbClr val=\"800080\"/></a:folHlink></a:clrScheme></a:themeOverride>"
    )
    .into_bytes()
}

/// Gives chart part `chart` (`xl/charts/chartN.xml`) a themeOverride relationship to `part`
/// (`xl/theme/...`) holding `bytes`; `declare` adds the content-type override.
fn add_override(parts: &mut BTreeMap<String, Vec<u8>>, chart: u32, part: &str, bytes: Option<Vec<u8>>, declare: bool) {
    let rels_name = format!("xl/charts/_rels/chart{chart}.xml.rels");
    let target = format!("../{}", part.trim_start_matches("xl/"));
    let rel = format!("<Relationship Id=\"rIdThemeOverride{chart}\" Type=\"{REL_TYPE}\" Target=\"{target}\"/>");
    let rels = match parts.get(&rels_name) {
        Some(b) => String::from_utf8(b.clone()).unwrap().replace("</Relationships>", &format!("{rel}</Relationships>")),
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rel}</Relationships>"
        ),
    };
    parts.insert(rels_name, rels.into_bytes());
    if let Some(b) = bytes {
        parts.insert(part.to_string(), b);
    }
    if declare {
        let ct = text(parts, "[Content_Types].xml").replace("</Types>", &format!("<Override PartName=\"/{part}\" ContentType=\"{CT}\"/></Types>"));
        parts.insert("[Content_Types].xml".into(), ct.into_bytes());
    }
}

/// Every themeOverride relationship in the package: (chart rels part, resolved target part name).
fn override_rels(parts: &BTreeMap<String, Vec<u8>>) -> Vec<(String, String)> {
    let mut out = vec![];
    for (name, bytes) in parts {
        if !(name.starts_with("xl/charts/_rels/") && name.ends_with(".rels")) {
            continue;
        }
        let root = parse(bytes).unwrap();
        for rel in root.children.iter().filter(|c| c.attr("Type") == Some(REL_TYPE)) {
            let target = rel.attr("Target").unwrap();
            let resolved = match target.strip_prefix('/') {
                Some(abs) => abs.to_string(),
                None => format!("xl/{}", target.trim_start_matches("../")),
            };
            out.push((name.clone(), resolved));
        }
    }
    out
}

fn theme_override_parts(parts: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    parts.iter().filter(|(n, b)| n.ends_with(".xml") && String::from_utf8_lossy(b).contains("<a:themeOverride")).map(|(n, _)| n.clone()).collect()
}

fn declared(parts: &BTreeMap<String, Vec<u8>>, part: &str) -> bool {
    text(parts, "[Content_Types].xml").contains(&format!("<Override PartName=\"/{part}\" ContentType=\"{CT}\"/>"))
}

fn round_trip(parts: &BTreeMap<String, Vec<u8>>) -> (BTreeMap<String, Vec<u8>>, crate::ReadReport) {
    let (wb, report) = read_xlsx(&rezip(parts)).unwrap();
    (unzip(&write_xlsx(&wb).unwrap()), report)
}

#[test]
fn a_chart_theme_override_survives_open_and_two_saves() {
    let original = override_xml("Custom 3");
    let mut parts = base(1);
    add_override(&mut parts, 1, "xl/theme/themeOverride1.xml", Some(original.clone()), true);
    let (once, _) = round_trip(&parts);
    let (twice, _) = round_trip(&once);
    for saved in [&once, &twice] {
        let rels = override_rels(saved);
        assert_eq!(rels.len(), 1, "one themeOverride relationship: {rels:?}");
        let (from, part) = &rels[0];
        assert_eq!(from, "xl/charts/_rels/chart1.xml.rels");
        assert_eq!(saved.get(part.as_str()), Some(&original), "override bytes unchanged in {part}");
        assert!(declared(saved, part), "content type declared for {part}");
        assert_eq!(theme_override_parts(saved), vec![part.clone()], "no duplicate override parts");
    }
}

#[test]
fn two_charts_keep_their_own_overrides() {
    let first = override_xml("First scheme");
    let second = override_xml("Second scheme");
    let mut parts = base(2);
    add_override(&mut parts, 1, "xl/theme/themeOverride1.xml", Some(first.clone()), true);
    add_override(&mut parts, 2, "xl/theme/themeOverride2.xml", Some(second.clone()), true);
    let (saved, _) = round_trip(&parts);
    let rels = override_rels(&saved);
    assert_eq!(rels.len(), 2, "{rels:?}");
    assert_ne!(rels[0].1, rels[1].1, "each chart has its own part");
    let mut payloads: Vec<&Vec<u8>> = rels.iter().map(|(_, p)| saved.get(p.as_str()).unwrap()).collect();
    payloads.sort();
    let mut expected = vec![&first, &second];
    expected.sort();
    assert_eq!(payloads, expected);
    for (_, p) in &rels {
        assert!(declared(&saved, p), "content type declared for {p}");
    }
    assert_eq!(theme_override_parts(&saved).len(), 2);
}

#[test]
fn a_chart_without_an_override_writes_none() {
    let (saved, _) = round_trip(&base(1));
    assert!(override_rels(&saved).is_empty());
    assert!(theme_override_parts(&saved).is_empty());
    assert!(!text(&saved, "[Content_Types].xml").contains(CT));
}

#[test]
fn a_damaged_override_is_dropped_with_a_warning() {
    for bad in [b"not xml at all <<<".to_vec(), b"<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"/>".to_vec()] {
        let mut parts = base(1);
        add_override(&mut parts, 1, "xl/theme/themeOverride1.xml", Some(bad.clone()), true);
        let (saved, report) = round_trip(&parts);
        assert!(override_rels(&saved).is_empty(), "dropped: {}", String::from_utf8_lossy(&bad));
        assert!(!saved.values().any(|b| *b == bad), "bad bytes not written back");
        assert_eq!(report.warnings.iter().filter(|w| w.to_lowercase().contains("theme override")).count(), 1, "{:?}", report.warnings);
    }
}

#[test]
fn a_missing_override_target_is_ignored() {
    let mut parts = base(1);
    add_override(&mut parts, 1, "xl/theme/themeOverride9.xml", None, false);
    let (saved, _) = round_trip(&parts);
    assert!(override_rels(&saved).is_empty());
    assert!(saved.contains_key("xl/charts/chart1.xml"), "the chart itself is kept");
}
