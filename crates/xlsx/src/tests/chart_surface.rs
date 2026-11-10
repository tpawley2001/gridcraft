//! The four surface chart types (3-D surface, wireframe 3-D surface, contour, wireframe contour)
//! open as their own chart kinds instead of being dropped, and save back the way Excel writes them:
//! `c:surface3DChart` / `c:surfaceChart` with `c:wireframe`, three axis ids, a series axis and the
//! default `c:view3D` angles of each type.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

fn ser(i: usize, col: &str) -> String {
    format!(
        "<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f></c:strRef></c:tx>\
         <c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f></c:strRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f></c:numRef></c:val></c:ser>"
    )
}

/// A chart part as Excel saves it: `view3D`, one surface group with three series, three axes.
fn chart_xml(group: &str, wireframe: Option<&str>, view: (i32, i32, i32)) -> String {
    let wf = wireframe.map(|v| format!("<c:wireframe val=\"{v}\"/>")).unwrap_or_default();
    let (rx, ry, persp) = view;
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>\
         <c:view3D><c:rotX val=\"{rx}\"/><c:rotY val=\"{ry}\"/><c:rAngAx val=\"0\"/><c:perspective val=\"{persp}\"/></c:view3D>\
         <c:plotArea><c:{group}>{wf}{}{}{}\
         <c:axId val=\"11\"/><c:axId val=\"12\"/><c:axId val=\"13\"/></c:{group}>\
         <c:catAx><c:axId val=\"11\"/><c:crossAx val=\"12\"/></c:catAx>\
         <c:valAx><c:axId val=\"12\"/><c:crossAx val=\"11\"/><c:crossBetween val=\"midCat\"/></c:valAx>\
         <c:serAx><c:axId val=\"13\"/><c:crossAx val=\"12\"/></c:serAx>\
         </c:plotArea></c:chart></c:chartSpace>",
        ser(0, "B"),
        ser(1, "C"),
        ser(2, "D"),
    )
}

fn read(xml: &str) -> Chart {
    read_chart(&parse(xml.as_bytes()).unwrap()).expect("a surface chart must be read, not dropped")
}

fn kind(ch: &Chart) -> String {
    format!("{:?}", ch.kind)
}

fn save(ch: &Chart) -> String {
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() };
    write_chart(&wb, 0, ch, "Book1")
}

/// The text between `<c:{name}>` and `</c:{name}>` (first occurrence).
fn inner<'a>(xml: &'a str, name: &str) -> &'a str {
    let open = format!("<c:{name}>");
    let close = format!("</c:{name}>");
    let s = xml.find(&open).unwrap_or_else(|| panic!("no <c:{name}> in {xml}")) + open.len();
    let e = s + xml[s..].find(&close).unwrap_or_else(|| panic!("no </c:{name}> in {xml}"));
    &xml[s..e]
}

/// The `val` of the first `<c:{name} val="..."/>` inside `xml`.
fn val<'a>(xml: &'a str, name: &str) -> &'a str {
    let tag = format!("<c:{name} val=\"");
    let s = xml.find(&tag).unwrap_or_else(|| panic!("no <c:{name} val> in {xml}")) + tag.len();
    &xml[s..s + xml[s..].find('"').unwrap()]
}

/// Checks the saved group, its children's order, the three axes and the view angles.
fn check_saved(xml: &str, group: &str, other: &str, wireframe: &str, view: (&str, &str, &str)) {
    assert!(xml.contains(&format!("<c:{group}>")), "{xml}");
    assert!(!xml.contains(&format!("<c:{other}>")), "{xml}");
    let g = inner(xml, group);
    assert!(g.starts_with("<c:wireframe"), "wireframe must be the group's first child: {g}");
    assert_eq!(val(g, "wireframe"), wireframe, "{g}");
    assert_eq!(g.matches("<c:ser>").count(), 3, "{g}");
    assert_eq!(g.matches("<c:axId ").count(), 3, "a surface group has three axis ids: {g}");
    let last_ser = g.rfind("</c:ser>").unwrap();
    let first_ax = g.find("<c:axId ").unwrap();
    assert!(last_ser < first_ax, "axis ids come after the series: {g}");
    for bad in ["<c:varyColors", "<c:marker", "<c:smooth"] {
        assert!(!g.contains(bad), "{bad} is not in the surface schema: {g}");
    }
    for ax in ["catAx", "valAx", "serAx"] {
        assert_eq!(xml.matches(&format!("<c:{ax}>")).count(), 1, "one <c:{ax}>: {xml}");
    }
    let v = inner(xml, "view3D");
    assert_eq!((val(v, "rotX"), val(v, "rotY"), val(v, "perspective")), view, "{v}");
    let (v3, pa) = (xml.find("<c:view3D>").unwrap(), xml.find("<c:plotArea>").unwrap());
    assert!(v3 < pa, "view3D comes before plotArea in c:chart: {xml}");
}

fn series_refs(ch: &Chart) -> Vec<(Option<&str>, Option<&str>, &str)> {
    ch.series.iter().map(|s| (s.name.as_deref(), s.categories.as_deref(), s.values.as_str())).collect()
}

const REFS: [(Option<&str>, Option<&str>, &str); 3] = [
    (Some("Sheet1!$B$1"), Some("Sheet1!$A$2:$A$5"), "Sheet1!$B$2:$B$5"),
    (Some("Sheet1!$C$1"), Some("Sheet1!$A$2:$A$5"), "Sheet1!$C$2:$C$5"),
    (Some("Sheet1!$D$1"), Some("Sheet1!$A$2:$A$5"), "Sheet1!$D$2:$D$5"),
];

#[test]
fn three_d_surface_reads_and_saves_as_excel_writes_it() {
    let ch = read(&chart_xml("surface3DChart", Some("0"), (15, 20, 30)));
    assert_eq!(kind(&ch), "Surface");
    assert_eq!(series_refs(&ch), REFS);
    let xml = save(&ch);
    check_saved(&xml, "surface3DChart", "surfaceChart", "0", ("15", "20", "30"));
    let back = read(&xml);
    assert_eq!(kind(&back), "Surface");
    assert_eq!(series_refs(&back), REFS);
}

#[test]
fn wireframe_three_d_surface_reads_and_saves() {
    let ch = read(&chart_xml("surface3DChart", Some("1"), (15, 20, 30)));
    assert_eq!(kind(&ch), "SurfaceWireframe");
    let xml = save(&ch);
    check_saved(&xml, "surface3DChart", "surfaceChart", "1", ("15", "20", "30"));
    assert_eq!(kind(&read(&xml)), "SurfaceWireframe");
}

#[test]
fn contour_reads_and_saves_with_the_top_view() {
    let ch = read(&chart_xml("surfaceChart", Some("0"), (90, 0, 0)));
    assert_eq!(kind(&ch), "Contour");
    assert_eq!(series_refs(&ch), REFS);
    let xml = save(&ch);
    check_saved(&xml, "surfaceChart", "surface3DChart", "0", ("90", "0", "0"));
    let back = read(&xml);
    assert_eq!(kind(&back), "Contour");
    assert_eq!(series_refs(&back), REFS);
}

#[test]
fn wireframe_contour_reads_and_saves() {
    let ch = read(&chart_xml("surfaceChart", Some("1"), (90, 0, 0)));
    assert_eq!(kind(&ch), "ContourWireframe");
    let xml = save(&ch);
    check_saved(&xml, "surfaceChart", "surface3DChart", "1", ("90", "0", "0"));
    assert_eq!(kind(&read(&xml)), "ContourWireframe");
}

#[test]
fn missing_wireframe_element_means_filled() {
    assert_eq!(kind(&read(&chart_xml("surface3DChart", None, (15, 20, 30)))), "Surface");
    assert_eq!(kind(&read(&chart_xml("surfaceChart", None, (90, 0, 0)))), "Contour");
}

#[test]
fn guard_other_kinds_still_read() {
    // Not claimed by this change: a line chart keeps reading as before (no `c:marker` = markers).
    let xml = "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
               <c:lineChart><c:grouping val=\"standard\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/>\
               <c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f></c:numRef></c:val></c:ser>\
               <c:axId val=\"1\"/><c:axId val=\"2\"/></c:lineChart></c:plotArea></c:chart></c:chartSpace>";
    assert_eq!(kind(&read(xml)), "LineMarkers");
}
