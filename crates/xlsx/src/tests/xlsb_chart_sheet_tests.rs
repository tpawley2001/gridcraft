//! XLSB chart sheets open as chart sheets (their chart read from the DrawingML drawing that the
//! chart sheet's BrtDrawing record names) and save to .xlsx as chart sheets. Packages are built in
//! code from the public format schema.
use gridcraft_model::{ChartKind, Visibility, Workbook};

use super::make_zip;
use super::xlsb_tests::{record, wide, worksheet};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const UNREAD: &str = "XLSB chart sheets that could not be read were skipped.";
const SKIPPED: &str = "XLSB non-worksheet sheets were skipped.";
pub(super) const DRAWING: &str = r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><xdr:absoluteAnchor><xdr:pos x="0" y="0"/><xdr:ext cx="8666667" cy="6299200"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Chart 1"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:absoluteAnchor></xdr:wsDr>"#;
pub(super) const CHART: &str = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="bar"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numRef><c:f>Data!$B$1:$B$3</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;

/// How a test chart sheet part is built.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Part {
    /// Page margins, then a BrtDrawing naming the drawing.
    Chart,
    /// Page margins only.
    NoDrawing,
}

/// A sheet entry of the test workbook: name, relationship kind, hidden state and part shape.
struct Entry {
    name: &'static str,
    kind: &'static str,
    state: u32,
    part: Part,
}
fn data() -> Entry {
    Entry { name: "Data", kind: "worksheet", state: 0, part: Part::Chart }
}
fn chart(name: &'static str) -> Entry {
    Entry { name, kind: "chartsheet", state: 0, part: Part::Chart }
}

fn book(entries: &[Entry]) -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    for (i, e) in entries.iter().enumerate() {
        let mut data = e.state.to_le_bytes().to_vec();
        data.extend((i as u32 + 1).to_le_bytes());
        data.extend(wide(&format!("s{i}")));
        data.extend(wide(e.name));
        out.extend(record(156, &data));
    }
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}
pub(super) fn chart_sheet_part(part: Part) -> Vec<u8> {
    let mut out = record(476, &[0; 48]); // BrtMargins.
    if part == Part::Chart {
        out.extend(record(550, &wide("rId3")));
    }
    out
}

/// An XLSB package with `entries`; `custom` adds a custom document properties part.
fn package(entries: &[Entry], custom: bool) -> Vec<u8> {
    package_with(entries, custom, None)
}

/// A worksheet "Data" and a chart sheet "Chart1" whose part is `chart_part`.
pub(super) fn package_with_chart_part(chart_part: &[u8]) -> Vec<u8> {
    package_with(&[data(), chart("Chart1")], false, Some(chart_part))
}

/// [`package`], with `chart_part` (when given) as every chart sheet's part.
fn package_with(entries: &[Entry], custom: bool, chart_part: Option<&[u8]>) -> Vec<u8> {
    let mut roots = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="{R}/officeDocument" Target="xl/workbook.bin"/>"#
    );
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>"#,
    );
    if custom {
        roots.push_str(&format!(r#"<Relationship Id="cust" Type="{R}/custom-properties" Target="docProps/custom.xml"/>"#));
        types.push_str(
            r#"<Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>"#,
        );
    }
    roots.push_str("</Relationships>");
    let mut rels = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    for (i, e) in entries.iter().enumerate() {
        let (dir, ct, bytes) = match e.kind {
            "worksheet" => ("worksheets", "worksheet", worksheet(&[], &[])),
            "dialogsheet" => ("dialogsheets", "dialogsheet", vec![]),
            _ => ("chartsheets", "chartsheet", chart_part.map_or_else(|| chart_sheet_part(e.part), <[u8]>::to_vec)),
        };
        let path = format!("xl/{dir}/sheet{i}.bin");
        rels.push_str(&format!(r#"<Relationship Id="s{i}" Type="{R}/{}" Target="{dir}/sheet{i}.bin"/>"#, e.kind));
        types.push_str(&format!(r#"<Override PartName="/{path}" ContentType="application/vnd.ms-excel.{ct}"/>"#));
        if e.kind == "chartsheet" {
            let sheet_rels = format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId3" Type="{R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
            );
            files.push((format!("xl/{dir}/_rels/sheet{i}.bin.rels"), sheet_rels.into_bytes()));
        }
        files.push((path, bytes));
    }
    rels.push_str("</Relationships>");
    types.push_str("</Types>");
    let drawing_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/chart" Target="../charts/chart1.xml"/></Relationships>"#
    );
    let book = book(entries);
    let custom_xml = r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Project"><vt:lpwstr>North</vt:lpwstr></property></Properties>"#;
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", roots.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/drawings/drawing1.xml", DRAWING.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/charts/chart1.xml", CHART.as_bytes()),
    ];
    if custom {
        parts.push(("docProps/custom.xml", custom_xml.as_bytes()));
    }
    parts.extend(files.iter().map(|(n, b)| (n.as_str(), b.as_slice())));
    make_zip(&parts)
}
fn read(entries: &[Entry]) -> (Workbook, ReadReport) {
    read_xlsb(&package(entries, false)).unwrap()
}
fn count(report: &ReadReport, warning: &str) -> usize {
    report.warnings.iter().filter(|w| *w == warning).count()
}
fn assert_bar_chart(wb: &Workbook) {
    assert_eq!(wb.chart_sheets.len(), 1);
    let cs = &wb.chart_sheets[0];
    assert_eq!(cs.name, "Chart1");
    assert_eq!(cs.chart.kind, ChartKind::BarClustered);
    assert_eq!(cs.chart.series.len(), 1);
    assert_eq!(cs.chart.series[0].values, "Data!$B$1:$B$3");
}

#[test]
fn chart_sheet_after_a_worksheet_opens_as_a_chart_sheet() {
    let (wb, report) = read(&[data(), chart("Chart1")]);
    assert_eq!(wb.sheets.len(), 1);
    assert_eq!(wb.sheets[0].name, "Data");
    assert_bar_chart(&wb);
    assert_eq!(wb.chart_sheets[0].before, 1);
    assert_eq!(wb.chart_sheets[0].visibility, Visibility::Visible);
    assert_eq!(count(&report, UNREAD) + count(&report, SKIPPED), 0, "{:?}", report.warnings);
}

#[test]
fn chart_sheet_placed_first_keeps_its_tab_position() {
    let (wb, _) = read(&[chart("Chart1"), data()]);
    assert_bar_chart(&wb);
    assert_eq!(wb.chart_sheets[0].before, 0);
}

#[test]
fn hidden_chart_sheet_stays_hidden() {
    let (wb, _) = read(&[data(), Entry { state: 1, ..chart("Chart1") }]);
    assert_bar_chart(&wb);
    assert_eq!(wb.chart_sheets[0].visibility, Visibility::Hidden);
}

#[test]
fn chart_sheet_without_a_drawing_is_skipped_with_one_warning() {
    let (wb, report) = read(&[data(), Entry { part: Part::NoDrawing, ..chart("Chart1") }]);
    assert_eq!(wb.sheets.len(), 1);
    assert!(wb.chart_sheets.is_empty());
    assert_eq!(count(&report, UNREAD), 1, "{:?}", report.warnings);
}

#[test]
fn two_broken_chart_sheets_warn_once() {
    let broken = |name| Entry { part: Part::NoDrawing, ..chart(name) };
    let (wb, report) = read(&[data(), broken("Chart1"), broken("Chart2")]);
    assert!(wb.chart_sheets.is_empty());
    assert_eq!(count(&report, UNREAD), 1, "{:?}", report.warnings);
}

#[test]
fn dialog_sheet_is_still_skipped_as_a_non_worksheet() {
    let (wb, report) = read(&[data(), Entry { name: "Dialog1", kind: "dialogsheet", state: 0, part: Part::Chart }]);
    assert_eq!(wb.sheets.len(), 1);
    assert!(wb.chart_sheets.is_empty());
    assert_eq!(count(&report, SKIPPED), 1, "{:?}", report.warnings);
    assert_eq!(count(&report, UNREAD), 0, "{:?}", report.warnings);
}

/// The names of the parts in a zip package (local file headers).
fn part_names(zip: &[u8]) -> Vec<String> {
    let mut names = vec![];
    let mut at = 0;
    while let Some(i) = zip.get(at..).and_then(|rest| rest.windows(4).position(|w| w == b"PK\x03\x04")) {
        let start = at + i;
        let len = u16::from_le_bytes([zip[start + 26], zip[start + 27]]) as usize;
        names.push(String::from_utf8_lossy(&zip[start + 30..start + 30 + len]).into_owned());
        at = start + 4;
    }
    names
}

/// A chart sheet part of a saved .xlsx (the writer names kept parts itself, `kept1.xml`…).
fn is_chart_sheet_part(name: &str) -> bool {
    name.starts_with("xl/chartsheets/") && !name.contains("/_rels/") && name.ends_with(".xml")
}

#[test]
fn chart_sheet_saves_to_xlsx_as_a_chart_sheet() {
    let (wb, _) = read(&[data(), chart("Chart1")]);
    let xlsx = write_xlsx(&wb).unwrap();
    let names = part_names(&xlsx);
    assert!(names.iter().any(|n| is_chart_sheet_part(n)), "{names:?}");
    let (back, _) = read_xlsx(&xlsx).unwrap();
    assert_eq!(back.sheets.len(), 1);
    assert_bar_chart(&back);
}

#[test]
fn chart_sheet_and_custom_properties_both_save_to_xlsx() {
    let (wb, _) = read_xlsb(&package(&[data(), chart("Chart1")], true)).unwrap();
    let xlsx = write_xlsx(&wb).unwrap();
    let names = part_names(&xlsx);
    assert!(names.iter().any(|n| n == "docProps/custom.xml"), "{names:?}");
    assert!(names.iter().any(|n| is_chart_sheet_part(n)), "{names:?}");
    let (back, _) = read_xlsx(&xlsx).unwrap();
    assert_bar_chart(&back);
}
