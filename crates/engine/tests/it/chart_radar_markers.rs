//! Excel's two line radar types are both `c:radarChart/c:radarStyle val="marker"` (ECMA-376 Part 1 §21.2.2.155);
//! the series' `c:marker` tells them apart (§21.2.2.106, §21.2.2.199):
//! - "Radar with Markers": no `c:marker` on the series -> Excel draws a marker at every data point;
//! - "Radar": every series has `<c:marker><c:symbol val="none"/></c:marker>` -> lines only, and Excel keeps the
//!   `symbol none` when it saves.
//! Measured in Excel: a 3-series x 5-category "Radar with Markers" prints 15 markers; the plain radar prints none.

use std::io::{Read, Write};

use gridcraft_chart::{ApproxMeasure, Prim, render, resolve};
use gridcraft_engine::Session;
use serde_json::json;

const NONE_MARKER: &str = "<c:marker><c:symbol val=\"none\"/></c:marker>";

/// A radar chart (`insert.chart` of `kind`) over two series and five categories, saved as .xlsx.
fn saved(kind: &str) -> Vec<u8> {
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
    s.execute("insert.chart", json!({"range": "A1:C6", "type": kind})).unwrap();
    gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap()
}

fn chart_xml(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name("xl/charts/chart1.xml").expect("chart part");
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
}

/// The package with its chart part replaced by `edit(chart part)`.
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

/// `bytes` with the chart's legend removed, so every circle drawn is a data marker (a legend swatch can be one too).
fn no_legend(bytes: &[u8]) -> Vec<u8> {
    edit_chart(bytes, |x| match (x.find("<c:legend>"), x.find("</c:legend>")) {
        (Some(a), Some(b)) => format!("{}{}", &x[..a], &x[b + "</c:legend>".len()..]),
        _ => x,
    })
}

/// Our radar with every series' `c:marker` removed and radarStyle `marker`: Excel's "Radar with Markers".
fn excel_radar_with_markers() -> Vec<u8> {
    edit_chart(&no_legend(&saved("radar")), |x| {
        let x = x.replace(NONE_MARKER, "");
        let start = x.find("<c:radarStyle ").expect("the writer emits c:radarStyle");
        let end = start + x[start..].find("/>").unwrap() + 2;
        format!("{}<c:radarStyle val=\"marker\"/>{}", &x[..start], &x[end..])
    })
}

/// Excel's plain "Radar": radarStyle `marker` and `symbol none` on every series, in CT_RadarSer order
/// (right before `c:cat`/`c:val`).
fn excel_plain_radar() -> Vec<u8> {
    edit_chart(&excel_radar_with_markers(), |x| {
        let mut out = String::new();
        for (i, part) in x.split("<c:ser>").enumerate() {
            if i == 0 {
                out.push_str(part);
                continue;
            }
            out.push_str("<c:ser>");
            let at = ["<c:cat>", "<c:val>"].iter().filter_map(|t| part.find(t)).min().expect("series has c:val");
            out.push_str(&part[..at]);
            out.push_str(NONE_MARKER);
            out.push_str(&part[at..]);
        }
        out
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

/// Marker circles filled with colour `rgb` (alpha ignored).
fn markers_in(prims: &[Prim], rgb: [u8; 3]) -> usize {
    prims.iter().filter(|p| matches!(p, Prim::Circle { fill: Some(f), .. } if [f[0], f[1], f[2]] == rgb)).count()
}

fn resave(bytes: &[u8]) -> String {
    let (wb, _) = gridcraft_xlsx::read_xlsx(bytes).unwrap();
    chart_xml(&gridcraft_xlsx::write_xlsx(&wb).unwrap())
}

fn radar_part(xml: &str) -> &str {
    let a = xml.find("<c:radarChart>").expect("radarChart");
    let b = xml.find("</c:radarChart>").expect("radarChart end");
    &xml[a..b]
}

#[test]
fn radar_with_markers_draws_a_marker_on_every_point() {
    let (prims, colors) = draw(&excel_radar_with_markers());
    assert_eq!(colors.len(), 2);
    for (i, rgb) in colors.iter().enumerate() {
        assert_eq!(markers_in(&prims, *rgb), 5, "series {i}: one marker per category in its colour {rgb:?}");
    }
}

#[test]
fn plain_radar_draws_no_markers() {
    let (prims, colors) = draw(&excel_plain_radar());
    for rgb in &colors {
        assert_eq!(markers_in(&prims, *rgb), 0, "a plain radar has no markers");
    }
}

#[test]
fn radar_with_markers_is_saved_with_markers() {
    let xml = resave(&excel_radar_with_markers());
    let part = radar_part(&xml);
    assert!(part.contains("<c:radarStyle val=\"marker\"/>"), "{part}");
    assert!(!part.contains("<c:symbol val=\"none\"/>"), "markers must not be switched off: {part}");
}

#[test]
fn plain_radar_is_saved_without_markers() {
    let xml = resave(&excel_plain_radar());
    let part = radar_part(&xml);
    assert!(part.contains("<c:radarStyle val=\"marker\"/>"), "{part}");
    assert_eq!(part.matches(NONE_MARKER).count(), 2, "every series keeps symbol none: {part}");
}

#[test]
fn inserted_radar_is_a_plain_radar_in_excel() {
    // insert.chart "radar" is Excel's plain Radar: Excel must not show markers on it.
    let xml = chart_xml(&saved("radar"));
    let part = radar_part(&xml);
    assert_eq!(part.matches(NONE_MARKER).count(), 2, "{part}");
    let (prims, colors) = draw(&no_legend(&saved("radar")));
    for rgb in &colors {
        assert_eq!(markers_in(&prims, *rgb), 0);
    }
}

#[test]
fn inserted_radar_with_markers_round_trips() {
    let bytes = saved("radarMarkers");
    let part = radar_part(&chart_xml(&bytes)).to_string();
    assert!(part.contains("<c:radarStyle val=\"marker\"/>") && !part.contains("<c:symbol val=\"none\"/>"), "{part}");
    let (prims, colors) = draw(&no_legend(&bytes));
    for rgb in &colors {
        assert_eq!(markers_in(&prims, *rgb), 5);
    }
}

#[test]
fn filled_radar_is_unchanged() {
    let bytes = edit_chart(&no_legend(&saved("radar")), |x| x.replace("<c:radarStyle val=\"marker\"/>", "<c:radarStyle val=\"filled\"/>"));
    let part = resave(&bytes);
    assert!(radar_part(&part).contains("<c:radarStyle val=\"filled\"/>"));
    let (prims, colors) = draw(&bytes);
    for rgb in &colors {
        assert_eq!(markers_in(&prims, *rgb), 0, "a filled radar has no markers");
    }
}
