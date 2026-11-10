//! Axis display units (`c:dispUnits`) and axis crossing points (`c:crosses` / `c:crossesAt`) are
//! kept when a chart is opened and saved, in the schema slots of CT_CatAx / CT_ValAx
//! (ECMA-376 Part 1 §21.2.2).

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

/// A clustered column chart; `cat` / `val` go after `c:crossAx` of each axis (crossing), `val_tail`
/// after the value axis's `c:crossBetween` (display units).
fn chart_xml(cat: &str, val: &str, val_tail: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
         <c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>\
         <c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx>\
         <c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f></c:strRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f></c:numRef></c:val></c:ser>\
         <c:axId val=\"10\"/><c:axId val=\"20\"/></c:barChart>\
         <c:catAx><c:axId val=\"10\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/>\
         <c:crossAx val=\"20\"/>{cat}<c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx>\
         <c:valAx><c:axId val=\"20\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"l\"/><c:majorGridlines/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>\
         <c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"10\"/>{val}<c:crossBetween val=\"between\"/>{val_tail}</c:valAx>\
         </c:plotArea></c:chart></c:chartSpace>"
    )
}

const AUTO: &str = "<c:crosses val=\"autoZero\"/>";

fn read(xml: &str) -> Chart {
    read_chart(&parse(xml.as_bytes()).unwrap()).expect("column chart must be read")
}

fn save(ch: &Chart) -> String {
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() };
    write_chart(&wb, 0, ch, "Book1")
}

/// Open and save, then open and save again: the second save must equal the first (stable).
fn round_trip(xml: &str) -> String {
    let once = save(&read(xml));
    let twice = save(&read(&once));
    assert_eq!(once, twice, "second save differs from the first");
    once
}

/// The text of the first `<c:{tag}>...</c:{tag}>` element.
fn element<'a>(xml: &'a str, tag: &str) -> &'a str {
    let open = format!("<c:{tag}>");
    let close = format!("</c:{tag}>");
    let s = xml.find(&open).unwrap_or_else(|| panic!("no {open} in {xml}"));
    &xml[s..s + xml[s..].find(&close).unwrap_or_else(|| panic!("unclosed {open}")) + close.len()]
}

/// The crossing elements (`c:crosses` / `c:crossesAt`) of an axis element, in order.
fn crossings(ax: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ax;
    while let Some(i) = rest.find("<c:crosses") {
        let t = &rest[i..];
        let e = &t[..t.find("/>").unwrap() + 2];
        out.push(e.to_string());
        rest = &t[e.len()..];
    }
    out
}

fn pos(hay: &str, needle: &str) -> usize {
    hay.find(needle).unwrap_or_else(|| panic!("no {needle} in {hay}"))
}

#[test]
fn category_axis_crossing_at_a_value_is_kept() {
    // Excel: Format Axis (vertical) > Horizontal axis crosses > Axis value 2 is saved on the catAx.
    let out = round_trip(&chart_xml("<c:crossesAt val=\"2\"/>", AUTO, ""));
    let cat = element(&out, "catAx");
    assert_eq!(crossings(cat), ["<c:crossesAt val=\"2\"/>"], "catAx: {cat}");
    assert!(pos(cat, "<c:crossAx") < pos(cat, "<c:crossesAt"), "crossesAt follows crossAx: {cat}");
    assert_eq!(crossings(element(&out, "valAx")), [AUTO]);
}

#[test]
fn value_axis_crossing_at_a_category_and_fractional_values_are_kept() {
    let out = round_trip(&chart_xml(AUTO, "<c:crossesAt val=\"3\"/>", ""));
    let val = element(&out, "valAx");
    assert_eq!(crossings(val), ["<c:crossesAt val=\"3\"/>"], "valAx: {val}");
    assert!(pos(val, "<c:crossesAt") < pos(val, "<c:crossBetween"), "crossesAt before crossBetween: {val}");

    let out = round_trip(&chart_xml("<c:crossesAt val=\"-180.5\"/>", AUTO, ""));
    assert_eq!(crossings(element(&out, "catAx")), ["<c:crossesAt val=\"-180.5\"/>"]);
}

#[test]
fn crosses_min_and_max_are_kept() {
    let out = round_trip(&chart_xml("<c:crosses val=\"max\"/>", "<c:crosses val=\"min\"/>", ""));
    assert_eq!(crossings(element(&out, "catAx")), ["<c:crosses val=\"max\"/>"]);
    assert_eq!(crossings(element(&out, "valAx")), ["<c:crosses val=\"min\"/>"]);
}

#[test]
fn a_crossing_that_is_not_a_number_falls_back_to_automatic() {
    let out = round_trip(&chart_xml("<c:crossesAt val=\"NaN\"/>", "<c:crossesAt val=\"abc\"/>", ""));
    assert_eq!(crossings(element(&out, "catAx")), [AUTO]);
    assert_eq!(crossings(element(&out, "valAx")), [AUTO]);
}

#[test]
fn built_in_display_units_and_their_label_are_kept_after_the_units() {
    for unit in ["hundreds", "thousands", "tenThousands", "hundredThousands", "millions", "tenMillions", "hundredMillions", "billions", "trillions"] {
        for label in ["", "<c:dispUnitsLbl/>"] {
            let tail = format!("<c:majorUnit val=\"5\"/><c:dispUnits><c:builtInUnit val=\"{unit}\"/>{label}</c:dispUnits>");
            let out = round_trip(&chart_xml(AUTO, AUTO, &tail));
            let val = element(&out, "valAx");
            let du = element(val, "dispUnits");
            assert_eq!(du, format!("<c:dispUnits><c:builtInUnit val=\"{unit}\"/>{label}</c:dispUnits>"), "{unit} {label:?}");
            assert_eq!(val.matches("<c:dispUnits>").count(), 1, "{val}");
            assert!(pos(val, "<c:crossBetween") < pos(val, "<c:dispUnits"), "{val}");
            assert!(pos(val, "<c:majorUnit") < pos(val, "<c:dispUnits"), "majorUnit before dispUnits: {val}");
        }
    }
}

#[test]
fn custom_display_units_are_kept_and_bad_ones_dropped() {
    let out = round_trip(&chart_xml(AUTO, AUTO, "<c:dispUnits><c:custUnit val=\"250\"/><c:dispUnitsLbl/></c:dispUnits>"));
    assert_eq!(element(element(&out, "valAx"), "dispUnits"), "<c:dispUnits><c:custUnit val=\"250\"/><c:dispUnitsLbl/></c:dispUnits>");

    for bad in ["0", "-5", "NaN"] {
        let out = round_trip(&chart_xml(AUTO, AUTO, &format!("<c:dispUnits><c:custUnit val=\"{bad}\"/></c:dispUnits>")));
        assert!(!out.contains("<c:dispUnits>"), "custUnit {bad} must be dropped: {out}");
    }
}

#[test]
fn a_chart_without_axis_options_still_saves_automatic_crossing_and_no_display_units() {
    let out = round_trip(&chart_xml(AUTO, AUTO, ""));
    assert_eq!(crossings(element(&out, "catAx")), [AUTO]);
    assert_eq!(crossings(element(&out, "valAx")), [AUTO]);
    assert!(!out.contains("dispUnits"), "{out}");
}
