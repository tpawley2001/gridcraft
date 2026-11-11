//! A category axis's "Interval between labels" (`c:tickLblSkip`) and "Interval between marks"
//! (`c:tickMarkSkip`) are kept when a chart is opened and saved, after `c:lblOffset` and before
//! `c:noMultiLvlLbl` in CT_CatAx (ECMA-376 Part 1 §21.2.2.25, §21.2.2.208, §21.2.2.209). Excel's own
//! re-save keeps both, including `val="1"`.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

/// A clustered column chart with `skip` placed after the category axis's `c:lblOffset`.
fn chart_xml(skip: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
         <c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>\
         <c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx>\
         <c:cat><c:numRef><c:f>Sheet1!$A$2:$A$6</c:f></c:numRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!$B$2:$B$6</c:f></c:numRef></c:val></c:ser>\
         <c:axId val=\"10\"/><c:axId val=\"20\"/></c:barChart>\
         <c:catAx><c:axId val=\"10\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/>\
         <c:crossAx val=\"20\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/>\
         <c:lblOffset val=\"100\"/>{skip}</c:catAx>\
         <c:valAx><c:axId val=\"20\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"l\"/><c:majorGridlines/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>\
         <c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"10\"/><c:crosses val=\"autoZero\"/>\
         <c:crossBetween val=\"between\"/></c:valAx>\
         </c:plotArea></c:chart></c:chartSpace>"
    )
}

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

fn pos(hay: &str, needle: &str) -> usize {
    hay.find(needle).unwrap_or_else(|| panic!("no {needle} in {hay}"))
}

#[test]
fn label_and_mark_intervals_are_read() {
    // XlsxWriter chart_axis40: tickLblSkip 3, tickMarkSkip 2 on the category axis.
    let ch = read(&chart_xml("<c:tickLblSkip val=\"3\"/><c:tickMarkSkip val=\"2\"/>"));
    assert_eq!(ch.scale.tick_label_skip, Some(3));
    assert_eq!(ch.scale.tick_mark_skip, Some(2));
    assert!(!ch.scale.is_auto(), "a set interval is not automatic scaling");

    let ch = read(&chart_xml("<c:tickLblSkip val=\"1\"/>"));
    assert_eq!((ch.scale.tick_label_skip, ch.scale.tick_mark_skip), (Some(1), None));
}

#[test]
fn intervals_are_written_in_schema_order_and_stay_stable() {
    let out = round_trip(&chart_xml("<c:tickLblSkip val=\"3\"/><c:tickMarkSkip val=\"2\"/>"));
    let cat = element(&out, "catAx");
    assert!(cat.contains("<c:tickLblSkip val=\"3\"/>"), "catAx: {cat}");
    assert!(cat.contains("<c:tickMarkSkip val=\"2\"/>"), "catAx: {cat}");
    assert!(pos(cat, "<c:lblOffset") < pos(cat, "<c:tickLblSkip"), "tickLblSkip follows lblOffset: {cat}");
    assert!(pos(cat, "<c:tickLblSkip") < pos(cat, "<c:tickMarkSkip"), "tickMarkSkip follows tickLblSkip: {cat}");
    if let Some(i) = cat.find("<c:noMultiLvlLbl") {
        assert!(pos(cat, "<c:tickMarkSkip") < i, "noMultiLvlLbl comes last: {cat}");
    }
    let val = element(&out, "valAx");
    assert!(!val.contains("Skip"), "the value axis gets no interval: {val}");
}

#[test]
fn an_interval_of_one_is_kept_like_excel() {
    let out = round_trip(&chart_xml("<c:tickLblSkip val=\"1\"/>"));
    let cat = element(&out, "catAx");
    assert!(cat.contains("<c:tickLblSkip val=\"1\"/>"), "catAx: {cat}");
    assert!(!cat.contains("tickMarkSkip"), "no mark interval was set: {cat}");
}

#[test]
fn no_interval_writes_none() {
    let ch = read(&chart_xml(""));
    assert_eq!((ch.scale.tick_label_skip, ch.scale.tick_mark_skip), (None, None));
    assert!(ch.scale.is_auto());
    let out = round_trip(&chart_xml(""));
    assert!(!out.contains("tickLblSkip") && !out.contains("tickMarkSkip"), "{out}");
}

#[test]
fn invalid_intervals_are_ignored() {
    for bad in ["0", "-2", "abc", "32000", "2.5", ""] {
        let xml = chart_xml(&format!("<c:tickLblSkip val=\"{bad}\"/><c:tickMarkSkip val=\"{bad}\"/>"));
        let ch = read(&xml);
        assert_eq!((ch.scale.tick_label_skip, ch.scale.tick_mark_skip), (None, None), "val={bad:?}");
        let out = round_trip(&xml);
        assert!(!out.contains("tickLblSkip") && !out.contains("tickMarkSkip"), "val={bad:?}: {out}");
    }
    let ch = read(&chart_xml("<c:tickLblSkip/><c:tickMarkSkip val=\"31999\"/>"));
    assert_eq!((ch.scale.tick_label_skip, ch.scale.tick_mark_skip), (None, Some(31999)), "missing val; Excel's maximum");
}
