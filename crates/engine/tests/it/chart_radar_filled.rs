//! A filled radar chart (`c:radarChart/c:radarStyle val="filled"`, ECMA-376 Part 1 §21.2.2.155,
//! ST_RadarStyle) opens as a filled radar, draws each series as a filled polygon in its colour,
//! and is saved back as `filled`; a marker radar is unchanged.

use std::io::{Read, Write};

use gridcraft_chart::{ApproxMeasure, Prim, render, resolve};
use gridcraft_engine::Session;
use serde_json::json;

/// A radar chart over two series and five categories, saved as .xlsx.
fn saved_radar() -> Vec<u8> {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([
        ["Axis", "North", "South"],
        ["Speed", "4", "2"],
        ["Range", "3", "5"],
        ["Cost", "5", "3"],
        ["Comfort", "2", "4"],
        ["Safety", "4", "4"]
    ]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:C6", "type": "radar"})).unwrap();
    gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap()
}

fn chart_xml(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name("xl/charts/chart1.xml").expect("chart part");
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
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

/// Our radar chart with its radar style set to `style` (as Excel writes it).
fn radar_with_style(style: &str) -> Vec<u8> {
    let bytes = saved_radar();
    assert!(chart_xml(&bytes).contains("<c:radarStyle "), "the writer emits c:radarStyle");
    edit_chart(&bytes, |x| {
        let start = x.find("<c:radarStyle ").unwrap();
        let end = start + x[start..].find("/>").unwrap() + 2;
        format!("{}<c:radarStyle val=\"{style}\"/>{}", &x[..start], &x[end..])
    })
}

/// Draws the first chart of the workbook; also returns the series colours.
fn draw(bytes: &[u8]) -> (Vec<Prim>, Vec<[u8; 3]>) {
    let (wb, _) = gridcraft_xlsx::read_xlsx(bytes).unwrap();
    let chart = wb.sheet(0).unwrap().charts[0].clone();
    let data = resolve(&wb, 0, &chart);
    let colors = data.series.iter().map(|s| [s.color[0], s.color[1], s.color[2]]).collect();
    (render(&chart, &data, 480.0, 360.0, &ApproxMeasure), colors)
}

/// Filled polygons (at least five points: one per category) in colour `rgb`, alpha ignored.
fn filled_in(prims: &[Prim], rgb: [u8; 3]) -> usize {
    prims
        .iter()
        .filter(|p| matches!(p, Prim::Polygon { pts, fill, .. } if pts.len() >= 5 && [fill[0], fill[1], fill[2]] == rgb && fill[3] > 0))
        .count()
}

#[test]
fn filled_radar_draws_each_series_as_a_filled_polygon() {
    let (prims, colors) = draw(&radar_with_style("filled"));
    assert_eq!(colors.len(), 2);
    for (i, rgb) in colors.iter().enumerate() {
        assert_eq!(filled_in(&prims, *rgb), 1, "series {i} is one filled area in its colour {rgb:?}");
    }
}

#[test]
fn filled_radar_is_saved_as_filled() {
    let (wb, _) = gridcraft_xlsx::read_xlsx(&radar_with_style("filled")).unwrap();
    let again = gridcraft_xlsx::write_xlsx(&wb).unwrap();
    let xml = chart_xml(&again);
    assert!(xml.contains("<c:radarStyle val=\"filled\"/>"), "{xml}");
}

#[test]
fn marker_radar_stays_lines() {
    let bytes = radar_with_style("marker");
    let (prims, colors) = draw(&bytes);
    for rgb in &colors {
        assert_eq!(filled_in(&prims, *rgb), 0, "a marker radar has no filled series area");
    }
    let (wb, _) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    let xml = chart_xml(&gridcraft_xlsx::write_xlsx(&wb).unwrap());
    assert!(xml.contains("<c:radarStyle val=\"marker\"/>"), "{xml}");
}
