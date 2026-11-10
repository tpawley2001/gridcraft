//! Exploded pie and doughnut slices: `c:explosion` on the series (every slice) and inside `c:dPt`
//! (one slice) is kept when the chart is opened and saved, in the schema position of CT_PieSer / CT_DPt.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

/// One series of a pie-type group; `extra` goes after `c:tx` (explosion, dPt).
fn chart_xml(group: &str, extra: &str) -> String {
    let hole = if group == "doughnutChart" { "<c:firstSliceAng val=\"0\"/><c:holeSize val=\"50\"/>" } else { "" };
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
         <c:{group}><c:varyColors val=\"1\"/>\
         <c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx>{extra}\
         <c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f></c:strRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f></c:numRef></c:val></c:ser>{hole}</c:{group}>\
         </c:plotArea></c:chart></c:chartSpace>"
    )
}

fn read(xml: &str) -> Chart {
    read_chart(&parse(xml.as_bytes()).unwrap()).expect("pie chart must be read")
}

fn save(ch: &Chart) -> String {
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() };
    write_chart(&wb, 0, ch, "Book1")
}

/// The text between the first `<c:ser>` and its `</c:ser>`.
fn ser(xml: &str) -> &str {
    let s = xml.find("<c:ser>").unwrap_or_else(|| panic!("no series in {xml}")) + "<c:ser>".len();
    &xml[s..s + xml[s..].find("</c:ser>").unwrap()]
}

/// `(idx, explosion)` of every `c:dPt` that has an explosion, in document order.
fn point_explosions(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in s.split("<c:dPt>").skip(1) {
        let d = &part[..part.find("</c:dPt>").unwrap_or_else(|| panic!("unclosed dPt in {s}"))];
        let v = |name: &str| {
            let tag = format!("<c:{name} val=\"");
            d.find(&tag).map(|i| {
                let t = &d[i + tag.len()..];
                t[..t.find('"').unwrap()].to_string()
            })
        };
        if let Some(e) = v("explosion") {
            let idx = v("idx").unwrap_or_else(|| panic!("dPt without idx: {d}"));
            assert!(d.find("<c:idx").unwrap() < d.find("<c:explosion").unwrap(), "idx before explosion in dPt: {d}");
            out.push((idx, e));
        }
    }
    out
}

/// Series-level explosion: the `c:explosion` that is not inside a `c:dPt`.
fn series_explosion(s: &str) -> Option<String> {
    let head = &s[..s.find("<c:dPt>").unwrap_or(s.len())];
    let i = head.find("<c:explosion val=\"")? + "<c:explosion val=\"".len();
    Some(head[i..i + head[i..].find('"').unwrap()].to_string())
}

/// CT_PieSer order: tx, spPr, explosion, dPt*, dLbls, cat, val.
fn check_order(s: &str) {
    let names = ["<c:tx>", "<c:spPr", "<c:explosion", "<c:dPt>", "<c:dLbls", "<c:cat>", "<c:val>"];
    let head_end = s.find("<c:dPt>").unwrap_or(s.len());
    let found: Vec<(usize, &str)> = names
        .iter()
        .filter_map(|n| {
            // A series explosion is the one before the first dPt; dPt explosions are checked separately.
            let hay = if *n == "<c:explosion" { &s[..head_end] } else { s };
            hay.find(n).map(|i| (i, *n))
        })
        .collect();
    for w in found.windows(2) {
        assert!(w[0].0 < w[1].0, "{} must come before {}: {s}", w[0].1, w[1].1);
    }
}

fn pe(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn series_explosion_is_kept() {
    let out = save(&read(&chart_xml("pieChart", "<c:explosion val=\"25\"/>")));
    let s = ser(&out);
    assert_eq!(series_explosion(s).as_deref(), Some("25"), "{s}");
    assert!(point_explosions(s).is_empty(), "{s}");
    check_order(s);
    assert_eq!(save(&read(&out)), out, "saving again changes nothing");
}

#[test]
fn one_exploded_point_is_kept() {
    let x = "<c:dPt><c:idx val=\"0\"/><c:bubble3D val=\"0\"/></c:dPt>\
             <c:dPt><c:idx val=\"1\"/><c:bubble3D val=\"0\"/><c:explosion val=\"20\"/></c:dPt>\
             <c:dPt><c:idx val=\"2\"/><c:bubble3D val=\"0\"/></c:dPt>";
    let out = save(&read(&chart_xml("pieChart", x)));
    let s = ser(&out);
    assert_eq!(series_explosion(s), None, "{s}");
    assert_eq!(point_explosions(s), pe(&[("1", "20")]), "{s}");
    check_order(s);
}

#[test]
fn series_and_point_explosion_together() {
    let x = "<c:explosion val=\"10\"/><c:dPt><c:idx val=\"2\"/><c:bubble3D val=\"0\"/><c:explosion val=\"25\"/></c:dPt>";
    let out = save(&read(&chart_xml("pieChart", x)));
    let s = ser(&out);
    assert_eq!(series_explosion(s).as_deref(), Some("10"), "{s}");
    assert_eq!(point_explosions(s), pe(&[("2", "25")]), "{s}");
    check_order(s);
}

#[test]
fn doughnut_explosion_is_kept() {
    let out = save(&read(&chart_xml("doughnutChart", "<c:explosion val=\"25\"/>")));
    let s = ser(&out);
    assert_eq!(series_explosion(s).as_deref(), Some("25"), "{s}");
    check_order(s);
}

#[test]
fn absurd_explosion_is_clamped_to_400() {
    let out = save(&read(&chart_xml("pieChart", "<c:explosion val=\"4000000000\"/>")));
    assert_eq!(series_explosion(ser(&out)).as_deref(), Some("400"), "{out}");
}

/// Guard (passes before the change): a pie without explosion writes neither explosion nor dPt.
#[test]
fn closed_pie_writes_no_explosion() {
    let out = save(&read(&chart_xml("pieChart", "")));
    assert!(!out.contains("<c:explosion"), "{out}");
    assert!(!out.contains("<c:dPt>"), "{out}");
}
