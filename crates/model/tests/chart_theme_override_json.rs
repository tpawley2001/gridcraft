//! A chart's theme override part is stored as base64 text in the JSON model, and older documents that stored it
//! as a number array still open.

use gridcraft_model::{Anchor, Chart, ChartKind, LegendPos};
use serde_json::{Value, json};

/// `<a></a>` as bytes and as base64.
const BYTES: &[u8] = b"<a></a>";
const B64: &str = "PGE+PC9hPg==";

fn chart() -> Chart {
    Chart {
        rounded_corners: false,
        lang: None,
        print_settings: None,
        of_pie: None,
        alt: String::new(),
        theme_override: None,
        hidden: false,
        no_print: false,
        id: 1,
        kind: ChartKind::Doughnut,
        anchor: Anchor::default(),
        title: None,
        series: vec![],
        legend: LegendPos::Right,
        data_labels: false,
        data_table: None,
        gridlines: false,
        style: 1,
        hole_size: 75,
        first_slice_angle: 0,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        pivot: None,
        scale: Default::default(),
        view_3d: None,
    }
}

fn with_override(value: Value) -> Chart {
    let mut json = serde_json::to_value(chart()).expect("serialize chart");
    json.as_object_mut().expect("chart object").insert("theme_override".into(), value);
    serde_json::from_value(json).expect("deserialize chart")
}

fn bytes(c: &Chart) -> Option<Vec<u8>> {
    c.theme_override.as_deref().map(|b| b[..].to_vec())
}

#[test]
fn the_override_is_written_as_base64_text() {
    let c = with_override(json!(B64));
    assert_eq!(bytes(&c).as_deref(), Some(BYTES));
    let out = serde_json::to_value(&c).unwrap();
    assert_eq!(out["theme_override"], json!(B64), "base64 text, not a number array");
    let back: Chart = serde_json::from_value(out).unwrap();
    assert_eq!(back, c);
}

#[test]
fn a_number_array_from_an_older_document_still_reads() {
    let c = with_override(json!(BYTES.iter().map(|b| u32::from(*b)).collect::<Vec<_>>()));
    assert_eq!(bytes(&c).as_deref(), Some(BYTES));
    assert_eq!(serde_json::to_value(&c).unwrap()["theme_override"], json!(B64));
}

#[test]
fn no_override_writes_no_key() {
    let out = serde_json::to_value(chart()).unwrap();
    assert!(out.get("theme_override").is_none());
    let c: Chart = serde_json::from_value(out).unwrap();
    assert!(c.theme_override.is_none());
}

#[test]
fn invalid_base64_is_an_error() {
    let mut json = serde_json::to_value(chart()).unwrap();
    json.as_object_mut().unwrap().insert("theme_override".into(), json!("not base64 !!"));
    assert!(serde_json::from_value::<Chart>(json).is_err());
}
