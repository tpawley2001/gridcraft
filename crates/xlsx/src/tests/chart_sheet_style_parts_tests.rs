//! Chart sheets opened from .xls and .xlsb save to .xlsx with the chart style and colour parts
//! that embedded charts get, and an .xlsb chart sheet's chart theme override (ECMA-376 Part 1
//! §14.2.8) survives. Kept parts are renamed on write, so the tests follow relationships from the
//! chart sheet part and check relationship types and content types, not part names.
use std::collections::HashMap;
use std::io::Read as _;

use gridcraft_model::Workbook;

use super::make_zip;
use super::xls_chart_tests::{bof, book, cat, cells, column_chart, eof};
use super::xlsb_chart_sheet_tests::{Part, chart_sheet_part, package_with_chart_part};
use crate::chartex::{CT_COLORS, CT_STYLE, REL_COLORS, REL_STYLE};
use crate::{read_xls, read_xlsb, write_xlsx};

const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_THEME_OVERRIDE: &str = "application/vnd.openxmlformats-officedocument.themeOverride+xml";
const OVERRIDE: &str = r#"<a:themeOverride xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:clrScheme name="Test"><a:dk1><a:srgbClr val="112233"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="000000"/></a:dk2><a:lt2><a:srgbClr val="EEEEEE"/></a:lt2><a:accent1><a:srgbClr val="AA0000"/></a:accent1><a:accent2><a:srgbClr val="00AA00"/></a:accent2><a:accent3><a:srgbClr val="0000AA"/></a:accent3><a:accent4><a:srgbClr val="AAAA00"/></a:accent4><a:accent5><a:srgbClr val="00AAAA"/></a:accent5><a:accent6><a:srgbClr val="AA00AA"/></a:accent6><a:hlink><a:srgbClr val="0000FF"/></a:hlink><a:folHlink><a:srgbClr val="800080"/></a:folHlink></a:clrScheme></a:themeOverride>"#;

fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut data = vec![];
        f.read_to_end(&mut data).unwrap();
        out.insert(f.name().to_string(), data);
    }
    out
}

/// `(type, target, id)` of each relationship in a `.rels` part.
fn rels(xml: &str) -> Vec<(String, String, String)> {
    let mut out = vec![];
    for chunk in xml.split("<Relationship ").skip(1) {
        let attr = |name: &str| {
            let key = format!("{name}=\"");
            chunk.find(&key).map(|i| {
                let rest = &chunk[i + key.len()..];
                rest[..rest.find('"').unwrap()].to_string()
            })
        };
        out.push((attr("Type").unwrap_or_default(), attr("Target").unwrap_or_default(), attr("Id").unwrap_or_default()));
    }
    out
}

fn rels_path(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    format!("{dir}/_rels/{file}.rels")
}

/// The part a relative relationship target names, seen from `from`.
fn resolve(from: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut segs: Vec<&str> = from.split('/').collect();
    segs.pop();
    for s in target.split('/') {
        match s {
            ".." => {
                segs.pop();
            }
            "." | "" => {}
            s => segs.push(s),
        }
    }
    segs.join("/")
}

/// The one relationship of `kind` (a full type URI) from `part`, resolved to a part name.
fn follow(parts: &HashMap<String, Vec<u8>>, part: &str, kind: &str) -> Vec<String> {
    let Some(xml) = parts.get(&rels_path(part)) else { return vec![] };
    rels(&String::from_utf8_lossy(xml)).into_iter().filter(|(t, _, _)| t == kind).map(|(_, target, _)| resolve(part, &target)).collect()
}

fn content_type(parts: &HashMap<String, Vec<u8>>, part: &str) -> Option<String> {
    let types = String::from_utf8_lossy(&parts["[Content_Types].xml"]).into_owned();
    let key = format!("PartName=\"/{part}\"");
    let i = types.find(&key)?;
    let start = types[..i].rfind('<')?;
    let tag = &types[start..start + types[start..].find('>')?];
    let ct = tag.split("ContentType=\"").nth(1)?;
    Some(ct[..ct.find('"')?].to_string())
}

/// The saved package and the name of the chart part of its one chart sheet.
fn saved_chart_part(wb: &Workbook) -> (HashMap<String, Vec<u8>>, String) {
    let parts = unzip(&write_xlsx(wb).unwrap());
    let sheets: Vec<_> =
        parts.keys().filter(|n| n.starts_with("xl/chartsheets/") && !n.contains("/_rels/") && n.ends_with(".xml")).cloned().collect();
    assert_eq!(sheets.len(), 1, "{:?}", parts.keys().collect::<Vec<_>>());
    let drawings = follow(&parts, &sheets[0], &format!("{R}/drawing"));
    assert_eq!(drawings.len(), 1, "chart sheet drawing rel");
    let charts = follow(&parts, &drawings[0], &format!("{R}/chart"));
    assert_eq!(charts.len(), 1, "drawing chart rel");
    assert!(parts.contains_key(&charts[0]), "chart part {} written", charts[0]);
    (parts, charts[0].clone())
}

/// The chart part has exactly one chartStyle and one chartColorStyle relationship, each to a
/// written part of the right content type.
fn assert_style_parts(parts: &HashMap<String, Vec<u8>>, chart: &str) {
    for (kind, ct) in [(REL_STYLE, CT_STYLE), (REL_COLORS, CT_COLORS)] {
        let targets = follow(parts, chart, kind);
        assert_eq!(targets.len(), 1, "{kind} rel from {chart}: {:?}", parts.get(&rels_path(chart)).map(|b| String::from_utf8_lossy(b).into_owned()));
        assert!(parts.contains_key(&targets[0]), "{} written", targets[0]);
        assert_eq!(content_type(parts, &targets[0]).as_deref(), Some(ct), "{}", targets[0]);
    }
}

fn read_xlsb_chart_sheet() -> Workbook {
    let (wb, report) = read_xlsb(&package_with_chart_part(&chart_sheet_part(Part::Chart))).unwrap();
    assert_eq!(wb.chart_sheets.len(), 1, "{:?}", report.warnings);
    wb
}

/// The .xlsb test package with a theme override on its chart part.
fn xlsb_with_theme_override() -> Vec<u8> {
    let mut parts = unzip(&package_with_chart_part(&chart_sheet_part(Part::Chart)));
    let types = String::from_utf8_lossy(&parts["[Content_Types].xml"])
        .replace("</Types>", &format!(r#"<Override PartName="/xl/theme/themeOverride1.xml" ContentType="{CT_THEME_OVERRIDE}"/></Types>"#));
    parts.insert("[Content_Types].xml".into(), types.into_bytes());
    let chart_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/themeOverride" Target="../theme/themeOverride1.xml"/></Relationships>"#
    );
    parts.insert("xl/charts/_rels/chart1.xml.rels".into(), chart_rels.into_bytes());
    parts.insert("xl/theme/themeOverride1.xml".into(), OVERRIDE.as_bytes().to_vec());
    let list: Vec<(&str, &[u8])> = parts.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
    make_zip(&list)
}

fn read_xls_chart_sheet() -> Workbook {
    let bytes = book(&[("Sheet1", 0, cat(&[&bof(0x0010), &cells(), &eof()])), ("Chart1", 2, column_chart())]);
    let (wb, report) = read_xls(&bytes).unwrap();
    assert_eq!(wb.chart_sheets.len(), 1, "{:?}", report.warnings);
    wb
}

#[test]
fn xlsb_chart_sheet_chart_gets_style_and_colour_parts() {
    let (parts, chart) = saved_chart_part(&read_xlsb_chart_sheet());
    assert_style_parts(&parts, &chart);
}

#[test]
fn xls_chart_sheet_chart_gets_style_and_colour_parts() {
    let (parts, chart) = saved_chart_part(&read_xls_chart_sheet());
    assert_style_parts(&parts, &chart);
    assert!(follow(&parts, &chart, &format!("{R}/themeOverride")).is_empty(), ".xls charts have no theme override");
}

#[test]
fn xlsb_chart_sheet_theme_override_survives_save_as_xlsx() {
    let (wb, report) = read_xlsb(&xlsb_with_theme_override()).unwrap();
    assert_eq!(wb.chart_sheets.len(), 1, "{:?}", report.warnings);
    assert!(wb.chart_sheets[0].chart.theme_override.is_some(), "the reader keeps the override on the chart");
    let (parts, chart) = saved_chart_part(&wb);
    let targets = follow(&parts, &chart, &format!("{R}/themeOverride"));
    assert_eq!(targets.len(), 1, "themeOverride rel from {chart}");
    assert_eq!(parts.get(&targets[0]).map(Vec::as_slice), Some(OVERRIDE.as_bytes()), "override bytes kept verbatim");
    assert_eq!(content_type(&parts, &targets[0]).as_deref(), Some(CT_THEME_OVERRIDE));
    assert_style_parts(&parts, &chart);
}

/// Guard: the chart sheet still saves as one chart sheet whose drawing names a written chart part
/// (passes before the change).
#[test]
fn chart_sheets_still_save_with_their_chart_part() {
    saved_chart_part(&read_xlsb_chart_sheet());
    saved_chart_part(&read_xls_chart_sheet());
}
