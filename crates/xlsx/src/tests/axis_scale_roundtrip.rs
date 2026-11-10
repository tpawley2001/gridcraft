//! Format Axis ▸ Bounds/Units/log scale/reverse order were dropped on every save (the writer
//! always wrote an empty `c:scaling`). Found by round-tripping a chart with fixed bounds.

use std::io::Read;
use std::sync::Arc;

use gridcraft_model::{Anchor, AxisScale, Chart, ChartKind, ChartScale, LegendPos, Series, Sheet, Workbook};

use crate::{read_xlsx, write_xlsx};

fn chart(id: u32, kind: ChartKind, scale: ChartScale) -> Chart {
    Chart {
        of_pie: None,
        alt: String::new(),
        theme_override: None,
        hidden: false,
        id,
        kind,
        anchor: Anchor { cell: gridcraft_core::CellRef::new(0, 3), width: 480.0, height: 300.0, ..Anchor::default() },
        title: None,
        series: vec![Series {
            name: None,
            categories: Some("Data!$A$2:$A$5".into()),
            values: "Data!$B$2:$B$5".into(),
            bubble_sizes: None,
            color: None,
            secondary: false,
            kind: None,
            quartiles: Default::default(),
            smooth: false,
            trendlines: Vec::new(),
            explosion: 0,
            point_explosions: Vec::new(),
            bar_shape: None,
        }],
        legend: LegendPos::Bottom,
        data_labels: false,
        gridlines: true,
        style: 2,
        hole_size: 75,
        first_slice_angle: 0,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        pivot: None,
        data_table: None,
        scale,
        view_3d: None,
    }
}

fn round_trip(charts: Vec<Chart>) -> (Vec<Chart>, String) {
    let mut wb = Workbook::default();
    let mut s = Sheet::new("Data");
    s.charts = charts;
    wb.sheets = vec![Arc::new(s)];
    let bytes = write_xlsx(&wb).unwrap();
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap().by_name("xl/charts/chart1.xml").unwrap().read_to_string(&mut xml).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    (back.sheets[0].charts.clone(), xml)
}

#[test]
fn value_axis_bounds_units_log_base_and_order_survive() {
    let y = AxisScale {
        min: Some(10000.0),
        max: Some(150000.0),
        major_unit: Some(20000.0),
        minor_unit: Some(5000.0),
        log_base: None,
        reverse: true,
        ..AxisScale::default()
    };
    let x = AxisScale {
        min: Some(-5.0),
        max: Some(5.0),
        major_unit: None,
        minor_unit: None,
        log_base: Some(10.0),
        reverse: false,
        ..AxisScale::default()
    };
    let column = ChartScale { x: AxisScale::default(), y, ..ChartScale::default() };
    let scatter = ChartScale { x, y: AxisScale { max: Some(1.0), ..AxisScale::default() }, ..ChartScale::default() };
    let (back, xml) = round_trip(vec![chart(1, ChartKind::ColumnClustered, column), chart(2, ChartKind::Scatter, scatter)]);
    assert_eq!(back[0].scale, column);
    assert_eq!(back[1].scale, scatter);
    // Schema order: orientation, max, min inside scaling; crossBetween, majorUnit, minorUnit.
    assert!(xml.contains(r#"<c:scaling><c:orientation val="maxMin"/><c:max val="150000"/><c:min val="10000"/></c:scaling>"#), "{xml}");
    assert!(xml.contains(r#"<c:crossBetween val="between"/><c:majorUnit val="20000"/><c:minorUnit val="5000"/>"#), "{xml}");
}

#[test]
fn unusable_scaling_reads_as_automatic() {
    let bad = AxisScale {
        min: Some(5.0),
        max: Some(5.0),
        major_unit: Some(-1.0),
        minor_unit: Some(f64::NAN),
        log_base: Some(1.0),
        reverse: false,
        ..AxisScale::default()
    };
    let (back, _) = round_trip(vec![chart(1, ChartKind::Line, ChartScale { x: AxisScale::default(), y: bad, ..ChartScale::default() })]);
    assert!(back[0].scale.is_auto(), "{:?}", back[0].scale);
}
