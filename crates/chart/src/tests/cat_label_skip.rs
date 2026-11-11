//! A category axis's "Interval between labels" (`chart.scale.tick_label_skip`, `c:tickLblSkip`):
//! every n-th category label is drawn, starting at the first. Measured in Excel's PDFs: five
//! categories with an interval of 3 label only the 1st and 4th; with 2, the 1st, 3rd and 5th.

use super::*;

const BLUE: Rgba = [20, 96, 130, 255];

fn plain(kind: ChartKind, skip: Option<u32>) -> Chart {
    let mut c = chart(kind);
    c.title = None;
    c.x_title = None;
    c.y_title = None;
    c.legend = LegendPos::None;
    c.data_labels = false;
    c.scale.tick_label_skip = skip;
    c
}

/// Five short categories that fit easily, so the automatic step draws every one of them.
fn five(kind: ChartKind) -> ChartData {
    let mut s = series("S", &[2.0, 4.0, 6.0, 8.0, 10.0], BLUE, kind);
    s.number_format = "General".into();
    ChartData { category_levels: vec![], palette: vec![], categories: (1..=5).map(|i| format!("Cat{i}")).collect(), series: vec![s] }
}

/// The category labels drawn, in drawing order.
fn cat_labels(kind: ChartKind, skip: Option<u32>) -> Vec<String> {
    let p = render(&plain(kind, skip), &five(kind), 480.0, 288.0, &ApproxMeasure);
    let mut v: Vec<String> = texts(&p).into_iter().filter(|t| t.starts_with("Cat")).map(str::to_string).collect();
    v.sort();
    v
}

fn names(ix: &[usize]) -> Vec<String> {
    ix.iter().map(|i| format!("Cat{i}")).collect()
}

#[test]
fn automatic_interval_draws_every_short_label() {
    for kind in [ChartKind::ColumnClustered, ChartKind::Line, ChartKind::BarClustered] {
        assert_eq!(cat_labels(kind, None), names(&[1, 2, 3, 4, 5]), "{kind:?}");
    }
}

#[test]
fn column_chart_interval_three_labels_first_and_fourth() {
    // XlsxWriter chart_axis40 in Excel: "1   4".
    assert_eq!(cat_labels(ChartKind::ColumnClustered, Some(3)), names(&[1, 4]));
}

#[test]
fn line_chart_interval_two_labels_odd_categories() {
    // XlsxWriter chart_axis34 in Excel: "1 3 5".
    assert_eq!(cat_labels(ChartKind::Line, Some(2)), names(&[1, 3, 5]));
    assert_eq!(cat_labels(ChartKind::Area, Some(2)), names(&[1, 3, 5]));
}

#[test]
fn bar_chart_interval_applies_to_its_vertical_category_axis() {
    assert_eq!(cat_labels(ChartKind::BarClustered, Some(2)), names(&[1, 3, 5]));
}

#[test]
fn interval_of_one_draws_all_and_a_huge_interval_only_the_first() {
    assert_eq!(cat_labels(ChartKind::ColumnClustered, Some(1)), names(&[1, 2, 3, 4, 5]));
    assert_eq!(cat_labels(ChartKind::ColumnClustered, Some(10)), names(&[1]));
    assert_eq!(cat_labels(ChartKind::ColumnClustered, Some(u32::MAX)), names(&[1]));
}
