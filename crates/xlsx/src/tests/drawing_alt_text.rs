//! Alternative text (`xdr:cNvPr descr`) on text boxes and chartex frames is written and read
//! back; an equation keeps its LaTeX marker in `descr` and is not given alternative text.

use std::io::Read;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::*;

use crate::{read_xlsx, write_xlsx};

fn shape(kind: ShapeKind, text: &str, alt: &str) -> Shape {
    Shape {
        hidden: false,
        hyperlink: None,
        name: None,
        alt: alt.into(),
        id: 1,
        kind,
        geometry: None,
        anchor: Anchor { cell: CellRef::parse("B2").unwrap(), width: 200.0, height: 60.0, ..Default::default() },
        fill: Color::Auto,
        line: Color::Auto,
        text: text.into(),
        format: Default::default(),
        math: Default::default(),
    }
}

fn waterfall(alt: &str) -> Chart {
    Chart {
        of_pie: None,
        theme_override: None,
        hidden: false,
        alt: alt.into(),
        id: 2,
        kind: ChartKind::Waterfall,
        anchor: Anchor { cell: CellRef::parse("E2").unwrap(), width: 300.0, height: 200.0, ..Default::default() },
        title: None,
        series: vec![Series { categories: Some("Data!$A$2:$A$4".into()), values: "Data!$B$2:$B$4".into(), ..Default::default() }],
        legend: LegendPos::Bottom,
        data_labels: false,
        data_table: None,
        gridlines: true,
        style: 2,
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

fn book(shapes: Vec<Shape>, charts: Vec<Chart>) -> Workbook {
    let mut wb = Workbook::new();
    let mut sheet = Sheet::new("Data");
    for mut s in shapes {
        s.math.refresh(&s.text.clone());
        sheet.shapes.push(s);
    }
    sheet.charts = charts;
    wb.sheets = vec![Arc::new(sheet)];
    wb
}

fn drawing(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn text_box_keeps_descr() {
    let bytes = write_xlsx(&book(vec![shape(ShapeKind::TextBox, "Note", "Totals & \"notes\"")], vec![])).unwrap();
    let d = drawing(&bytes);
    assert!(d.contains(r#"name="Shape 1" descr="Totals &amp; &quot;notes&quot;">"#), "{d}");
    assert!(d.contains(r#"<xdr:cNvSpPr txBox="1"/>"#), "{d}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    let s = &back.sheets[0].shapes[0];
    assert_eq!((s.kind, s.alt.as_str()), (ShapeKind::TextBox, "Totals & \"notes\""));
}

#[test]
fn chartex_frame_keeps_descr() {
    let bytes = write_xlsx(&book(vec![], vec![waterfall("Monthly changes")])).unwrap();
    let d = drawing(&bytes);
    assert!(d.contains(r#"<xdr:cNvPr id="2" name="Chart 1" descr="Monthly changes"/>"#), "{d}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    let c = &back.sheets[0].charts[0];
    assert_eq!((c.kind, c.alt.as_str()), (ChartKind::Waterfall, "Monthly changes"));
    // Without alternative text, no `descr` at all.
    let d = drawing(&write_xlsx(&book(vec![], vec![waterfall("")])).unwrap());
    assert!(!d.contains("descr="), "{d}");
}

#[test]
fn equation_keeps_its_marker_and_gets_no_alt_text() {
    let bytes = write_xlsx(&book(vec![shape(ShapeKind::Equation, "a^2 + b^2", "")], vec![])).unwrap();
    let d = drawing(&bytes);
    // The Office Math box and its text fallback each carry the marker, and nothing else.
    let markers = d.matches("descr=\"gridcraft:latex:").count();
    assert!(markers > 0 && markers == d.matches("descr=").count(), "{d}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    let s = &back.sheets[0].shapes[0];
    assert_eq!((s.kind, s.text.as_str(), s.alt.as_str()), (ShapeKind::Equation, "a^2 + b^2", ""));
    // Saved again, the marker is still the only description.
    let again = drawing(&write_xlsx(&back).unwrap());
    assert_eq!(again.matches("descr=\"gridcraft:latex:").count(), markers, "{again}");
    assert_eq!(again.matches("descr=").count(), markers, "{again}");
}
