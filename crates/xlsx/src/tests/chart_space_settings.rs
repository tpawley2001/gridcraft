//! Chart-space settings Excel keeps when it re-saves a chart part (ECMA-376 Part 1 §21.2.2.29):
//! `c:roundedCorners` (§21.2.2.156, absent or without `val` = rounded), `c:lang` (§21.2.2.82) and
//! `c:date1904` (§21.2.2.38, from the workbook's date system), written in CT_ChartSpace order.

use gridcraft_core::DateSystem;
use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

/// A one-series line chart part with `head` placed before `<c:chart>`.
fn chart_part(head: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">{head}<c:chart>\
         <c:plotArea><c:layout/><c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>\
         <c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:val><c:numRef><c:f>Sheet1!$B$2:$B$6</c:f></c:numRef></c:val></c:ser>\
         <c:marker val=\"1\"/><c:axId val=\"11\"/><c:axId val=\"12\"/></c:lineChart>\
         <c:catAx><c:axId val=\"11\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:crossAx val=\"12\"/></c:catAx>\
         <c:valAx><c:axId val=\"12\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:crossAx val=\"11\"/></c:valAx>\
         </c:plotArea><c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>"
    )
}

fn read(head: &str) -> Chart {
    read_chart(&parse(chart_part(head).as_bytes()).unwrap()).expect("the chart must be read")
}

fn write(ch: &Chart, date_system: DateSystem) -> String {
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], date_system, ..Workbook::default() };
    write_chart(&wb, 0, ch, "Book1")
}

/// The chart-space prefix of a written part: everything between `<c:chartSpace ...>` and `<c:chart>`.
fn head(out: &str) -> &str {
    let open = out.find("<c:chartSpace").expect("chartSpace");
    let start = out[open..].find('>').unwrap() + open + 1;
    let end = out.find("<c:chart>").expect("<c:chart>");
    &out[start..end]
}

fn rounded_written(out: &str) -> &'static str {
    let h = head(out);
    assert_eq!(h.matches("<c:roundedCorners").count(), 1, "exactly one c:roundedCorners in {h}");
    if h.contains("<c:roundedCorners val=\"1\"/>") {
        "1"
    } else if h.contains("<c:roundedCorners val=\"0\"/>") {
        "0"
    } else {
        panic!("roundedCorners must be written as val=\"0\" or val=\"1\": {h}")
    }
}

#[test]
fn rounded_corners_follow_the_file() {
    // (chart-space head in the file, roundedCorners Excel writes when it re-saves the file)
    let cases = [
        ("<c:roundedCorners val=\"1\"/>", "1"),
        ("<c:roundedCorners val=\"true\"/>", "1"),
        ("<c:roundedCorners/>", "1"),
        ("", "1"), // absent: Excel 365 re-saves these as val="1" (rounded)
        ("<c:roundedCorners val=\"0\"/>", "0"),
        ("<c:roundedCorners val=\"false\"/>", "0"),
    ];
    for (file_head, want) in cases {
        let out = write(&read(file_head), DateSystem::D1900);
        assert_eq!(rounded_written(&out), want, "file head {file_head:?} -> {}", head(&out));
    }
}

#[test]
fn rounded_corners_survive_a_second_save() {
    let once = write(&read("<c:roundedCorners val=\"1\"/>"), DateSystem::D1900);
    let twice = write(&read_chart(&parse(once.as_bytes()).unwrap()).unwrap(), DateSystem::D1900);
    assert_eq!(rounded_written(&twice), "1");
}

#[test]
fn chart_language_is_kept() {
    let out = write(&read("<c:lang val=\"de-DE\"/><c:roundedCorners val=\"0\"/>"), DateSystem::D1900);
    let h = head(&out);
    assert_eq!(h.matches("<c:lang val=\"de-DE\"/>").count(), 1, "c:lang kept: {h}");
    assert!(h.find("<c:lang").unwrap() < h.find("<c:roundedCorners").unwrap(), "lang before roundedCorners: {h}");
    let none = write(&read("<c:roundedCorners val=\"0\"/>"), DateSystem::D1900);
    assert!(!head(&none).contains("<c:lang"), "no c:lang invented: {}", head(&none));
}

#[test]
fn date1904_comes_from_the_workbook() {
    let ch = read("<c:roundedCorners val=\"0\"/>");
    let h1904 = write(&ch, DateSystem::D1904);
    let h = head(&h1904);
    assert_eq!(h.matches("<c:date1904 val=\"1\"/>").count(), 1, "1904 workbook: {h}");
    assert!(h.starts_with("<c:date1904"), "c:date1904 is the first chartSpace child: {h}");
    let h1900 = write(&ch, DateSystem::D1900);
    assert!(!head(&h1900).contains("<c:date1904 val=\"1\""), "1900 workbook must not claim 1904: {}", head(&h1900));
}

#[test]
fn chart_space_children_in_schema_order() {
    let out = write(&read("<c:date1904 val=\"0\"/><c:lang val=\"en-US\"/><c:roundedCorners val=\"1\"/><c:style val=\"2\"/>"), DateSystem::D1904);
    let h = head(&out);
    let at = |t: &str| h.find(t).unwrap_or_else(|| panic!("{t} missing in {h}"));
    assert!(at("<c:date1904") < at("<c:lang"), "{h}");
    assert!(at("<c:lang") < at("<c:roundedCorners"), "{h}");
    if let Some(style) = h.find("<c:style") {
        assert!(at("<c:roundedCorners") < style, "{h}");
    }
}
