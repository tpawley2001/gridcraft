//! A conditional format's border side with no line draws nothing, whatever colour it names, so the
//! round-trip check must not count a changed colour on such a side as a lost rule. A side that has a
//! line keeps its colour in the comparison.
#![allow(clippy::unwrap_used)]

use gridcraft_engine::core::RangeRef;
use gridcraft_engine::model::{BorderLine, BorderStyle, CfOperator, CfRule, Color, CondFormat, Fill, Style};

use super::*;

fn line(style: BorderStyle, color: Color) -> BorderLine {
    BorderLine { style, color }
}

fn green() -> Style {
    Style { fill: Fill::solid(Color::Rgb(0x00FF00)), ..Style::default() }
}

/// The four outer sides set to `side`.
fn with_sides(mut s: Style, side: BorderLine) -> Style {
    s.borders.left = side;
    s.borders.right = side;
    s.borders.top = side;
    s.borders.bottom = side;
    s
}

fn book_with(style: Style) -> Workbook {
    let mut wb = Workbook::new();
    let sh = wb.sheet_mut(0).unwrap();
    sh.set_cell(CellRef::parse("A1").unwrap(), Cell::value(Value::text("green")));
    sh.cond_formats.push(CondFormat {
        pivot: false,
        ranges: vec![RangeRef::parse("A1:A5").unwrap()],
        rule: CfRule::CellIs { op: CfOperator::Equal, a: "\"green\"".into(), b: None, style: Box::new(style) },
        priority: 1,
        stop_if_true: false,
    });
    wb
}

fn cf_diffs(a: &Workbook, b: &Workbook) -> usize {
    diff_workbooks(a, b, &Options::default()).into_iter().find(|c| c.name == "condFormats").unwrap().diffs
}

#[test]
fn a_side_with_no_line_compares_equal_whatever_its_colour() {
    let black_none = with_sides(green(), line(BorderStyle::None, Color::Rgb(0)));
    assert_eq!(cf_diffs(&book_with(black_none), &book_with(green())), 0);
    let theme_none = with_sides(green(), line(BorderStyle::None, Color::Theme(1, 0)));
    assert_eq!(cf_diffs(&book_with(green()), &book_with(theme_none)), 0);
}

#[test]
fn a_side_with_a_line_still_compares_its_colour() {
    let mut black = green();
    black.borders.left = line(BorderStyle::Thin, Color::Rgb(0));
    let mut auto = green();
    auto.borders.left = line(BorderStyle::Thin, Color::Auto);
    assert_eq!(cf_diffs(&book_with(black), &book_with(auto)), 1);
}

#[test]
fn other_style_changes_are_still_losses() {
    let red = Style { fill: Fill::solid(Color::Rgb(0xFF0000)), ..Style::default() };
    let none = line(BorderStyle::None, Color::Rgb(0));
    assert_eq!(cf_diffs(&book_with(with_sides(green(), none)), &book_with(with_sides(red, none))), 1);
}

#[test]
fn saving_a_rule_whose_empty_sides_name_a_colour_is_clean() {
    let wb = book_with(with_sides(green(), line(BorderStyle::None, Color::Rgb(0))));
    let source = gridcraft_engine::io::save_bytes(&wb, "t.xlsx").unwrap();
    let rt = round_trip(&wb, &source, &Options::default());
    assert_eq!(rt.error, None);
    let cf = rt.categories.iter().find(|c| c.name == "condFormats").unwrap();
    assert_eq!(cf.diffs, 0, "{:?}", cf.examples);
}
