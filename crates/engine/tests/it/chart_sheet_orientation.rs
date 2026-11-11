//! A chart sheet whose page setup doesn't give an orientation prints landscape, like Excel
//! (ECMA-376 Part 1 §18.3.1.12 CT_Chartsheet, §18.3.1.64 CT_CsPageSetup). Measured in Excel for
//! Microsoft 365: chart sheets with no `<pageSetup>` print on a landscape page; an explicit
//! `orientation="portrait"` prints portrait. Worksheets keep portrait as their default.

use std::io::Write;

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument";

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = format!(r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}">"#);
    for (id, kind, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{NS_R}/{kind}" Target="{target}"/>"#));
    }
    s + "</Relationships>"
}

/// A chart sheet part with `page_setup` (a whole `<pageSetup .../>` element, or "") after its margins.
fn chartsheet(page_setup: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><chartsheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetPr/><sheetViews><sheetView workbookViewId="0"/></sheetViews><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>{page_setup}<drawing r:id="rId1"/></chartsheet>"#
    )
}

fn drawing() -> String {
    r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:absoluteAnchor><xdr:pos x="0" y="0"/><xdr:ext cx="8670925" cy="6291263"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Chart 1"/><xdr:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></xdr:cNvGraphicFramePr></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:absoluteAnchor></xdr:wsDr>"#
        .to_string()
}

fn chart() -> String {
    r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numRef><c:f>Data!$A$1:$A$3</c:f><c:numCache><c:ptCount val="3"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt><c:pt idx="2"><c:v>3</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:crossAx val="1"/></c:valAx></c:plotArea></c:chart></c:chartSpace>"#
        .to_string()
}

/// Tabs: Data (worksheet, no page setup), then one chart sheet per entry of `setups`, named
/// C1, C2, ... in order.
fn package(setups: &[&str]) -> Vec<u8> {
    let mut sheets = r#"<sheet name="Data" sheetId="1" r:id="rId1"/>"#.to_string();
    let mut wb_rels = vec![("rId1".to_string(), "worksheet", "worksheets/sheet1.xml".to_string())];
    let mut types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/>"#
    );
    let mut parts: Vec<(String, String)> = Vec::new();
    for (i, setup) in setups.iter().enumerate() {
        let n = i + 1;
        sheets.push_str(&format!(r#"<sheet name="C{n}" sheetId="{}" r:id="rId{}"/>"#, n + 1, n + 1));
        wb_rels.push((format!("rId{}", n + 1), "chartsheet", format!("chartsheets/sheet{n}.xml")));
        types.push_str(&format!(
            r#"<Override PartName="/xl/chartsheets/sheet{n}.xml" ContentType="{CT}.spreadsheetml.chartsheet+xml"/><Override PartName="/xl/drawings/drawing{n}.xml" ContentType="{CT}.drawing+xml"/><Override PartName="/xl/charts/chart{n}.xml" ContentType="{CT}.drawingml.chart+xml"/>"#
        ));
        parts.push((format!("xl/chartsheets/sheet{n}.xml"), chartsheet(setup)));
        parts.push((format!("xl/chartsheets/_rels/sheet{n}.xml.rels"), rels(&[("rId1", "drawing", &format!("../drawings/drawing{n}.xml"))])));
        parts.push((format!("xl/drawings/drawing{n}.xml"), drawing()));
        parts.push((format!("xl/drawings/_rels/drawing{n}.xml.rels"), rels(&[("rId1", "chart", &format!("../charts/chart{n}.xml"))])));
        parts.push((format!("xl/charts/chart{n}.xml"), chart()));
    }
    types.push_str("</Types>");
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><bookViews><workbookView activeTab="0"/></bookViews><sheets>{sheets}</sheets></workbook>"#
    );
    let wb_rels: Vec<(&str, &str, &str)> = wb_rels.iter().map(|(a, b, c)| (a.as_str(), *b, c.as_str())).collect();
    let worksheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row><row r="2"><c r="A2"><v>2</v></c></row><row r="3"><c r="A3"><v>3</v></c></row></sheetData></worksheet>"#
    );
    parts.splice(
        0..0,
        [
            ("[Content_Types].xml".to_string(), types),
            ("_rels/.rels".to_string(), rels(&[("rId1", "officeDocument", "xl/workbook.xml")])),
            ("xl/workbook.xml".to_string(), workbook),
            ("xl/_rels/workbook.xml.rels".to_string(), rels(&wb_rels)),
            ("xl/worksheets/sheet1.xml".to_string(), worksheet),
        ],
    );
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

/// The whole-workbook print preview's pages: (sheet, orientation, width, height, paper).
fn pages(setups: &[&str]) -> Vec<(String, String, f64, f64, String)> {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "charts.xlsx", "base64": gridcraft_engine::io::base64_encode(&package(setups))})).unwrap();
    let r = s.execute("file.printPreview", json!({"sheets": "all"})).unwrap();
    let list: &Vec<Json> = r["pageList"].as_array().unwrap();
    list.iter()
        .map(|p| {
            (
                p["sheet"].as_str().unwrap().to_string(),
                p["orientation"].as_str().unwrap().to_string(),
                p["width"].as_f64().unwrap(),
                p["height"].as_f64().unwrap(),
                p["paper"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

fn page_of<'a>(all: &'a [(String, String, f64, f64, String)], sheet: &str) -> &'a (String, String, f64, f64, String) {
    all.iter().find(|p| p.0 == sheet).unwrap_or_else(|| panic!("no page for {sheet}: {all:?}"))
}

#[test]
fn chart_sheet_without_page_setup_prints_landscape() {
    let all = pages(&[""]);
    let c1 = page_of(&all, "C1");
    assert_eq!(c1.1, "landscape", "{all:?}");
    // Letter turned sideways: 11 x 8.5 inches.
    assert!((c1.2 - 792.0).abs() < 0.5 && (c1.3 - 612.0).abs() < 0.5, "{all:?}");
}

#[test]
fn chart_sheet_page_setup_without_orientation_prints_landscape_on_its_paper() {
    let all = pages(&[r#"<pageSetup paperSize="9"/>"#]);
    let c1 = page_of(&all, "C1");
    assert_eq!(c1.1, "landscape", "{all:?}");
    assert_eq!(c1.4, "A4", "{all:?}");
    // A4 sideways: 841.89 x 595.28 pt.
    assert!(c1.2 > c1.3, "{all:?}");
}

#[test]
fn chart_sheet_orientation_portrait_stays_portrait() {
    let all = pages(&[r#"<pageSetup orientation="portrait"/>"#]);
    let c1 = page_of(&all, "C1");
    assert_eq!(c1.1, "portrait", "{all:?}");
    assert!((c1.2 - 612.0).abs() < 0.5 && (c1.3 - 792.0).abs() < 0.5, "{all:?}");
}

#[test]
fn chart_sheet_orientation_landscape_stays_landscape() {
    let all = pages(&[r#"<pageSetup orientation="landscape"/>"#]);
    assert_eq!(page_of(&all, "C1").1, "landscape", "{all:?}");
}

#[test]
fn mixed_chart_sheets_and_the_worksheet_keep_their_own_orientation() {
    let all = pages(&["", r#"<pageSetup orientation="portrait"/>"#, r#"<pageSetup orientation="landscape"/>"#]);
    // Worksheets keep portrait as their default.
    assert_eq!(page_of(&all, "Data").1, "portrait", "{all:?}");
    assert_eq!(page_of(&all, "C1").1, "landscape", "{all:?}");
    assert_eq!(page_of(&all, "C2").1, "portrait", "{all:?}");
    assert_eq!(page_of(&all, "C3").1, "landscape", "{all:?}");
}
