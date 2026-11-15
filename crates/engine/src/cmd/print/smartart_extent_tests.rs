//! A SmartArt graphic (a drawing `graphicFrame` whose `graphicData` names the diagram URI) is not
//! drawn, but with no print area it extends the printed range like a chart or picture does: a
//! sheet holding only a graphic wider than the page prints two pages. A graphic hidden in the
//! file (`cNvPr hidden="1"`) or with "Print object" off (`clientData fPrintsWithSheet="0"`,
//! ECMA-376 Part 1 §20.5.2.3) takes no room. Letter portrait, margins 0.7 / 0.75 in, Calibri 11
//! defaults (64 px columns, 15 pt rows). The packages and diagram parts are built here.
use std::io::{Read, Write};

use serde_json::json;

use super::*;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument";

const DATA: &str = r#"<?xml version="1.0"?><dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dgm:ptLst><dgm:pt modelId="{10000000-0000-0000-0000-000000000001}" type="doc"><dgm:prSet/><dgm:spPr/></dgm:pt></dgm:ptLst><dgm:cxnLst/><dgm:bg/><dgm:whole/></dgm:dataModel>"#;
const LAYOUT: &str = r#"<?xml version="1.0"?><dgm:layoutDef xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" uniqueId="urn:example/layout/extent"><dgm:layoutNode name="root"/></dgm:layoutDef>"#;
const STYLE: &str = r#"<?xml version="1.0"?><dgm:styleDef xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" uniqueId="urn:example/style/extent"/>"#;
const COLORS: &str = r#"<?xml version="1.0"?><dgm:colorsDef xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" uniqueId="urn:example/colors/extent"/>"#;

/// How the graphic is marked in the file.
#[derive(Clone, Copy)]
enum Mark {
    Plain,
    NoPrint,
    Hidden,
}

/// One sheet "S" (A1 holds "x" when `a1`) with one SmartArt graphic anchored from `from` to
/// `to` (zero-based column, row).
fn package(a1: bool, from: (u32, u32), to: (u32, u32), mark: Mark) -> Vec<u8> {
    let data = if a1 { r#"<row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row>"# } else { "" };
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetFormatPr defaultRowHeight="15"/><sheetData>{data}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait"/><drawing r:id="rId1"/></worksheet>"#
    );
    let hidden = if matches!(mark, Mark::Hidden) { r#" hidden="1""# } else { "" };
    let client = if matches!(mark, Mark::NoPrint) { r#"<xdr:clientData fPrintsWithSheet="0"/>"# } else { "<xdr:clientData/>" };
    let corner = |(c, r): (u32, u32)| format!("<xdr:col>{c}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r}</xdr:row><xdr:rowOff>0</xdr:rowOff>");
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}"><xdr:twoCellAnchor><xdr:from>{}</xdr:from><xdr:to>{}</xdr:to><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Diagram 1"{hidden}/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId1" r:lo="rId2" r:qs="rId3" r:cs="rId4"/></a:graphicData></a:graphic></xdr:graphicFrame>{client}</xdr:twoCellAnchor></xdr:wsDr>"#,
        corner(from),
        corner(to)
    );
    let drawing_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG}"><Relationship Id="rId1" Type="{NS_R}/diagramData" Target="../diagrams/data1.xml"/><Relationship Id="rId2" Type="{NS_R}/diagramLayout" Target="../diagrams/layout1.xml"/><Relationship Id="rId3" Type="{NS_R}/diagramQuickStyle" Target="../diagrams/quickStyle1.xml"/><Relationship Id="rId4" Type="{NS_R}/diagramColors" Target="../diagrams/colors1.xml"/></Relationships>"#
    );
    let one_rel = |kind: &str, target: &str| {
        format!(
            r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG}"><Relationship Id="rId1" Type="{NS_R}/{kind}" Target="{target}"/></Relationships>"#
        )
    };
    let types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/><Override PartName="/xl/diagrams/data1.xml" ContentType="{CT}.drawingml.diagramData+xml"/><Override PartName="/xl/diagrams/layout1.xml" ContentType="{CT}.drawingml.diagramLayout+xml"/><Override PartName="/xl/diagrams/quickStyle1.xml" ContentType="{CT}.drawingml.diagramStyle+xml"/><Override PartName="/xl/diagrams/colors1.xml" ContentType="{CT}.drawingml.diagramColors+xml"/></Types>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", types),
        ("_rels/.rels", one_rel("officeDocument", "xl/workbook.xml")),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", one_rel("worksheet", "worksheets/sheet1.xml")),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/_rels/sheet1.xml.rels", one_rel("drawing", "../drawings/drawing1.xml")),
        ("xl/drawings/drawing1.xml", drawing),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels),
        ("xl/diagrams/data1.xml", DATA.to_string()),
        ("xl/diagrams/layout1.xml", LAYOUT.to_string()),
        ("xl/diagrams/quickStyle1.xml", STYLE.to_string()),
        ("xl/diagrams/colors1.xml", COLORS.to_string()),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in &parts {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "diagram.xlsx", "base64": crate::io::base64_encode(bytes)})).unwrap();
    s
}

/// The printed pages' ranges, in print order.
fn pages(s: &mut Session) -> Vec<String> {
    let pv = s.execute("file.printPreview", json!({})).unwrap();
    pv["pageList"].as_array().unwrap().iter().map(|p| p["range"].as_str().unwrap_or("").to_string()).collect()
}

/// What an empty sheet with the same page setup prints.
fn empty_sheet_pages() -> Vec<String> {
    let mut s = Session::new();
    s.new_workbook();
    pages(&mut s)
}

/// Case a: 15 default columns (960 px) are wider than one Letter-portrait page.
fn wide(mark: Mark) -> Vec<u8> {
    package(false, (0, 0), (14, 22), mark)
}

#[test]
fn a_wide_graphic_alone_prints_two_pages() {
    let p = pages(&mut open(&wide(Mark::Plain)));
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(p[0].starts_with("A1:"), "{p:?}");
    assert!(p[1].ends_with("O23"), "{p:?}");
}

#[test]
fn a_narrow_graphic_alone_prints_one_page() {
    let p = pages(&mut open(&package(false, (0, 0), (5, 10), Mark::Plain)));
    assert_eq!(p, ["A1:F11"], "{p:?}");
}

#[test]
fn a_graphic_that_does_not_print_takes_no_room() {
    let empty = empty_sheet_pages();
    assert_eq!(pages(&mut open(&wide(Mark::NoPrint))), empty);
}

#[test]
fn a_hidden_graphic_takes_no_room() {
    let empty = empty_sheet_pages();
    assert_eq!(pages(&mut open(&wide(Mark::Hidden))), empty);
}

#[test]
fn a_graphic_below_a_cell_extends_the_rows_within_one_page() {
    let p = pages(&mut open(&package(true, (0, 20), (2, 35), Mark::Plain)));
    assert_eq!(p, ["A1:C36"], "{p:?}");
}

#[test]
fn a_graphic_far_below_a_cell_extends_the_rows_onto_a_second_page() {
    let p = pages(&mut open(&package(true, (0, 60), (2, 75), Mark::Plain)));
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(p[0].starts_with("A1:"), "{p:?}");
    assert!(p[1].ends_with("C76"), "{p:?}");
}

#[test]
fn the_diagram_parts_still_round_trip_unchanged() {
    let mut s = open(&wide(Mark::Plain));
    let r = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap();
    let bytes = crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut body = String::new();
        let name = f.name().to_string();
        if f.read_to_string(&mut body).is_ok() {
            parts.push((name, body));
        }
    }
    for want in [DATA, LAYOUT, STYLE, COLORS] {
        assert!(parts.iter().any(|(_, b)| b == want), "a diagram part changed: {want}");
    }
    let frames: usize = parts
        .iter()
        .filter(|(n, _)| n.starts_with("xl/drawings/") && n.ends_with(".xml"))
        .map(|(_, b)| b.matches("uri=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"").count())
        .sum();
    assert_eq!(frames, 1, "the graphic is written once");
}
