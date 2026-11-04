//! A combination chart made of pie and doughnut groups saves as pie and doughnut groups, the way
//! Excel keeps it, instead of becoming clustered columns.

use gridcraft_model::{Chart, ChartKind, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

fn ser(i: usize, col: &str) -> String {
    format!("<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/><c:val><c:numRef><c:f>Sheet1!${col}$3:${col}$6</c:f></c:numRef></c:val></c:ser>")
}

fn pie(s: &str) -> String {
    format!("<c:pieChart><c:varyColors val=\"1\"/>{s}<c:firstSliceAng val=\"0\"/></c:pieChart>")
}

fn doughnut(s: &str) -> String {
    format!("<c:doughnutChart><c:varyColors val=\"1\"/>{s}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"50\"/></c:doughnutChart>")
}

const AX: &str = "<c:axId val=\"1\"/><c:axId val=\"2\"/>";
const AXES: &str = "<c:catAx><c:axId val=\"1\"/><c:crossAx val=\"2\"/></c:catAx><c:valAx><c:axId val=\"2\"/><c:crossAx val=\"1\"/></c:valAx>";

fn chart(groups: &str) -> Chart {
    let xml = format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">\
         <c:chart><c:plotArea>{groups}</c:plotArea></c:chart></c:chartSpace>"
    );
    read_chart(&parse(xml.as_bytes()).unwrap()).unwrap()
}

fn book() -> Workbook {
    Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() }
}

fn save(ch: &Chart) -> String {
    write_chart(&book(), 0, ch, "Book1")
}

#[test]
fn doughnut_and_pie_combo_keeps_both_groups_and_no_axes() {
    let ch = chart(&(doughnut(&ser(0, "H")) + &pie(&ser(1, "I"))));
    assert_eq!(ch.kind, ChartKind::Combo);
    let xml = save(&ch);
    assert_eq!(xml.matches("<c:doughnutChart>").count(), 1, "{xml}");
    assert_eq!(xml.matches("<c:pieChart>").count(), 1, "{xml}");
    assert!(xml.find("<c:doughnutChart>") < xml.find("<c:pieChart>"), "{xml}");
    assert!(!xml.contains("<c:barChart"), "{xml}");
    assert!(!xml.contains("<c:catAx"), "{xml}");
    assert!(!xml.contains("<c:valAx"), "{xml}");
    let back = read_chart(&parse(xml.as_bytes()).unwrap()).unwrap();
    assert_eq!(back.kind, ChartKind::Combo);
    let kinds: Vec<_> = back.series.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![Some(ChartKind::Doughnut), Some(ChartKind::Pie)]);
    let vals: Vec<_> = back.series.iter().map(|s| s.values.as_str()).collect();
    assert_eq!(vals, vec!["Sheet1!$H$3:$H$6", "Sheet1!$I$3:$I$6"]);
}

#[test]
fn pie_then_doughnut_keeps_file_order() {
    let xml = save(&chart(&(pie(&ser(0, "H")) + &doughnut(&ser(1, "I")))));
    let (p, d) = (xml.find("<c:pieChart>"), xml.find("<c:doughnutChart>"));
    assert!(p.is_some() && d.is_some() && p < d, "{xml}");
    assert!(!xml.contains("<c:barChart"), "{xml}");
}

#[test]
fn bar_and_line_combo_is_unchanged() {
    let bar = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/>{}{AX}</c:barChart>", ser(0, "H"));
    let line = format!("<c:lineChart><c:grouping val=\"standard\"/>{}{AX}</c:lineChart>", ser(1, "I"));
    let xml = save(&chart(&(bar + &line + AXES)));
    let (b, l) = (xml.find("<c:barChart>"), xml.find("<c:lineChart>"));
    assert!(b.is_some() && l.is_some() && b < l, "{xml}");
    assert!(xml.contains("<c:catAx>") && xml.contains("<c:valAx>"), "{xml}");
}
