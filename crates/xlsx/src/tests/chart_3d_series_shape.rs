//! A 3-D column or bar series keeps its own bar shape (`c:ser/c:shape`, ECMA-376 Part 1
//! §21.2.2.177) on open and save, written after `c:val` as CT_BarSer orders it; a flat bar chart
//! never gets one.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

/// One `c:ser` as Excel writes it in a bar chart; `shape` is the whole `<c:shape .../>` or "".
fn ser(i: usize, col: &str, shape: &str) -> String {
    format!(
        "<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f></c:strRef></c:tx>\
         <c:invertIfNegative val=\"0\"/>\
         <c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f></c:strRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f></c:numRef></c:val>{shape}</c:ser>"
    )
}

fn shape(v: &str) -> String {
    format!("<c:shape val=\"{v}\"/>")
}

const AXES: &str = "<c:catAx><c:axId val=\"11\"/><c:crossAx val=\"12\"/></c:catAx>\
     <c:valAx><c:axId val=\"12\"/><c:crossAx val=\"11\"/><c:crossBetween val=\"between\"/></c:valAx>";

/// A clustered 3-D column chart with group shape `box` and the two given series shapes.
fn bar3d(s0: &str, s1: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>\
         <c:view3D><c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:rAngAx val=\"1\"/></c:view3D>\
         <c:plotArea><c:layout/><c:bar3DChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>\
         {}{}<c:gapWidth val=\"150\"/><c:shape val=\"box\"/>\
         <c:axId val=\"11\"/><c:axId val=\"12\"/><c:axId val=\"0\"/></c:bar3DChart>{AXES}</c:plotArea>\
         <c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>",
        ser(0, "B", s0),
        ser(1, "C", s1),
    )
}

fn flat_bar(s0: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>\
         <c:plotArea><c:layout/><c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>\
         {}<c:gapWidth val=\"150\"/><c:axId val=\"11\"/><c:axId val=\"12\"/></c:barChart>{AXES}</c:plotArea>\
         <c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>",
        ser(0, "B", s0),
    )
}

fn round_trip(xml: &str) -> String {
    let ch: Chart = read_chart(&parse(xml.as_bytes()).unwrap()).expect("the chart must be read");
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() };
    write_chart(&wb, 0, &ch, "Book1")
}

/// The inner text of every `<c:ser>` in order.
fn series(xml: &str) -> Vec<&str> {
    xml.split("<c:ser>").skip(1).map(|t| &t[..t.find("</c:ser>").expect("closed c:ser")]).collect()
}

/// The `val` of the series' own `c:shape`, checking it sits after `c:val`.
fn ser_shape(ser: &str) -> Option<&str> {
    let tag = "<c:shape val=\"";
    let at = ser.find(tag)?;
    let val_end = ser.find("</c:val>").expect("series has c:val");
    assert!(at > val_end, "c:shape must follow c:val in CT_BarSer: {ser}");
    let s = at + tag.len();
    Some(&ser[s..s + ser[s..].find('"').unwrap()])
}

/// The group's own `c:shape` (outside every `c:ser`).
fn group_shape(xml: &str) -> Option<String> {
    let g = &xml[xml.find("<c:bar3DChart>")?..xml.find("</c:bar3DChart>")?];
    let after = &g[g.rfind("</c:ser>").map_or(0, |i| i + "</c:ser>".len())..];
    let tag = "<c:shape val=\"";
    let s = after.find(tag)? + tag.len();
    Some(after[s..s + after[s..].find('"')?].to_string())
}

#[test]
fn each_3d_series_keeps_its_own_shape() {
    let out = round_trip(&bar3d(&shape("cylinder"), &shape("pyramid")));
    let s = series(&out);
    assert_eq!(s.len(), 2, "{out}");
    assert_eq!(ser_shape(s[0]), Some("cylinder"), "{out}");
    assert_eq!(ser_shape(s[1]), Some("pyramid"), "{out}");
    assert_eq!(group_shape(&out).as_deref(), Some("box"), "the group's shape stays: {out}");
}

#[test]
fn a_series_without_a_shape_gets_none() {
    let out = round_trip(&bar3d(&shape("coneToMax"), ""));
    let s = series(&out);
    assert_eq!(ser_shape(s[0]), Some("coneToMax"), "{out}");
    assert_eq!(ser_shape(s[1]), None, "{out}");
}

#[test]
fn an_unknown_series_shape_is_dropped() {
    let out = round_trip(&bar3d(&shape("hexagon"), ""));
    assert!(series(&out).iter().all(|s| ser_shape(s).is_none()), "{out}");
}

#[test]
fn a_flat_bar_series_never_writes_a_shape() {
    let out = round_trip(&flat_bar(&shape("cylinder")));
    assert!(out.contains("<c:barChart>"), "{out}");
    assert!(!out.contains("<c:shape"), "{out}");
}
