//! A chart's date axis (`c:dateAx`, ECMA-376 Part 1 §21.2.2) stays a date axis on open and save,
//! with `auto`, its base/major/minor time units (ST_TimeUnit, §21.2.3), its units and its scaling
//! bounds, written in CT_DateAx order and without the catAx-only `lblAlgn` / `noMultiLvlLbl`
//! (Excel keeps all of them when it re-saves such a file).

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

const SER: &str = "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx>\
     <c:cat><c:numRef><c:f>Sheet1!$A$2:$A$6</c:f></c:numRef></c:cat>\
     <c:val><c:numRef><c:f>Sheet1!$B$2:$B$6</c:f></c:numRef></c:val></c:ser>";

const VAL_AX: &str = "<c:valAx><c:axId val=\"12\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
     <c:axPos val=\"l\"/><c:majorGridlines/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"out\"/>\
     <c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"11\"/><c:crosses val=\"autoZero\"/>\
     <c:crossBetween val=\"between\"/></c:valAx>";

/// A line chart whose category axis is the given axis element (id 11, crossing 12).
fn line_chart(cat_axis: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>\
         <c:plotArea><c:layout/><c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{SER}\
         <c:marker val=\"1\"/><c:axId val=\"11\"/><c:axId val=\"12\"/></c:lineChart>{cat_axis}{VAL_AX}</c:plotArea>\
         <c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>"
    )
}

/// A `c:dateAx` (id 11) with the given scaling extras and the given children after `c:crosses`.
fn date_ax(scaling_extra: &str, tail: &str) -> String {
    format!(
        "<c:dateAx><c:axId val=\"11\"/><c:scaling><c:orientation val=\"minMax\"/>{scaling_extra}</c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"b\"/><c:numFmt formatCode=\"m/d/yyyy\" sourceLinked=\"1\"/><c:majorTickMark val=\"out\"/>\
         <c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"12\"/><c:crosses val=\"autoZero\"/>{tail}</c:dateAx>"
    )
}

fn round_trip(xml: &str) -> String {
    let ch: Chart = read_chart(&parse(xml.as_bytes()).unwrap()).expect("the chart must be read");
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() };
    write_chart(&wb, 0, &ch, "Book1")
}

/// The inner XML of the one written `<c:dateAx>`.
fn written_date_axis(out: &str) -> &str {
    assert_eq!(out.matches("<c:dateAx>").count(), 1, "exactly one c:dateAx expected in {out}");
    let start = out.find("<c:dateAx>").unwrap() + "<c:dateAx>".len();
    let end = out[start..].find("</c:dateAx>").expect("closed c:dateAx") + start;
    &out[start..end]
}

/// Asserts `tags` appear in `inner` in this order (each present exactly once).
fn assert_order(inner: &str, tags: &[&str]) {
    let mut last = 0usize;
    for t in tags {
        assert_eq!(inner.matches(t).count(), 1, "{t} once in {inner}");
        let at = inner.find(t).unwrap();
        assert!(at >= last, "{t} out of CT_DateAx order in {inner}");
        last = at;
    }
}

#[test]
fn a_date_axis_with_months_units_is_written_back_as_a_date_axis() {
    // XlsxWriter/Excel shape: base unit months, major unit 2 months (Excel keeps every value).
    let xml = line_chart(&date_ax(
        "",
        "<c:auto val=\"1\"/><c:lblOffset val=\"100\"/><c:baseTimeUnit val=\"months\"/><c:majorUnit val=\"2\"/><c:majorTimeUnit val=\"months\"/>",
    ));
    let out = round_trip(&xml);
    assert!(!out.contains("<c:catAx>"), "the date axis must not become a category axis: {out}");
    let ax = written_date_axis(&out);
    assert_order(
        ax,
        &[
            "<c:axId val=\"11\"/>",
            "<c:scaling>",
            "<c:crossAx val=\"12\"/>",
            "<c:crosses val=\"autoZero\"/>",
            "<c:auto val=\"1\"/>",
            "<c:lblOffset val=\"100\"/>",
            "<c:baseTimeUnit val=\"months\"/>",
            "<c:majorUnit val=\"2\"/>",
            "<c:majorTimeUnit val=\"months\"/>",
        ],
    );
    assert!(!ax.contains("minorUnit") && !ax.contains("minorTimeUnit"), "no minor units were in the file: {ax}");
}

#[test]
fn a_date_axis_never_carries_cat_axis_only_children() {
    // CT_DateAx has no lblAlgn / noMultiLvlLbl: Excel repairs a file that has them there.
    let xml = line_chart(&date_ax("", "<c:auto val=\"1\"/><c:lblOffset val=\"100\"/><c:baseTimeUnit val=\"days\"/>"));
    let ax = written_date_axis(&round_trip(&xml)).to_owned();
    assert!(!ax.contains("lblAlgn"), "{ax}");
    assert!(!ax.contains("noMultiLvlLbl"), "{ax}");
    assert!(ax.contains("<c:baseTimeUnit val=\"days\"/>"), "{ax}");
}

#[test]
fn major_and_minor_time_units_bounds_and_auto_off_are_kept() {
    // POI's 32readwriteLineDateAxisChart1 shape: fixed bounds (date serials), auto off,
    // base unit days, major 3 months, minor unit given only as a time unit.
    let xml = line_chart(&date_ax(
        "<c:max val=\"44926\"/><c:min val=\"44197\"/>",
        "<c:auto val=\"0\"/><c:lblOffset val=\"100\"/><c:baseTimeUnit val=\"days\"/><c:majorUnit val=\"3\"/>\
         <c:majorTimeUnit val=\"months\"/><c:minorTimeUnit val=\"months\"/>",
    ));
    let ax = written_date_axis(&round_trip(&xml)).to_owned();
    assert_order(
        &ax,
        &[
            "<c:orientation val=\"minMax\"/>",
            "<c:max val=\"44926\"/>",
            "<c:min val=\"44197\"/>",
            "</c:scaling>",
            "<c:auto val=\"0\"/>",
            "<c:baseTimeUnit val=\"days\"/>",
            "<c:majorUnit val=\"3\"/>",
            "<c:majorTimeUnit val=\"months\"/>",
            "<c:minorTimeUnit val=\"months\"/>",
        ],
    );
    assert!(!ax.contains("<c:minorUnit"), "{ax}");
}

#[test]
fn minor_unit_in_days_under_a_monthly_major_unit_is_kept() {
    // Excel's chart_axis16 shape: major 1 month, minor 14 days.
    let xml = line_chart(&date_ax(
        "",
        "<c:auto val=\"1\"/><c:lblOffset val=\"100\"/><c:baseTimeUnit val=\"days\"/><c:majorUnit val=\"1\"/>\
         <c:majorTimeUnit val=\"months\"/><c:minorUnit val=\"14\"/><c:minorTimeUnit val=\"days\"/>",
    ));
    let ax = written_date_axis(&round_trip(&xml)).to_owned();
    assert_order(
        &ax,
        &[
            "<c:baseTimeUnit val=\"days\"/>",
            "<c:majorUnit val=\"1\"/>",
            "<c:majorTimeUnit val=\"months\"/>",
            "<c:minorUnit val=\"14\"/>",
            "<c:minorTimeUnit val=\"days\"/>",
        ],
    );
}

#[test]
fn a_bare_date_axis_stays_a_date_axis() {
    // XlsxWriter writes only auto + lblOffset; Excel keeps it a date axis with no units added.
    let xml = line_chart(&date_ax("", "<c:auto val=\"1\"/><c:lblOffset val=\"100\"/>"));
    let ax = written_date_axis(&round_trip(&xml)).to_owned();
    assert!(ax.contains("<c:auto val=\"1\"/>"), "{ax}");
    assert!(!ax.contains("TimeUnit") && !ax.contains("majorUnit") && !ax.contains("minorUnit"), "{ax}");
}

#[test]
fn invalid_time_units_and_units_are_dropped() {
    let xml = line_chart(&date_ax(
        "",
        "<c:auto val=\"1\"/><c:lblOffset val=\"100\"/><c:baseTimeUnit val=\"weeks\"/><c:majorUnit val=\"-2\"/>\
         <c:majorTimeUnit val=\"months\"/><c:minorUnit val=\"NaN\"/>",
    ));
    let ax = written_date_axis(&round_trip(&xml)).to_owned();
    assert!(!ax.contains("baseTimeUnit"), "an unknown time unit is dropped: {ax}");
    assert!(!ax.contains("majorUnit val=\"-2\""), "a non-positive unit is dropped: {ax}");
    assert!(!ax.contains("<c:minorUnit"), "a non-number unit is dropped: {ax}");
    assert!(ax.contains("<c:majorTimeUnit val=\"months\"/>"), "{ax}");
}

#[test]
fn guard_a_category_axis_stays_a_category_axis() {
    let cat = "<c:catAx><c:axId val=\"11\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"b\"/><c:majorTickMark val=\"out\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>\
         <c:crossAx val=\"12\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/>\
         <c:noMultiLvlLbl val=\"0\"/></c:catAx>";
    let out = round_trip(&line_chart(cat));
    assert!(out.contains("<c:catAx>") && !out.contains("dateAx"), "{out}");
    assert!(!out.contains("TimeUnit"), "{out}");
}

#[test]
fn guard_a_scatter_chart_keeps_two_value_axes() {
    let xml = "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>\
         <c:plotArea><c:layout/><c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>\
         <c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:xVal><c:numRef><c:f>Sheet1!$A$2:$A$6</c:f></c:numRef></c:xVal>\
         <c:yVal><c:numRef><c:f>Sheet1!$B$2:$B$6</c:f></c:numRef></c:yVal></c:ser>\
         <c:axId val=\"11\"/><c:axId val=\"12\"/></c:scatterChart>\
         <c:valAx><c:axId val=\"11\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>\
         <c:crossAx val=\"12\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>\
         <c:valAx><c:axId val=\"12\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>\
         <c:crossAx val=\"11\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>\
         </c:plotArea><c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>";
    let out = round_trip(xml);
    assert!(!out.contains("dateAx") && !out.contains("TimeUnit"), "{out}");
    assert_eq!(out.matches("<c:valAx>").count(), 2, "{out}");
}
