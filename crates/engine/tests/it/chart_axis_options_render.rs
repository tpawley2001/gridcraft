//! Axis display units and axis crossing points read from a file are drawn like Excel: labels divided
//! by the unit with the unit's label, the category axis at a value, the value axis at a category.
//! Expected positions are from Excel's own prints of charts with these settings.

use std::io::{Read, Write};

use gridcraft_chart::{ApproxMeasure, Prim, render, resolve};
use gridcraft_engine::Session;
use serde_json::json;

const AUTO: &str = "<c:crosses val=\"autoZero\"/>";

/// A column chart over `values` (categories "Category 1".."Category n"), saved as .xlsx.
fn saved(values: &[f64]) -> Vec<u8> {
    let mut s = Session::new();
    s.new_workbook();
    let mut rows = vec![json!(["Category", "Series 1"])];
    for (i, v) in values.iter().enumerate() {
        rows.push(json!([format!("Category {}", i + 1), v.to_string()]));
    }
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    let range = format!("A1:B{}", values.len() + 1);
    s.execute("insert.chart", json!({"range": range, "type": "column"})).unwrap();
    gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap()
}

/// The package with `edit` applied to its chart part.
fn edit_chart(bytes: &[u8], edit: impl Fn(String) -> String) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "xl/charts/chart1.xml" {
            data = edit(String::from_utf8(data).unwrap()).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// Replaces the crossing of the `c:{axis}` element with `crossing`; `val_tail` goes at the end of `c:valAx`.
fn set_axis(xml: String, axis: &str, crossing: &str, val_tail: &str) -> String {
    let open = format!("<c:{axis}>");
    let start = xml.find(&open).unwrap_or_else(|| panic!("no {open} in {xml}"));
    let at = start + xml[start..].find(AUTO).unwrap_or_else(|| panic!("no automatic crossing in {axis}: {xml}"));
    let mut xml = format!("{}{crossing}{}", &xml[..at], &xml[at + AUTO.len()..]);
    let end = xml.find("</c:valAx>").expect("value axis");
    xml.insert_str(end, val_tail);
    xml
}

/// Draws the first chart of the reopened workbook at 480 x 288.
fn draw(bytes: &[u8]) -> Vec<Prim> {
    let (wb, _) = gridcraft_xlsx::read_xlsx(bytes).unwrap();
    let chart = wb.sheet(0).unwrap().charts[0].clone();
    let data = resolve(&wb, 0, &chart);
    render(&chart, &data, 480.0, 288.0, &ApproxMeasure)
}

fn texts(prims: &[Prim]) -> Vec<(String, f32, f32)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, x, y, .. } => Some((text.clone(), *x, *y)),
            _ => None,
        })
        .collect()
}

/// Anchor `(x, y)` of the text `t`.
fn at(prims: &[Prim], t: &str) -> (f32, f32) {
    let all = texts(prims);
    all.iter().find(|(s, ..)| s == t).map(|(_, x, y)| (*x, *y)).unwrap_or_else(|| panic!("no text {t:?} in {all:?}"))
}

fn numeric_labels(prims: &[Prim]) -> Vec<f64> {
    texts(prims).iter().filter_map(|(s, ..)| s.replace(',', "").parse::<f64>().ok()).collect()
}

const TENS_OF_MILLIONS: [f64; 5] = [10_000_000.0, 20_000_000.0, 30_000_000.0, 20_000_000.0, 10_000_000.0];

#[test]
fn display_units_divide_the_value_axis_labels() {
    // Excel's print of this chart with tenMillions: 0, 0.5, 1 ... 3.5.
    let bytes =
        edit_chart(&saved(&TENS_OF_MILLIONS), |x| set_axis(x, "valAx", AUTO, "<c:dispUnits><c:builtInUnit val=\"tenMillions\"/></c:dispUnits>"));
    let p = draw(&bytes);
    let nums = numeric_labels(&p);
    for want in [0.5, 1.0, 3.0] {
        assert!(nums.contains(&want), "label {want} missing: {:?}", texts(&p));
    }
    assert!(nums.iter().all(|v| *v < 10.0), "labels not divided by ten million: {nums:?}");
    // No label shown without c:dispUnitsLbl.
    assert!(!texts(&p).iter().any(|(s, ..)| s.starts_with("x ") || s.contains("Millions")), "{:?}", texts(&p));

    // Trillions: 0.000005 steps.
    let bytes =
        edit_chart(&saved(&TENS_OF_MILLIONS), |x| set_axis(x, "valAx", AUTO, "<c:dispUnits><c:builtInUnit val=\"trillions\"/></c:dispUnits>"));
    let nums = numeric_labels(&draw(&bytes));
    assert!(nums.iter().any(|v| (*v - 0.00003).abs() < 1e-12), "{nums:?}");
    assert!(nums.iter().all(|v| *v < 0.001), "{nums:?}");
}

#[test]
fn the_display_units_label_reads_like_excels() {
    // Measured from Excel's prints: tenThousands shows "x 10000", hundreds shows "Hundreds".
    for (unit, label) in [("tenThousands", "x 10000"), ("hundreds", "Hundreds")] {
        let bytes = edit_chart(&saved(&[560.0, 2900.0, 3130.0, 1040.0]), |x| {
            set_axis(x, "valAx", AUTO, &format!("<c:dispUnits><c:builtInUnit val=\"{unit}\"/><c:dispUnitsLbl/></c:dispUnits>"))
        });
        let p = draw(&bytes);
        at(&p, label);
    }
}

#[test]
fn the_category_axis_sits_at_the_value_it_crosses() {
    // Excel (catAx crossesAt 2): "Category 1" is drawn just under the value-2 gridline, not under 0.
    let bytes = edit_chart(&saved(&[4.3, 2.5, 3.5, 4.5]), |x| set_axis(x, "catAx", "<c:crossesAt val=\"2\"/>", ""));
    let p = draw(&bytes);
    let (_, cat_y) = at(&p, "Category 1");
    let (_, two_y) = at(&p, "2");
    let (_, zero_y) = at(&p, "0");
    assert!((cat_y - two_y).abs() < (cat_y - zero_y).abs(), "category labels at y {cat_y}, value 2 at {two_y}, 0 at {zero_y}");
    assert!(cat_y > two_y, "labels go under the axis line: {cat_y} vs {two_y}");
}

#[test]
fn the_value_axis_sits_at_the_category_it_crosses() {
    // Excel (valAx crossesAt 3, four categories): the value labels sit between Category 2 and Category 3.
    let bytes = edit_chart(&saved(&[4.3, 2.5, 3.5, 4.5]), |x| set_axis(x, "valAx", "<c:crossesAt val=\"3\"/>", ""));
    let p = draw(&bytes);
    let (zero_x, _) = at(&p, "0");
    let (c2, _) = at(&p, "Category 2");
    let (c3, _) = at(&p, "Category 3");
    assert!(c2 < zero_x && zero_x < c3, "value labels at x {zero_x}, Category 2 at {c2}, Category 3 at {c3}");
}

#[test]
fn automatic_crossing_draws_as_before() {
    let p = draw(&saved(&[4.3, 2.5, 3.5, 4.5]));
    let (zero_x, zero_y) = at(&p, "0");
    let (c1x, c1y) = at(&p, "Category 1");
    assert!(zero_x < c1x, "value axis on the left: {zero_x} vs {c1x}");
    assert!(c1y > zero_y, "categories under value 0: {c1y} vs {zero_y}");
}
