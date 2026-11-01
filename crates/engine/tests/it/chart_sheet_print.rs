//! Chart sheets print as one page each when the whole workbook is printed, like Excel's
//! "Print Entire Workbook"; the active-sheet print and the kept chart sheet parts are unchanged.

use std::io::{Read, Write};

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

fn worksheet(rows: &[(u32, f64)]) -> String {
    let mut s = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}"><sheetData>"#);
    for (r, v) in rows {
        s.push_str(&format!(r#"<row r="{r}"><c r="A{r}"><v>{v}</v></c></row>"#));
    }
    s + "</sheetData></worksheet>"
}

fn chartsheet(orientation: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><chartsheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetPr/><sheetViews><sheetView workbookViewId="0" zoomToFit="1"/></sheetViews><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup orientation="{orientation}"/><drawing r:id="rId1"/></chartsheet>"#
    )
}

fn drawing() -> String {
    r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:absoluteAnchor><xdr:pos x="0" y="0"/><xdr:ext cx="8670925" cy="6291263"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Chart 1"/><xdr:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></xdr:cNvGraphicFramePr></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:absoluteAnchor></xdr:wsDr>"#
        .to_string()
}

fn chart(formula: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numRef><c:f>{formula}</c:f><c:numCache><c:ptCount val="3"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt><c:pt idx="2"><c:v>3</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:crossAx val="1"/></c:valAx></c:plotArea><c:legend><c:legendPos val="r"/></c:legend></c:chart></c:chartSpace>"#
    )
}

/// Tabs in order: Data (worksheet), Chart1 (chart sheet on Data, landscape), More (worksheet),
/// Last (hidden chart sheet on More). The active tab is Data. With `chart1_part` false, Chart1's
/// chart part is missing from the package.
fn package(chart1_part: bool) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><bookViews><workbookView activeTab="0"/></bookViews><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Chart1" sheetId="2" r:id="rId2"/><sheet name="More" sheetId="3" r:id="rId3"/><sheet name="Last" sheetId="4" state="hidden" r:id="rId4"/></sheets></workbook>"#
    );
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/xl/chartsheets/sheet1.xml" ContentType="{CT}.spreadsheetml.chartsheet+xml"/><Override PartName="/xl/chartsheets/sheet2.xml" ContentType="{CT}.spreadsheetml.chartsheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/><Override PartName="/xl/drawings/drawing2.xml" ContentType="{CT}.drawing+xml"/><Override PartName="/xl/charts/chart1.xml" ContentType="{CT}.drawingml.chart+xml"/><Override PartName="/xl/charts/chart2.xml" ContentType="{CT}.drawingml.chart+xml"/></Types>"#
    );
    let mut parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", types),
        ("_rels/.rels", rels(&[("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", workbook),
        (
            "xl/_rels/workbook.xml.rels",
            rels(&[
                ("rId1", "worksheet", "worksheets/sheet1.xml"),
                ("rId2", "chartsheet", "chartsheets/sheet1.xml"),
                ("rId3", "worksheet", "worksheets/sheet2.xml"),
                ("rId4", "chartsheet", "chartsheets/sheet2.xml"),
            ]),
        ),
        ("xl/worksheets/sheet1.xml", worksheet(&[(1, 1.0), (2, 2.0), (3, 3.0)])),
        ("xl/worksheets/sheet2.xml", worksheet(&[(1, 4.0)])),
        ("xl/chartsheets/sheet1.xml", chartsheet("landscape")),
        ("xl/chartsheets/sheet2.xml", chartsheet("portrait")),
        ("xl/chartsheets/_rels/sheet1.xml.rels", rels(&[("rId1", "drawing", "../drawings/drawing1.xml")])),
        ("xl/chartsheets/_rels/sheet2.xml.rels", rels(&[("rId1", "drawing", "../drawings/drawing2.xml")])),
        ("xl/drawings/drawing1.xml", drawing()),
        ("xl/drawings/drawing2.xml", drawing()),
        ("xl/drawings/_rels/drawing1.xml.rels", rels(&[("rId1", "chart", "../charts/chart1.xml")])),
        ("xl/drawings/_rels/drawing2.xml.rels", rels(&[("rId1", "chart", "../charts/chart2.xml")])),
        ("xl/charts/chart2.xml", chart("More!$A$1")),
    ];
    if chart1_part {
        parts.push(("xl/charts/chart1.xml", chart("Data!$A$1:$A$3")));
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "charts.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn preview_pages(s: &mut Session) -> Vec<Json> {
    let r = s.execute("file.printPreview", json!({"sheets": "all"})).unwrap();
    r["pageList"].as_array().unwrap().clone()
}

fn page_sheets(pages: &[Json]) -> Vec<String> {
    pages.iter().map(|p| p["sheet"].as_str().unwrap().to_string()).collect()
}

#[test]
fn visible_chart_sheets_print_as_one_page_each_in_tab_order() {
    let mut s = open(&package(true));
    let pages = preview_pages(&mut s);
    assert_eq!(page_sheets(&pages), ["Data", "Chart1", "More"]);
    assert_eq!(pages[1]["orientation"], "landscape", "{:?}", pages[1]);
}

#[test]
fn whole_workbook_pdf_counts_chart_sheet_pages() {
    let mut s = open(&package(true));
    let r = s.execute("file.exportPdf", json!({"sheets": "all"})).unwrap();
    assert_eq!(r["pages"], 3);
}

#[test]
fn active_sheet_pdf_is_unchanged() {
    let mut s = open(&package(true));
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    assert_eq!(r["pages"], 1);
}

#[test]
fn chart_sheet_without_its_chart_part_is_skipped() {
    let mut s = open(&package(false));
    assert_eq!(page_sheets(&preview_pages(&mut s)), ["Data", "More"]);
}

#[test]
fn chart_sheet_parts_are_still_written_back_unchanged() {
    let s = open(&package(true));
    let saved = gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "charts.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(saved)).unwrap();
    let mut kept = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        if f.name().starts_with("xl/chartsheets/") && f.name().ends_with(".xml") {
            let mut body = String::new();
            f.read_to_string(&mut body).unwrap();
            assert!(body.contains(r#"<pageSetup orientation="#), "{body}");
            kept += 1;
        }
    }
    assert_eq!(kept, 2);
}
