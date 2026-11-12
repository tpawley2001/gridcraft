//! A drawing object's "Locked" setting (`xdr:clientData fLocksWithSheet`, ECMA-376 Part 1
//! §20.5.2.3, default true) and its alternative-text title (`xdr:cNvPr title`, §20.5.2.8) survive
//! open + save, for shapes and for chart frames. Packages are built in code.

use std::io::Read;

use gridcraft_model::Workbook;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const NS: &str = r#"xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const SHEET_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;

/// A two-cell anchor at `row` holding `object`, with `client` as its `xdr:clientData` element.
fn anchor(row: u32, object: &str, client: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{object}{client}</xdr:twoCellAnchor>"#,
        row + 3
    )
}

/// A filled rectangle named `name`; `props` are extra `xdr:cNvPr` attributes (with a leading space).
fn shape(id: u32, name: &str, props: &str) -> String {
    format!(
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="{name}"{props}/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="571500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFF00"/></a:solidFill></xdr:spPr></xdr:sp>"#
    )
}

/// One shape per `(cNvPr attributes, clientData element)`, named `S0`, `S1`, ..., opened.
fn shapes(specs: &[(&str, &str)]) -> Workbook {
    let mut body = String::new();
    for (i, (props, client)) in specs.iter().enumerate() {
        body.push_str(&anchor(i as u32 * 4, &shape(i as u32 + 2, &format!("S{i}"), props), client));
    }
    let drawing = format!("<xdr:wsDr {NS}>{body}</xdr:wsDr>");
    let bytes = minimal(
        r#"<sheetData/><drawing r:id="rId1"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", SHEET_RELS), ("xl/drawings/drawing1.xml", &drawing)],
        "",
        "",
    );
    read_xlsx(&bytes).unwrap().0
}

/// The drawing part of the workbook as saved.
fn written_drawing(wb: &Workbook) -> String {
    let bytes = write_xlsx(wb).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut drawing = String::new();
    archive.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut drawing).unwrap();
    drawing
}

/// The saved anchor (up to its closing tag) of the object whose `xdr:cNvPr` has `name`.
fn anchor_of(xml: &str, name: &str) -> String {
    let at = xml.find(&format!("name=\"{name}\"")).unwrap_or_else(|| panic!("{name} was not written: {xml}"));
    let start = xml[..at]
        .rfind("<xdr:twoCellAnchor")
        .or_else(|| xml[..at].rfind("<xdr:oneCellAnchor"))
        .unwrap_or_else(|| panic!("{name} has no anchor: {xml}"));
    let rest = &xml[start..];
    let end = rest
        .find("</xdr:twoCellAnchor>")
        .or_else(|| rest.find("</xdr:oneCellAnchor>"))
        .unwrap_or_else(|| panic!("{name}'s anchor is not closed: {rest}"));
    rest[..end].to_string()
}

/// The `<xdr:clientData .../>` element of a saved anchor.
fn client_data(anchor: &str) -> String {
    let at = anchor.find("<xdr:clientData").unwrap_or_else(|| panic!("the anchor has no clientData: {anchor}"));
    let rest = &anchor[at..];
    rest[..=rest.find('>').unwrap_or_else(|| panic!("clientData is not closed: {rest}"))].to_string()
}

/// The `<xdr:cNvPr ...>` start tag of a saved anchor's object.
fn cnvpr(anchor: &str) -> String {
    let at = anchor.find("<xdr:cNvPr").unwrap_or_else(|| panic!("the anchor has no cNvPr: {anchor}"));
    let rest = &anchor[at..];
    rest[..=rest.find('>').unwrap_or_else(|| panic!("cNvPr is not closed: {rest}"))].to_string()
}

/// "Locked" off is kept, alone and next to "Print object" off; the defaults stay unwritten.
#[test]
fn a_shapes_locked_setting_is_kept() {
    let xml = written_drawing(&shapes(&[
        ("", r#"<xdr:clientData fLocksWithSheet="0"/>"#),
        ("", r#"<xdr:clientData fLocksWithSheet="0" fPrintsWithSheet="0"/>"#),
        ("", r#"<xdr:clientData fPrintsWithSheet="0"/>"#),
        ("", r#"<xdr:clientData fLocksWithSheet="1"/>"#),
        ("", "<xdr:clientData/>"),
    ]));
    let got: Vec<String> = (0..5).map(|i| client_data(&anchor_of(&xml, &format!("S{i}")))).collect();
    assert_eq!(got[0], r#"<xdr:clientData fLocksWithSheet="0"/>"#);
    assert_eq!(got[1], r#"<xdr:clientData fLocksWithSheet="0" fPrintsWithSheet="0"/>"#);
    assert_eq!(got[2], r#"<xdr:clientData fPrintsWithSheet="0"/>"#);
    assert_eq!(got[3], "<xdr:clientData/>");
    assert_eq!(got[4], "<xdr:clientData/>");
}

/// The setting is still there after a second open + save.
#[test]
fn a_shapes_locked_setting_survives_a_second_save() {
    let once = write_xlsx(&shapes(&[("", r#"<xdr:clientData fLocksWithSheet="0"/>"#), ("", "<xdr:clientData/>")])).unwrap();
    let xml = written_drawing(&read_xlsx(&once).unwrap().0);
    assert_eq!(client_data(&anchor_of(&xml, "S0")), r#"<xdr:clientData fLocksWithSheet="0"/>"#);
    assert_eq!(client_data(&anchor_of(&xml, "S1")), "<xdr:clientData/>");
}

/// A shape's alt-text title is kept next to its description; an empty title writes nothing.
#[test]
fn a_shapes_alt_text_title_is_kept() {
    let wb = shapes(&[
        (r#" descr="D" title="T &amp; &quot;q&quot;""#, "<xdr:clientData/>"),
        (r#" title="Only a title""#, "<xdr:clientData/>"),
        (r#" descr="Only a description" title="""#, "<xdr:clientData/>"),
        ("", "<xdr:clientData/>"),
    ]);
    assert_eq!(wb.sheet(0).unwrap().shapes[0].alt, "D");
    let xml = written_drawing(&wb);
    assert!(crate::xml::parse(xml.as_bytes()).is_ok(), "the saved drawing parses: {xml}");
    let got: Vec<String> = (0..4).map(|i| cnvpr(&anchor_of(&xml, &format!("S{i}")))).collect();
    assert!(got[0].contains(r#" descr="D""#), "{}", got[0]);
    assert!(got[0].contains(r#" title="T &amp; &quot;q&quot;""#), "{}", got[0]);
    assert!(got[1].contains(r#" title="Only a title""#), "{}", got[1]);
    assert!(!got[1].contains("descr="), "{}", got[1]);
    assert!(got[2].contains(r#" descr="Only a description""#), "{}", got[2]);
    assert!(!got[2].contains("title="), "an empty title is not written: {}", got[2]);
    assert!(!got[3].contains("title=") && !got[3].contains("descr="), "{}", got[3]);
    // Saved and opened again, the title is still written.
    let again = written_drawing(&read_xlsx(&write_xlsx(&wb).unwrap()).unwrap().0);
    assert!(cnvpr(&anchor_of(&again, "S0")).contains(r#" title="T &amp; &quot;q&quot;""#), "{again}");
}

/// A chart frame keeps its alt-text title and "Locked" off.
#[test]
fn a_charts_locked_setting_and_title_are_kept() {
    let frame = |id: u32, name: &str, props: &str, rid: &str| {
        format!(
            r#"<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{id}" name="{name}"{props}/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></xdr:graphicFrame>"#
        )
    };
    let drawing = format!(
        "<xdr:wsDr {NS}>{}{}</xdr:wsDr>",
        anchor(0, &frame(2, "Chart 1", r#" descr="Sales by region" title="Sales chart""#, "rId1"), r#"<xdr:clientData fLocksWithSheet="0"/>"#),
        anchor(20, &frame(3, "Chart 2", "", "rId2"), "<xdr:clientData/>"),
    );
    let chart = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f></c:numRef></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:crossAx val="1"/></c:valAx></c:plotArea></c:chart></c:chartSpace>"#;
    let drawing_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart2.xml"/></Relationships>"#;
    let bytes = minimal(
        r#"<sheetData/><drawing r:id="rId1"/>"#,
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", SHEET_RELS),
            ("xl/drawings/drawing1.xml", &drawing),
            ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels),
            ("xl/charts/chart1.xml", chart),
            ("xl/charts/chart2.xml", chart),
        ],
        "",
        "",
    );
    let wb = read_xlsx(&bytes).unwrap().0;
    assert_eq!(wb.sheet(0).unwrap().charts.len(), 2, "both charts are read");
    assert_eq!(wb.sheet(0).unwrap().charts[0].alt, "Sales by region");
    let xml = written_drawing(&wb);
    let first = anchor_of(&xml, "Chart 1");
    assert!(cnvpr(&first).contains(r#" title="Sales chart""#), "{first}");
    assert!(cnvpr(&first).contains(r#" descr="Sales by region""#), "{first}");
    assert_eq!(client_data(&first), r#"<xdr:clientData fLocksWithSheet="0"/>"#);
    let second = anchor_of(&xml, "Chart 2");
    assert!(!cnvpr(&second).contains("title="), "{second}");
    assert_eq!(client_data(&second), "<xdr:clientData/>");
}
