//! Pie of pie and bar of pie (`c:ofPieChart`) open as their own chart kinds instead of a plain pie,
//! and save back as `c:ofPieChart` with the second plot's settings (type, gap width, split,
//! second plot size, series lines) in the schema's child order.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

const SER: &str = "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx>\
     <c:cat><c:strRef><c:f>Sheet1!$A$2:$A$7</c:f></c:strRef></c:cat>\
     <c:val><c:numRef><c:f>Sheet1!$B$2:$B$7</c:f></c:numRef></c:val></c:ser>";

/// A chart part as Excel saves it: one `group` in the plot area holding one series, no axes.
fn chart_xml(group: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><c:chart>\
         <c:plotArea>{group}</c:plotArea><c:legend><c:legendPos val=\"b\"/></c:legend></c:chart></c:chartSpace>"
    )
}

/// An `ofPieChart` group the way Excel writes a new one (auto split, series lines on).
fn of_pie(kind: &str, settings: &str) -> String {
    chart_xml(&format!("<c:ofPieChart><c:ofPieType val=\"{kind}\"/><c:varyColors val=\"1\"/>{SER}{settings}</c:ofPieChart>"))
}

const EXCEL_DEFAULTS: &str = "<c:gapWidth val=\"100\"/><c:secondPieSize val=\"75\"/>\
     <c:serLines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"595959\"/></a:solidFill></a:ln></c:spPr></c:serLines>";

fn read(xml: &str) -> Chart {
    read_chart(&parse(xml.as_bytes()).unwrap()).expect("the chart must be read, not dropped")
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

/// Position of the first `<c:{name}` (followed by `>`, ` ` or `/`) in `g`.
fn pos(g: &str, name: &str) -> usize {
    [">", " ", "/"].iter().find_map(|end| g.find(&format!("<c:{name}{end}"))).unwrap_or_else(|| panic!("no <c:{name}> in {g}"))
}

/// The saved `ofPieChart` group: right kind, one series, no axes, and the children present
/// in CT_OfPieChart order.
fn saved_group(xml: &str, of_type: &str, order: &[&str]) -> String {
    assert!(!xml.contains("<c:pieChart>"), "saved as a plain pie: {xml}");
    let g = inner(xml, "ofPieChart").to_string();
    assert!(g.starts_with("<c:ofPieType"), "ofPieType must be the group's first child: {g}");
    assert_eq!(val(&g, "ofPieType"), of_type, "{g}");
    assert_eq!(g.matches("<c:ser>").count(), 1, "{g}");
    for bad in ["<c:axId", "<c:firstSliceAng", "<c:holeSize"] {
        assert!(!g.contains(bad), "{bad} is not in the ofPieChart schema: {g}");
    }
    for ax in ["<c:catAx", "<c:valAx"] {
        assert!(!xml.contains(ax), "a pie of pie has no axes: {xml}");
    }
    let at: Vec<usize> = order.iter().map(|n| pos(&g, n)).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "children out of schema order {order:?}: {g}");
    g
}

const REFS: (Option<&str>, Option<&str>, &str) = (Some("Sheet1!$B$1"), Some("Sheet1!$A$2:$A$7"), "Sheet1!$B$2:$B$7");

fn series_refs(ch: &Chart) -> Vec<(Option<&str>, Option<&str>, &str)> {
    ch.series.iter().map(|s| (s.name.as_deref(), s.categories.as_deref(), s.values.as_str())).collect()
}

#[test]
fn bar_of_pie_reads_as_its_own_kind_and_saves_back() {
    let ch = read(&of_pie("bar", EXCEL_DEFAULTS));
    assert_eq!(kind(&ch), "BarOfPie");
    assert_eq!(series_refs(&ch), [REFS]);
    let xml = save(&ch);
    let g = saved_group(&xml, "bar", &["ofPieType", "varyColors", "ser", "gapWidth", "secondPieSize", "serLines"]);
    assert_eq!(val(&g, "gapWidth"), "100", "{g}");
    assert_eq!(val(&g, "secondPieSize"), "75", "{g}");
    assert!(!g.contains("<c:splitType val=\"cust\""), "{g}");
    let back = read(&xml);
    assert_eq!(kind(&back), "BarOfPie");
    assert_eq!(series_refs(&back), [REFS]);
}

#[test]
fn pie_of_pie_reads_as_its_own_kind_and_saves_back() {
    let ch = read(&of_pie("pie", EXCEL_DEFAULTS));
    assert_eq!(kind(&ch), "PieOfPie");
    let xml = save(&ch);
    saved_group(&xml, "pie", &["ofPieType", "varyColors", "ser", "gapWidth", "secondPieSize", "serLines"]);
    assert_eq!(kind(&read(&xml)), "PieOfPie");
}

#[test]
fn split_by_value_gap_and_second_plot_size_round_trip() {
    let settings = "<c:gapWidth val=\"150\"/><c:splitType val=\"val\"/><c:splitPos val=\"1.5\"/>\
                    <c:secondPieSize val=\"60\"/><c:serLines/>";
    let ch = read(&of_pie("pie", settings));
    assert_eq!(kind(&ch), "PieOfPie");
    let xml = save(&ch);
    let g = saved_group(&xml, "pie", &["ofPieType", "varyColors", "ser", "gapWidth", "splitType", "splitPos", "secondPieSize", "serLines"]);
    assert_eq!(val(&g, "gapWidth"), "150", "{g}");
    assert_eq!(val(&g, "splitType"), "val", "{g}");
    assert_eq!(val(&g, "splitPos").parse::<f64>().unwrap(), 1.5, "{g}");
    assert_eq!(val(&g, "secondPieSize"), "60", "{g}");
    // and again after a second open/save
    let g2 = saved_group(&save(&read(&xml)), "pie", &["ofPieType", "splitType", "splitPos", "secondPieSize"]);
    assert_eq!((val(&g2, "splitType"), val(&g2, "gapWidth"), val(&g2, "secondPieSize")), ("val", "150", "60"), "{g2}");
}

#[test]
fn split_by_position_and_percent_round_trip() {
    for (split, at) in [("pos", "3"), ("percent", "10")] {
        let settings = format!("<c:gapWidth val=\"100\"/><c:splitType val=\"{split}\"/><c:splitPos val=\"{at}\"/><c:secondPieSize val=\"75\"/>");
        let xml = save(&read(&of_pie("bar", &settings)));
        let g = saved_group(&xml, "bar", &["ofPieType", "ser", "gapWidth", "splitType", "splitPos", "secondPieSize"]);
        assert_eq!(val(&g, "splitType"), split, "{g}");
        assert_eq!(val(&g, "splitPos").parse::<f64>().unwrap(), at.parse::<f64>().unwrap(), "{g}");
        assert!(!g.contains("<c:serLines"), "series lines were off in the file: {g}");
    }
}

#[test]
fn custom_split_keeps_the_chosen_points() {
    let settings = "<c:gapWidth val=\"100\"/><c:splitType val=\"cust\"/>\
                    <c:custSplit><c:secondPiePt val=\"0\"/><c:secondPiePt val=\"3\"/></c:custSplit>\
                    <c:secondPieSize val=\"75\"/><c:serLines/>";
    let ch = read(&of_pie("bar", settings));
    assert_eq!(kind(&ch), "BarOfPie");
    let xml = save(&ch);
    let g = saved_group(&xml, "bar", &["ofPieType", "ser", "gapWidth", "splitType", "custSplit", "secondPieSize", "serLines"]);
    assert_eq!(val(&g, "splitType"), "cust", "{g}");
    let cust = inner(&g, "custSplit");
    let pts: Vec<&str> = cust.split("<c:secondPiePt val=\"").skip(1).map(|s| &s[..s.find('"').unwrap()]).collect();
    assert_eq!(pts, ["0", "3"], "{cust}");
}

#[test]
fn missing_of_pie_type_means_pie_of_pie() {
    let xml = chart_xml(&format!("<c:ofPieChart><c:varyColors val=\"1\"/>{SER}{EXCEL_DEFAULTS}</c:ofPieChart>"));
    assert_eq!(kind(&read(&xml)), "PieOfPie");
}

#[test]
fn plain_and_three_d_pies_are_unchanged() {
    let pie = chart_xml(&format!("<c:pieChart><c:varyColors val=\"1\"/>{SER}<c:firstSliceAng val=\"0\"/></c:pieChart>"));
    let ch = read(&pie);
    assert_eq!(kind(&ch), "Pie");
    let xml = save(&ch);
    assert!(xml.contains("<c:pieChart>") && !xml.contains("ofPie"), "{xml}");
    let pie3d = chart_xml(&format!("<c:pie3DChart><c:varyColors val=\"1\"/>{SER}</c:pie3DChart>"));
    assert_eq!(kind(&read(&pie3d)), "Pie");
}
