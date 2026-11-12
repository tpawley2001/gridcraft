//! A table that names one of the workbook's custom table styles (ECMA-376 Part 1 §18.8.40
//! `tableStyle`, §18.8.41 `tableStyleElement`, §18.18.83 `ST_TableStyleType`) is drawn from that
//! style's elements, not from a built-in look.

use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_engine::{Session, tables::table_cell_style};
use gridcraft_model::{BorderLine, BorderStyle, Borders, Color, CustomTableStyle, Fill, PatternType, Style, Table, TableStyleElement, Workbook};
use serde_json::json;

const WHOLE: Color = Color::Rgb(0xDDEEFF);
const INK: Color = Color::Rgb(0x112233);
const HEAD: Color = Color::Rgb(0x203040);
const BAND_A: Color = Color::Rgb(0xAA0000);
const BAND_B: Color = Color::Rgb(0x00BB00);

fn dxf(set: impl FnOnce(&mut Style)) -> Option<Style> {
    let mut s = Style::default();
    set(&mut s);
    Some(s)
}

fn fill(c: Color) -> Option<Style> {
    dxf(|s| s.fill = Fill::solid(c))
}

fn el(kind: &str, size: u32, dxf: Option<Style>) -> TableStyleElement {
    TableStyleElement { kind: kind.to_string(), size, dxf }
}

fn line(style: BorderStyle, rgb: u32) -> BorderLine {
    BorderLine { style, color: Color::Rgb(rgb) }
}

/// A workbook with a three-column table at A1:C8 (a header row and seven data rows) that uses
/// the custom style `name` made of `elements`. Returns the workbook and a copy of the table.
fn setup(name: &str, elements: Vec<TableStyleElement>) -> (Workbook, Table) {
    let mut s = Session::new();
    s.new_workbook();
    let mut rows = vec![vec!["Item".to_string(), "Qty".to_string(), "Note".to_string()]];
    for i in 1..=7 {
        rows.push(vec![format!("i{i}"), i.to_string(), format!("n{i}")]);
    }
    s.execute("range.setValues", json!({"range":"A1", "values": rows})).unwrap();
    s.execute("insert.table", json!({"range":"A1:C8", "header":true, "name":"T"})).unwrap();
    let wb = Arc::make_mut(&mut s.doc_mut().unwrap().wb);
    wb.table_styles.styles.push(CustomTableStyle { name: name.to_string(), pivot: false, table: true, elements });
    let sh = wb.sheet_mut(0).unwrap();
    let t = sh.tables.iter_mut().find(|t| t.name == "T").unwrap();
    t.style = name.to_string();
    t.banded_rows = true;
    t.banded_cols = false;
    t.first_col = false;
    t.last_col = false;
    let t = t.clone();
    (wb.clone(), t)
}

fn at(wb: &Workbook, t: &Table, cell: &str) -> Style {
    table_cell_style(wb, 0, t, CellRef::parse(cell).unwrap())
}

#[test]
fn whole_table_formats_every_cell_and_no_built_in_look_is_added() {
    let (wb, mut t) = setup(
        "My Style",
        vec![el(
            "wholeTable",
            1,
            dxf(|s| {
                s.fill = Fill::solid(WHOLE);
                s.font.color = INK;
            }),
        )],
    );
    for cell in ["A1", "C1", "B4", "A8", "C8"] {
        let s = at(&wb, &t, cell);
        assert_eq!(s.fill, Fill::solid(WHOLE), "{cell} fill");
        assert_eq!(s.font.color, INK, "{cell} font colour");
        assert!(!s.font.bold, "{cell}: the custom style sets no bold");
        assert_eq!(s.borders, Borders::default(), "{cell}: the custom style sets no borders");
    }
    // A total row with no totalRow element gets nothing of its own.
    t.totals_row = true;
    let s = at(&wb, &t, "B8");
    assert_eq!(s.fill, Fill::solid(WHOLE));
    assert!(!s.font.bold);
    assert_eq!(s.borders.top.style, BorderStyle::None);
}

#[test]
fn a_custom_style_named_like_a_built_in_one_is_still_the_custom_one() {
    let (wb, t) = setup("TableStyleDark1 2", vec![el("wholeTable", 1, fill(WHOLE))]);
    let head = at(&wb, &t, "A1");
    assert_eq!(head.fill, Fill::solid(WHOLE));
    assert!(!head.font.bold);
    assert_eq!(head.font.color, Color::Auto);
    let body = at(&wb, &t, "B3");
    assert_eq!(body.fill, Fill::solid(WHOLE));
    assert_eq!(body.font.color, Color::Auto, "no Dark-family white text on a custom style");
}

#[test]
fn header_row_lies_over_the_whole_table_property_by_property() {
    let (wb, mut t) = setup(
        "My Style",
        vec![
            el(
                "wholeTable",
                1,
                dxf(|s| {
                    s.fill = Fill::solid(WHOLE);
                    s.font.color = INK;
                }),
            ),
            el(
                "headerRow",
                1,
                dxf(|s| {
                    s.fill = Fill::solid(HEAD);
                    s.font.bold = true;
                }),
            ),
        ],
    );
    for cell in ["A1", "B1", "C1"] {
        let s = at(&wb, &t, cell);
        assert_eq!(s.fill, Fill::solid(HEAD), "{cell}");
        assert!(s.font.bold, "{cell}");
        assert_eq!(s.font.color, INK, "{cell}: the header sets no font colour, so the whole-table one shows");
    }
    let body = at(&wb, &t, "A2");
    assert_eq!(body.fill, Fill::solid(WHOLE));
    assert!(!body.font.bold);
    // Without a header row, row 1 is a data row.
    t.header_row = false;
    let first = at(&wb, &t, "A1");
    assert_eq!(first.fill, Fill::solid(WHOLE));
    assert!(!first.font.bold);
}

#[test]
fn row_stripes_follow_their_sizes() {
    let (wb, mut t) =
        setup("My Style", vec![el("wholeTable", 1, fill(WHOLE)), el("firstRowStripe", 2, fill(BAND_A)), el("secondRowStripe", 1, fill(BAND_B))]);
    // Data rows are sheet rows 2..=8: two rows of the first stripe, one of the second, repeating.
    let want = [("B2", BAND_A), ("B3", BAND_A), ("B4", BAND_B), ("B5", BAND_A), ("B6", BAND_A), ("B7", BAND_B), ("B8", BAND_A)];
    for (cell, colour) in want {
        assert_eq!(at(&wb, &t, cell).fill, Fill::solid(colour), "{cell}");
    }
    assert_eq!(at(&wb, &t, "B1").fill, Fill::solid(WHOLE), "the header row is not striped");
    t.banded_rows = false;
    for (cell, _) in want {
        assert_eq!(at(&wb, &t, cell).fill, Fill::solid(WHOLE), "{cell} with banded rows off");
    }
}

#[test]
fn a_missing_second_stripe_leaves_those_rows_alone() {
    let (wb, t) = setup("My Style", vec![el("firstRowStripe", 1, fill(BAND_A))]);
    assert_eq!(at(&wb, &t, "A2").fill, Fill::solid(BAND_A));
    assert_eq!(at(&wb, &t, "A3").fill.pattern, PatternType::None);
    assert_eq!(at(&wb, &t, "A4").fill, Fill::solid(BAND_A));
    assert_eq!(at(&wb, &t, "A5").fill.pattern, PatternType::None);
}

#[test]
fn column_stripes_need_banded_columns() {
    let (wb, mut t) = setup("My Style", vec![el("firstColumnStripe", 1, fill(BAND_A)), el("secondColumnStripe", 1, fill(BAND_B))]);
    t.banded_rows = false;
    for cell in ["A3", "B3", "C3"] {
        assert_eq!(at(&wb, &t, cell).fill.pattern, PatternType::None, "{cell} with banded columns off");
    }
    t.banded_cols = true;
    assert_eq!(at(&wb, &t, "A3").fill, Fill::solid(BAND_A));
    assert_eq!(at(&wb, &t, "B3").fill, Fill::solid(BAND_B));
    assert_eq!(at(&wb, &t, "C3").fill, Fill::solid(BAND_A));
    assert_eq!(at(&wb, &t, "C8").fill, Fill::solid(BAND_A));
}

#[test]
fn first_and_last_column_apply_only_when_switched_on() {
    let (wb, mut t) = setup("My Style", vec![el("firstColumn", 1, dxf(|s| s.font.bold = true)), el("lastColumn", 1, dxf(|s| s.font.color = INK))]);
    assert!(!at(&wb, &t, "A3").font.bold);
    assert_eq!(at(&wb, &t, "C3").font.color, Color::Auto);
    t.first_col = true;
    t.last_col = true;
    assert!(at(&wb, &t, "A3").font.bold);
    assert!(!at(&wb, &t, "B3").font.bold);
    assert!(!at(&wb, &t, "C3").font.bold);
    assert_eq!(at(&wb, &t, "C3").font.color, INK);
    assert_eq!(at(&wb, &t, "B3").font.color, Color::Auto);
    assert_eq!(at(&wb, &t, "A3").font.color, Color::Auto);
}

#[test]
fn total_row_element_formats_the_total_row() {
    let top = line(BorderStyle::Double, 0x445566);
    let (wb, mut t) = setup(
        "My Style",
        vec![el(
            "totalRow",
            1,
            dxf(|s| {
                s.font.bold = true;
                s.borders.top = top;
            }),
        )],
    );
    // No total row: row 8 is a data row.
    assert!(!at(&wb, &t, "B8").font.bold);
    assert_eq!(at(&wb, &t, "B8").borders.top.style, BorderStyle::None);
    t.totals_row = true;
    for cell in ["A8", "B8", "C8"] {
        let s = at(&wb, &t, cell);
        assert!(s.font.bold, "{cell}");
        assert_eq!(s.borders.top, top, "{cell}");
    }
    let above = at(&wb, &t, "B7");
    assert!(!above.font.bold);
    assert_eq!(above.borders, Borders::default());
}

#[test]
fn borders_go_to_the_edges_of_the_element_and_inside_lines_between_its_cells() {
    let outer = line(BorderStyle::Thin, 0x010101);
    let inner_h = line(BorderStyle::Hair, 0x020202);
    let inner_v = line(BorderStyle::Dotted, 0x030303);
    let under = line(BorderStyle::Medium, 0x040404);
    let none = BorderLine::default();
    let (wb, t) = setup(
        "My Style",
        vec![
            el(
                "wholeTable",
                1,
                dxf(|s| {
                    s.borders.left = outer;
                    s.borders.right = outer;
                    s.borders.top = outer;
                    s.borders.bottom = outer;
                    s.borders.horizontal = inner_h;
                    s.borders.vertical = inner_v;
                }),
            ),
            el("headerRow", 1, dxf(|s| s.borders.bottom = under)),
        ],
    );
    // (cell, left, right, top, bottom)
    let want = [
        ("A1", outer, inner_v, outer, under),
        ("B1", none, inner_v, outer, under),
        ("C1", none, outer, outer, under),
        ("A2", outer, inner_v, none, inner_h),
        ("B4", none, inner_v, none, inner_h),
        ("C4", none, outer, none, inner_h),
        ("A8", outer, inner_v, none, outer),
        ("C8", none, outer, none, outer),
    ];
    for (cell, left, right, top, bottom) in want {
        let b = at(&wb, &t, cell).borders;
        assert_eq!(b.left, left, "{cell} left");
        assert_eq!(b.right, right, "{cell} right");
        assert_eq!(b.top, top, "{cell} top");
        assert_eq!(b.bottom, bottom, "{cell} bottom");
    }
}

#[test]
fn a_cells_own_fill_and_font_colour_win() {
    let (mut wb, t) = setup(
        "My Style",
        vec![el(
            "wholeTable",
            1,
            dxf(|s| {
                s.fill = Fill::solid(WHOLE);
                s.font.color = INK;
            }),
        )],
    );
    let own = Style {
        fill: Fill::solid(Color::Rgb(0xFFFF00)),
        font: gridcraft_model::Font { color: Color::Rgb(0xFF0000), ..Default::default() },
        ..Default::default()
    };
    let id = wb.styles.intern(own);
    wb.sheet_mut(0).unwrap().set_style(CellRef::parse("B3").unwrap(), id);
    let s = at(&wb, &t, "B3");
    assert_eq!(s.fill, Fill::solid(Color::Rgb(0xFFFF00)));
    assert_eq!(s.font.color, Color::Rgb(0xFF0000));
    assert_eq!(at(&wb, &t, "B4").fill, Fill::solid(WHOLE));
}

#[test]
fn built_in_styles_are_unchanged_when_the_workbook_has_custom_styles() {
    let (wb, mut t) = setup("My Style", vec![el("wholeTable", 1, fill(WHOLE))]);
    t.style = "TableStyleMedium2".to_string();
    let mut plain = wb.clone();
    plain.table_styles.styles.clear();
    for cell in ["A1", "B2", "B3", "C8"] {
        assert_eq!(at(&wb, &t, cell), at(&plain, &t, cell), "{cell}");
    }
    assert!(at(&wb, &t, "A1").font.bold, "the built-in header stays bold");
}
