//! The four surface chart types: `insert.chart` and `chart.set` take `surface`,
//! `surfaceWireframe`, `contour` and `contourWireframe`, and each prints into the PDF export.

use gridcraft_engine::Session;
use gridcraft_model::ChartKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([["", "S1", "S2", "S3"], ["a", "0", "20", "40"], ["b", "20", "40", "60"], ["c", "40", "60", "80"]]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

fn kinds(s: &Session) -> Vec<ChartKind> {
    s.doc().unwrap().wb.sheet(0).unwrap().charts.iter().map(|c| c.kind).collect()
}

fn pdf_bytes(s: &mut Session) -> u64 {
    s.execute("file.exportPdf", json!({})).unwrap()["bytes"].as_u64().unwrap()
}

const TYPES: [(&str, ChartKind); 4] = [
    ("surface", ChartKind::Surface),
    ("surfaceWireframe", ChartKind::SurfaceWireframe),
    ("contour", ChartKind::Contour),
    ("contourWireframe", ChartKind::ContourWireframe),
];

#[test]
fn insert_chart_makes_each_surface_type() {
    for (t, kind) in TYPES {
        let mut s = session();
        s.execute("insert.chart", json!({"range": "A1:D4", "type": t})).unwrap();
        assert_eq!(kinds(&s), [kind], "{t}");
    }
    let mut s = session();
    s.execute("insert.chart", json!({"range": "A1:D4", "type": "contour", "subtype": "wireframe"})).unwrap();
    assert_eq!(kinds(&s), [ChartKind::ContourWireframe]);
}

#[test]
fn chart_set_switches_between_surface_types_and_undoes() {
    let mut s = session();
    s.execute("insert.chart", json!({"range": "A1:D4", "type": "column"})).unwrap();
    s.execute("chart.set", json!({"type": "surface"})).unwrap();
    assert_eq!(kinds(&s), [ChartKind::Surface]);
    s.execute("chart.set", json!({"type": "contourWireframe"})).unwrap();
    assert_eq!(kinds(&s), [ChartKind::ContourWireframe]);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(kinds(&s), [ChartKind::Surface]);
}

#[test]
fn every_surface_type_prints_into_the_pdf() {
    let mut plain = session();
    let without = pdf_bytes(&mut plain);
    for (t, _) in TYPES {
        let mut s = session();
        s.execute("insert.chart", json!({"range": "A1:D4", "type": t})).unwrap();
        let with = pdf_bytes(&mut s);
        assert!(with > without, "{t}: the chart adds drawing to the page ({with} vs {without} bytes)");
    }
}
