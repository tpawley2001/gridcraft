//! 3-D column, bar, line, area and pie charts open and save as 3-D charts: the `c:*3DChart` group
//! element, its `gapDepth` / `shape`, three axis ids (the third `0` unless the series stand in depth,
//! which adds a `c:serAx`), and the file's own `c:view3D` angles before `c:plotArea`.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

fn ser(i: usize, col: &str, extra: &str) -> String {
    format!(
        "<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f></c:strRef></c:tx>{extra}\
         <c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f></c:strRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f></c:numRef></c:val></c:ser>"
    )
}

/// A chart part as Excel saves it: `view3D` (+ floor and walls), one group, its axes.
/// `head` goes before the series, `tail` after them (gapWidth, gapDepth, shape, axIds).
fn chart_xml(view: &str, group: &str, head: &str, ser_extra: &str, tail: &str, axes: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>\
         <c:view3D>{view}</c:view3D>\
         <c:floor><c:thickness val=\"0\"/></c:floor><c:sideWall><c:thickness val=\"0\"/></c:sideWall>\
         <c:backWall><c:thickness val=\"0\"/></c:backWall>\
         <c:plotArea><c:layout/><c:{group}>{head}{}{}{tail}</c:{group}>{axes}</c:plotArea>\
         <c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend><c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>",
        ser(0, "B", ser_extra),
        ser(1, "C", ser_extra),
    )
}

const AXES_2: &str = "<c:catAx><c:axId val=\"11\"/><c:crossAx val=\"12\"/></c:catAx>\
     <c:valAx><c:axId val=\"12\"/><c:crossAx val=\"11\"/><c:crossBetween val=\"between\"/></c:valAx>";
const AXES_3: &str = "<c:catAx><c:axId val=\"11\"/><c:crossAx val=\"12\"/></c:catAx>\
     <c:valAx><c:axId val=\"12\"/><c:crossAx val=\"11\"/><c:crossBetween val=\"between\"/></c:valAx>\
     <c:serAx><c:axId val=\"13\"/><c:axPos val=\"b\"/><c:crossAx val=\"12\"/></c:serAx>";
const IDS_0: &str = "<c:axId val=\"11\"/><c:axId val=\"12\"/><c:axId val=\"0\"/>";
const IDS_3: &str = "<c:axId val=\"11\"/><c:axId val=\"12\"/><c:axId val=\"13\"/>";

fn read(xml: &str) -> Chart {
    read_chart(&parse(xml.as_bytes()).unwrap()).expect("a 3-D chart must be read, not dropped")
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
fn val<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let tag = format!("<c:{name} val=\"");
    let s = xml.find(&tag)? + tag.len();
    Some(&xml[s..s + xml[s..].find('"')?])
}

fn ax_ids(group: &str) -> Vec<&str> {
    group.match_indices("<c:axId val=\"").map(|(i, m)| &group[i + m.len()..i + m.len() + group[i + m.len()..].find('"').unwrap()]).collect()
}

/// The names of the direct `c:` children of `view3D`, in order.
fn view_children(xml: &str) -> Vec<(String, String)> {
    let v = inner(xml, "view3D");
    v.split("<c:")
        .skip(1)
        .map(|t| {
            let name = t.split([' ', '/', '>']).next().unwrap().to_string();
            (name.clone(), val(&format!("<c:{t}"), &name).unwrap_or("").to_string())
        })
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

/// `view3D`, floor, sideWall and backWall come in this order before `plotArea`.
fn check_chart_order(xml: &str) {
    let pos = |t: &str| xml.find(t).unwrap_or_else(|| panic!("no {t} in {xml}"));
    let (v, f, s, b, p) = (pos("<c:view3D>"), pos("<c:floor"), pos("<c:sideWall"), pos("<c:backWall"), pos("<c:plotArea>"));
    assert!(v < f && f < s && s < b && b < p, "view3D, floor, sideWall, backWall, plotArea order: {xml}");
}

/// Positions of `names` inside `g` (first occurrence) are strictly increasing; absent names are skipped.
fn check_order(g: &str, names: &[&str]) {
    let found: Vec<(usize, &str)> = names.iter().filter_map(|n| g.find(&format!("<c:{n}")).map(|i| (i, *n))).collect();
    for w in found.windows(2) {
        assert!(w[0].0 < w[1].0, "<c:{}> must come before <c:{}>: {g}", w[0].1, w[1].1);
    }
}

const VIEW_DEFAULT: &str = "<c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:rAngAx val=\"0\"/>";

#[test]
fn clustered_3d_column_stays_3d() {
    let xml = chart_xml(
        VIEW_DEFAULT,
        "bar3DChart",
        "<c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>",
        "<c:invertIfNegative val=\"0\"/>",
        &format!("<c:gapWidth val=\"150\"/><c:shape val=\"box\"/>{IDS_0}"),
        AXES_2,
    );
    let ch = read(&xml);
    assert_eq!(kind(&ch), "ColumnClustered");
    let out = save(&ch);
    assert!(!out.contains("<c:barChart>"), "saved flat: {out}");
    let g = inner(&out, "bar3DChart");
    assert_eq!(val(g, "barDir"), Some("col"));
    assert_eq!(val(g, "grouping"), Some("clustered"));
    assert_eq!(val(g, "shape"), Some("box"));
    assert_eq!(ax_ids(g).len(), 3, "{g}");
    assert_eq!(ax_ids(g)[2], "0", "clustered: third axis id is 0: {g}");
    assert!(!out.contains("<c:serAx>"), "no series axis for clustered: {out}");
    check_order(g, &["barDir", "grouping", "varyColors", "ser", "gapWidth", "gapDepth", "shape", "axId"]);
    assert_eq!(view_children(&out), pairs(&[("rotX", "15"), ("rotY", "20"), ("rAngAx", "0")]));
    check_chart_order(&out);
    let back = save(&read(&out));
    assert_eq!(back, out, "saving the saved chart again changes nothing");
}

#[test]
fn three_d_column_with_series_in_depth_keeps_its_series_axis() {
    let view = "<c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:depthPercent val=\"100\"/><c:rAngAx val=\"1\"/>";
    let xml = chart_xml(
        view,
        "bar3DChart",
        "<c:barDir val=\"col\"/><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>",
        "<c:invertIfNegative val=\"0\"/>",
        &format!("<c:gapWidth val=\"150\"/><c:shape val=\"box\"/>{IDS_3}"),
        AXES_3,
    );
    let out = save(&read(&xml));
    let g = inner(&out, "bar3DChart");
    assert_eq!(val(g, "grouping"), Some("standard"), "{g}");
    let ids = ax_ids(g);
    assert_eq!(ids.len(), 3, "{g}");
    assert_ne!(ids[2], "0", "series in depth: the third axis id is a real axis: {g}");
    assert_eq!(out.matches("<c:serAx>").count(), 1, "{out}");
    assert_eq!(val(inner(&out, "serAx"), "axId"), Some(ids[2]), "the serAx is the group's third axis: {out}");
    let (va, sa) = (out.find("<c:valAx>").unwrap(), out.find("<c:serAx>").unwrap());
    assert!(va < sa, "serAx comes after valAx: {out}");
    assert_eq!(view_children(&out), pairs(&[("rotX", "15"), ("rotY", "20"), ("depthPercent", "100"), ("rAngAx", "1")]));
}

#[test]
fn percent_stacked_3d_bar_keeps_gap_depth_shape_and_view() {
    let view = "<c:rotX val=\"15\"/><c:hPercent val=\"51\"/><c:rotY val=\"20\"/><c:depthPercent val=\"300\"/><c:rAngAx val=\"0\"/><c:perspective val=\"0\"/>";
    let xml = chart_xml(
        view,
        "bar3DChart",
        "<c:barDir val=\"bar\"/><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>",
        "<c:invertIfNegative val=\"0\"/>",
        &format!("<c:gapWidth val=\"75\"/><c:gapDepth val=\"200\"/><c:shape val=\"cylinder\"/>{IDS_0}"),
        AXES_2,
    );
    let ch = read(&xml);
    assert_eq!(kind(&ch), "BarStacked100");
    let out = save(&ch);
    let g = inner(&out, "bar3DChart");
    assert_eq!(val(g, "barDir"), Some("bar"));
    assert_eq!(val(g, "grouping"), Some("percentStacked"));
    assert_eq!(val(g, "gapDepth"), Some("200"));
    assert_eq!(val(g, "shape"), Some("cylinder"));
    check_order(g, &["barDir", "grouping", "varyColors", "ser", "gapWidth", "gapDepth", "shape", "axId"]);
    assert_eq!(
        view_children(&out),
        pairs(&[("rotX", "15"), ("hPercent", "51"), ("rotY", "20"), ("depthPercent", "300"), ("rAngAx", "0"), ("perspective", "0")])
    );
    check_chart_order(&out);
}

#[test]
fn three_d_line_stays_3d_with_its_series_axis() {
    let xml = chart_xml(VIEW_DEFAULT, "line3DChart", "<c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>", "", IDS_3, AXES_3);
    let ch = read(&xml);
    assert_eq!(kind(&ch), "Line", "3-D line has no markers");
    let out = save(&ch);
    assert!(!out.contains("<c:lineChart>"), "saved flat: {out}");
    let g = inner(&out, "line3DChart");
    assert_eq!(val(g, "grouping"), Some("standard"));
    assert_eq!(ax_ids(g).len(), 3, "{g}");
    assert_ne!(ax_ids(g)[2], "0", "{g}");
    assert_eq!(out.matches("<c:serAx>").count(), 1, "{out}");
    assert!(!g.contains("<c:marker val=\"1\""), "no group marker flag in a 3-D line: {g}");
    check_order(g, &["grouping", "varyColors", "ser", "dLbls", "dropLines", "gapDepth", "axId"]);
    check_chart_order(&out);
    assert_eq!(kind(&read(&out)), "Line");
}

#[test]
fn stacked_3d_area_stays_3d_without_a_series_axis() {
    let xml = chart_xml(VIEW_DEFAULT, "area3DChart", "<c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>", "", IDS_0, AXES_2);
    let ch = read(&xml);
    assert_eq!(kind(&ch), "AreaStacked");
    let out = save(&ch);
    assert!(!out.contains("<c:areaChart>"), "saved flat: {out}");
    let g = inner(&out, "area3DChart");
    assert_eq!(val(g, "grouping"), Some("stacked"));
    assert_eq!(ax_ids(g).len(), 3, "{g}");
    assert_eq!(ax_ids(g)[2], "0", "{g}");
    assert!(!out.contains("<c:serAx>"), "{out}");
    check_chart_order(&out);
}

#[test]
fn standard_3d_area_keeps_its_series_axis() {
    let xml = chart_xml(VIEW_DEFAULT, "area3DChart", "<c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>", "", IDS_3, AXES_3);
    let ch = read(&xml);
    assert_eq!(kind(&ch), "Area");
    let out = save(&ch);
    let g = inner(&out, "area3DChart");
    assert_eq!(val(g, "grouping"), Some("standard"));
    assert_ne!(ax_ids(g)[2], "0", "{g}");
    assert_eq!(out.matches("<c:serAx>").count(), 1, "{out}");
}

#[test]
fn three_d_pie_stays_3d_with_its_tilt() {
    let view = "<c:rotX val=\"75\"/><c:rotY val=\"0\"/><c:rAngAx val=\"0\"/>";
    let xml = chart_xml(view, "pie3DChart", "<c:varyColors val=\"1\"/>", "<c:explosion val=\"25\"/>", "", "");
    let ch = read(&xml);
    assert_eq!(kind(&ch), "Pie");
    let out = save(&ch);
    assert!(!out.contains("<c:pieChart>"), "saved flat: {out}");
    let g = inner(&out, "pie3DChart");
    assert!(g.starts_with("<c:varyColors"), "varyColors is the first child: {g}");
    assert!(ax_ids(g).is_empty(), "a pie has no axes: {g}");
    for ax in ["<c:catAx>", "<c:valAx>", "<c:serAx>"] {
        assert!(!out.contains(ax), "{ax} in a pie: {out}");
    }
    assert_eq!(view_children(&out), pairs(&[("rotX", "75"), ("rotY", "0"), ("rAngAx", "0")]));
    check_chart_order(&out);
}

/// Guard (passes before the change): a 2-D column chart still saves flat, with no view3D or walls.
#[test]
fn flat_column_stays_flat() {
    let xml = "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
         <c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>"
        .to_string()
        + &ser(0, "B", "")
        + "<c:gapWidth val=\"150\"/><c:axId val=\"11\"/><c:axId val=\"12\"/></c:barChart>"
        + AXES_2
        + "</c:plotArea></c:chart></c:chartSpace>";
    let out = save(&read(&xml));
    assert!(out.contains("<c:barChart>"), "{out}");
    for bad in ["bar3DChart", "<c:view3D", "<c:floor", "<c:sideWall", "<c:backWall", "<c:serAx"] {
        assert!(!out.contains(bad), "{bad} in a flat chart: {out}");
    }
}
