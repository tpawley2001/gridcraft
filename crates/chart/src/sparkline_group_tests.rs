//! The style built from a sparkline's group settings: colours and Individual / Group / Custom
//! axis limits.

use gridcraft_core::CellRef;
use gridcraft_model::{Color, Sparkline, SparklineAxis, SparklineGroup, SparklineKind, Theme};

use crate::{SparklineGroupLimits, sparkline_style};

fn spark(row: u32, id: u32, min: SparklineAxis, max: SparklineAxis) -> Sparkline {
    Sparkline {
        cell: CellRef::new(row, 0),
        source: format!("B{}:D{}", row + 1, row + 1),
        kind: SparklineKind::Line,
        color: Color::rgb(1, 2, 3),
        markers: false,
        group: SparklineGroup { id, min_axis: min, max_axis: max, manual_min: Some(-1.0), manual_max: Some(9.0), ..Default::default() },
    }
}

fn values(sp: &Sparkline) -> Vec<Option<f64>> {
    match sp.cell.row {
        0 => vec![Some(1.0), Some(5.0)],
        1 => vec![Some(-3.0), None, Some(2.0)],
        2 => vec![Some(100.0)],
        _ => vec![Some(-50.0), Some(f64::NAN)],
    }
}

#[test]
fn group_limits_span_every_sparkline_with_the_same_id() {
    let g = SparklineAxis::Group;
    let sheet = vec![spark(0, 7, g, g), spark(1, 7, g, g), spark(2, 8, g, g), spark(3, 0, g, g)];
    let limits = SparklineGroupLimits::new(&sheet, values);
    assert_eq!(limits.get(7), Some((-3.0, 5.0)));
    assert_eq!(limits.get(8), Some((100.0, 100.0)));
    let theme = Theme::default();
    let first = sheet.first().map(|sp| sparkline_style(sp, &theme, [0; 3], &values(sp), &limits));
    assert_eq!(first.as_ref().map(|s| (s.min, s.max)), Some((Some(-3.0), Some(5.0))));
    assert_eq!(first.map(|s| s.color), Some([1, 2, 3, 255]));
    // Group id 0 is a group of its own: its own values.
    let own = sheet.get(3).map(|sp| sparkline_style(sp, &theme, [0; 3], &values(sp), &limits));
    assert_eq!(own.map(|s| (s.min, s.max)), Some((Some(-50.0), Some(-50.0))));
}

#[test]
fn individual_and_custom_limits() {
    let sp = spark(0, 7, SparklineAxis::Individual, SparklineAxis::Custom);
    let limits = SparklineGroupLimits::new(std::slice::from_ref(&sp), values);
    assert_eq!(limits.get(7), None);
    let s = sparkline_style(&sp, &Theme::default(), [0; 3], &values(&sp), &limits);
    assert_eq!((s.min, s.max), (None, Some(9.0)));
}
